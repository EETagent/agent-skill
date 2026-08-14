use std::{
    collections::BTreeSet,
    fs::{self, File},
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use serde_json::Value;

use crate::{
    frontmatter::{read_skill_document, read_skill_document_in_directory},
    io_utils::read_limited,
    model::{Catalog, InstallableSkill, Skill, SkillRoot, SkillState},
    path_utils::ensure_safe_relative_path,
    paths::known_local_relative_roots,
};

const MAX_INSTALLED_DEPTH: usize = 32;
const MAX_SOURCE_FALLBACK_DEPTH: usize = 16;
const SOURCE_CONTAINER_DEPTH: usize = 4;
const MAX_PLUGIN_MANIFEST_BYTES: usize = 2 * 1024 * 1024;

const SKIP_DIRECTORIES: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "target",
    "dist",
    "build",
    "__pycache__",
];

pub fn discover_catalog(roots: &[SkillRoot]) -> Catalog {
    let mut catalog = Catalog::default();

    for root in roots {
        for state in [SkillState::Enabled, SkillState::Disabled] {
            scan_installed_store(root, root.store_path(state), state, &mut catalog);
        }
    }

    catalog.sort();
    catalog
}

fn scan_installed_store(root: &SkillRoot, store: &Path, state: SkillState, catalog: &mut Catalog) {
    if !store.exists() {
        return;
    }

    if store.join("SKILL.md").is_file() {
        catalog.warnings.push(format!(
            "ignored container-level SKILL.md at {}; scan roots must contain skill directories",
            store.display()
        ));
    }

    let entries = match fs::read_dir(store) {
        Ok(entries) => entries,
        Err(error) => {
            catalog
                .warnings
                .push(format!("could not read {}: {error}", store.display()));
            return;
        }
    };

    let mut visited = BTreeSet::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() || should_skip(&path) {
            continue;
        }
        visit_installed_directory(root, store, &path, state, 1, &mut visited, catalog);
    }
}

fn visit_installed_directory(
    root: &SkillRoot,
    store: &Path,
    directory: &Path,
    state: SkillState,
    depth: usize,
    visited: &mut BTreeSet<PathBuf>,
    catalog: &mut Catalog,
) {
    if depth > MAX_INSTALLED_DEPTH {
        catalog.warnings.push(format!(
            "stopped scanning {} after {MAX_INSTALLED_DEPTH} levels",
            directory.display()
        ));
        return;
    }

    let is_symlink = fs::symlink_metadata(directory)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false);
    let skill_md = directory.join("SKILL.md");
    if skill_md.is_file() {
        match read_skill_document(&skill_md) {
            Ok(document) => {
                let relative_path = directory
                    .strip_prefix(store)
                    .unwrap_or(directory)
                    .to_path_buf();
                catalog.skills.push(Skill::from_metadata(
                    document,
                    directory.to_path_buf(),
                    relative_path,
                    root,
                    state,
                ));
            }
            Err(error) => catalog
                .warnings
                .push(format!("could not parse {}: {error:#}", skill_md.display())),
        }
        // A shallower skill shadows nested SKILL.md files.
        return;
    }

    if is_symlink {
        catalog.warnings.push(format!(
            "ignored linked container without a root SKILL.md at {}",
            directory.display()
        ));
        return;
    }

    let identity = match directory.canonicalize() {
        Ok(path) => path,
        Err(error) => {
            catalog.warnings.push(format!(
                "could not resolve {}: {error}",
                directory.display()
            ));
            return;
        }
    };
    if !visited.insert(identity) {
        catalog.warnings.push(format!(
            "stopped a repeated directory traversal at {}",
            directory.display()
        ));
        return;
    }

    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) => {
            catalog
                .warnings
                .push(format!("could not read {}: {error}", directory.display()));
            return;
        }
    };

    for entry in entries.flatten() {
        let child = entry.path();
        if child.is_dir() && !should_skip(&child) {
            visit_installed_directory(root, store, &child, state, depth + 1, visited, catalog);
        }
    }
}

pub fn discover_source_skills(
    base_path: &Path,
    subpath: Option<&Path>,
    full_depth: bool,
) -> Result<Vec<InstallableSkill>> {
    if !base_path.exists() {
        bail!("source does not exist: {}", base_path.display());
    }
    if !base_path.is_dir() {
        bail!("source is not a directory: {}", base_path.display());
    }

    let source_root = base_path
        .canonicalize()
        .with_context(|| format!("failed to resolve source {}", base_path.display()))?;
    let requested_path = match subpath {
        Some(subpath) => safe_join(&source_root, subpath)?,
        None => source_root.clone(),
    };
    if !requested_path.exists() {
        bail!(
            "source subpath does not exist: {}",
            requested_path.display()
        );
    }

    let search_path = requested_path
        .canonicalize()
        .with_context(|| format!("failed to resolve source path {}", requested_path.display()))?;
    if !search_path.starts_with(&source_root) {
        bail!(
            "source subpath escapes the repository: {}",
            requested_path.display()
        );
    }
    if !search_path.is_dir() {
        bail!("source path is not a directory: {}", search_path.display());
    }

    let mut skills = Vec::new();
    let mut seen = BTreeSet::new();

    if search_path.join("SKILL.md").is_file() {
        add_installable(
            &search_path,
            &search_path,
            &search_path,
            &mut skills,
            &mut seen,
        )?;
        if !full_depth {
            return Ok(skills);
        }
    }

    // Search the repository root one level deep, then all known dedicated
    // skill containers more deeply. This avoids test fixtures unless the
    // fallback/full-depth scan is needed.
    SourceScan::new(&search_path, &search_path, 1, &mut skills, &mut seen)
        .scan_children(&search_path, 1)?;

    let mut containers = known_local_relative_roots()
        .into_iter()
        .map(|relative| search_path.join(relative))
        .collect::<Vec<_>>();
    containers.extend(plugin_declared_paths(&search_path));
    containers.sort();
    containers.dedup();

    for container in containers {
        if container == search_path || !container.is_dir() {
            continue;
        }
        SourceScan::new(
            &search_path,
            &search_path,
            SOURCE_CONTAINER_DEPTH,
            &mut skills,
            &mut seen,
        )
        .scan_directory(&container, 0)?;
    }

    if skills.is_empty() || full_depth {
        let mut scan = SourceScan::new(
            &search_path,
            &search_path,
            MAX_SOURCE_FALLBACK_DEPTH,
            &mut skills,
            &mut seen,
        );
        if search_path.join("SKILL.md").is_file() {
            scan.scan_children(&search_path, 1)?;
        } else {
            scan.scan_directory(&search_path, 0)?;
        }
    }

    skills.sort_by(|left, right| {
        left.title
            .to_lowercase()
            .cmp(&right.title.to_lowercase())
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(skills)
}

struct SourceScan<'a> {
    scan_root: &'a Path,
    relative_base: &'a Path,
    max_depth: usize,
    skills: &'a mut Vec<InstallableSkill>,
    seen: &'a mut BTreeSet<PathBuf>,
    visited: BTreeSet<PathBuf>,
}

impl<'a> SourceScan<'a> {
    fn new(
        scan_root: &'a Path,
        relative_base: &'a Path,
        max_depth: usize,
        skills: &'a mut Vec<InstallableSkill>,
        seen: &'a mut BTreeSet<PathBuf>,
    ) -> Self {
        Self {
            scan_root,
            relative_base,
            max_depth,
            skills,
            seen,
            visited: BTreeSet::new(),
        }
    }

    fn scan_directory(&mut self, directory: &Path, depth: usize) -> Result<()> {
        if depth > self.max_depth || should_skip(directory) {
            return Ok(());
        }

        let canonical_directory = match directory.canonicalize() {
            Ok(path) => path,
            Err(_) => return Ok(()),
        };
        if !canonical_directory.starts_with(self.scan_root)
            || !self.visited.insert(canonical_directory.clone())
            || should_skip(&canonical_directory)
        {
            return Ok(());
        }

        if canonical_directory.join("SKILL.md").is_file() {
            add_installable(
                &canonical_directory,
                self.relative_base,
                self.scan_root,
                self.skills,
                self.seen,
            )?;
            return Ok(());
        }

        self.scan_children(&canonical_directory, depth + 1)
    }

    fn scan_children(&mut self, directory: &Path, depth: usize) -> Result<()> {
        if depth > self.max_depth {
            return Ok(());
        }

        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(_) => return Ok(()),
        };

        for entry in entries.flatten() {
            let child = entry.path();
            if child.is_dir() && !should_skip(&child) {
                self.scan_directory(&child, depth)?;
            }
        }
        Ok(())
    }
}

fn add_installable(
    directory: &Path,
    relative_base: &Path,
    scan_root: &Path,
    skills: &mut Vec<InstallableSkill>,
    seen: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    let identity = directory
        .canonicalize()
        .unwrap_or_else(|_| directory.to_path_buf());
    if !seen.insert(identity) {
        return Ok(());
    }

    let skill_md = directory.join("SKILL.md");
    let resolved_skill_md = skill_md
        .canonicalize()
        .with_context(|| format!("failed to resolve {}", skill_md.display()))?;
    if !resolved_skill_md.starts_with(scan_root) {
        bail!(
            "skill document escapes the source repository: {}",
            skill_md.display()
        );
    }

    let document = read_skill_document_in_directory(&resolved_skill_md, directory)
        .with_context(|| format!("could not parse {}", skill_md.display()))?;
    let relative_path = directory
        .strip_prefix(relative_base)
        .unwrap_or(directory)
        .to_path_buf();

    skills.push(InstallableSkill::new(
        document,
        directory.to_path_buf(),
        relative_path,
    ));
    Ok(())
}

fn plugin_declared_paths(repository_root: &Path) -> Vec<PathBuf> {
    let plugin_directory = repository_root.join(".claude-plugin");
    let mut paths = Vec::new();

    let marketplace_path = plugin_directory.join("marketplace.json");
    if let Some(raw) = read_plugin_manifest(&marketplace_path) {
        if let Ok(value) = serde_json::from_str::<Value>(&raw) {
            let plugin_root = local_manifest_path_or_current(value.pointer("/metadata/pluginRoot"));
            if let Some(plugin_root) = plugin_root {
                if let Ok(plugin_root) = safe_join(repository_root, Path::new(plugin_root)) {
                    if let Some(plugins) = value.get("plugins").and_then(Value::as_array) {
                        for plugin in plugins {
                            let Some(source) = local_manifest_path(plugin.get("source")) else {
                                // Missing or object-valued sources do not identify a local path
                                // in this repository.
                                continue;
                            };
                            if let Ok(base) = safe_join(&plugin_root, Path::new(source)) {
                                paths.push(base.join("skills"));
                                collect_skill_values(plugin.get("skills"), &base, &mut paths);
                            }
                        }
                    }
                }
            }
        }
    }

    let plugin_manifest_path = plugin_directory.join("plugin.json");
    if let Some(raw) = read_plugin_manifest(&plugin_manifest_path) {
        if let Ok(value) = serde_json::from_str::<Value>(&raw) {
            paths.push(repository_root.join("skills"));
            collect_skill_values(value.get("skills"), repository_root, &mut paths);
        }
    }

    paths
        .into_iter()
        .filter(|path| path.starts_with(repository_root))
        .collect()
}

fn read_plugin_manifest(path: &Path) -> Option<String> {
    let read = read_limited(File::open(path).ok()?, MAX_PLUGIN_MANIFEST_BYTES).ok()?;
    if read.truncated {
        return None;
    }
    String::from_utf8(read.bytes).ok()
}

fn local_manifest_path(value: Option<&Value>) -> Option<&str> {
    value.and_then(Value::as_str)
}

fn local_manifest_path_or_current(value: Option<&Value>) -> Option<&str> {
    match value {
        None => Some("."),
        Some(value) => value.as_str(),
    }
}

fn collect_skill_values(value: Option<&Value>, base: &Path, output: &mut Vec<PathBuf>) {
    match value {
        Some(Value::String(path)) => {
            if let Ok(path) = safe_join(base, Path::new(path)) {
                output.push(path);
            }
        }
        Some(Value::Array(paths)) => {
            for path in paths {
                collect_skill_values(Some(path), base, output);
            }
        }
        _ => {}
    }
}

fn safe_join(base: &Path, relative: &Path) -> Result<PathBuf> {
    ensure_safe_relative_path(relative, "source subpath")?;
    Ok(base.join(relative))
}

fn should_skip(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    let name = name.to_ascii_lowercase();
    SKIP_DIRECTORIES.iter().any(|skipped| name == *skipped)
        || (name.starts_with('.') && (name.ends_with("-disabled") || name.ends_with("-trash")))
        || name.starts_with(".skillctl-")
}
