use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Context, Result};

use crate::{
    model::{normalize_skill_name, normalized_path_text},
    path_utils::resolve_existing_ancestors,
};

#[derive(Debug, Clone)]
pub struct MovePlan {
    pub from: PathBuf,
    pub to: PathBuf,
}

impl MovePlan {
    pub fn new(from: impl Into<PathBuf>, to: impl Into<PathBuf>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
        }
    }
}

pub fn move_paths_transactionally(plans: &[MovePlan]) -> Result<()> {
    preflight_moves(plans)?;

    let mut completed = Vec::<MovePlan>::new();
    for plan in plans {
        if let Some(parent) = plan.to.parent() {
            if let Err(error) = fs::create_dir_all(parent) {
                let rollback_errors = rollback_moves(&completed);
                return Err(rollback_error(
                    error.into(),
                    format!(
                        "failed to create destination directory {}",
                        parent.display()
                    ),
                    rollback_errors,
                ));
            }
        }

        if let Err(error) = fs::rename(&plan.from, &plan.to) {
            let rollback_errors = rollback_moves(&completed);
            return Err(rollback_error(
                error.into(),
                format!(
                    "failed to move {} to {}",
                    plan.from.display(),
                    plan.to.display()
                ),
                rollback_errors,
            ));
        }
        completed.push(plan.clone());
    }

    Ok(())
}

pub fn cleanup_empty_parents(start: Option<&Path>, stop_at: &Path) {
    let mut current = start.map(Path::to_path_buf);
    while let Some(directory) = current {
        if directory == stop_at || !directory.starts_with(stop_at) {
            break;
        }

        let is_empty = fs::read_dir(&directory)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false);
        if !is_empty || fs::remove_dir(&directory).is_err() {
            break;
        }
        current = directory.parent().map(Path::to_path_buf);
    }
}

pub fn remove_path(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect {}", path.display()))?;

    if metadata.file_type().is_symlink() || metadata.is_file() {
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))
    } else if metadata.is_dir() {
        fs::remove_dir_all(path).with_context(|| format!("failed to remove {}", path.display()))
    } else {
        bail!("unsupported filesystem entry: {}", path.display())
    }
}

pub fn copy_directory_secure(source: &Path, destination: &Path) -> Result<()> {
    if path_exists(destination) {
        bail!("destination already exists: {}", destination.display());
    }

    let source_root = source
        .canonicalize()
        .with_context(|| format!("failed to resolve source {}", source.display()))?;
    if !source_root.is_dir() {
        bail!("skill source is not a directory: {}", source.display());
    }

    let destination_absolute = absolute_destination(destination)?;
    if destination_absolute.starts_with(&source_root) {
        bail!(
            "refused to copy a skill into itself: {} is inside {}",
            destination.display(),
            source.display()
        );
    }

    if let Some(parent) = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::create_dir(destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;

    let mut visited = BTreeSet::new();
    if let Err(error) = copy_directory_inner(&source_root, &source_root, destination, &mut visited)
    {
        let cleanup_error = remove_path(destination).err();
        return match cleanup_error {
            Some(cleanup_error) => Err(anyhow!(
                "{error:#}; failed to remove incomplete destination {}: {cleanup_error:#}",
                destination.display()
            )),
            None => Err(error),
        };
    }

    Ok(())
}

fn absolute_destination(destination: &Path) -> Result<PathBuf> {
    let absolute = if destination.is_absolute() {
        destination.to_path_buf()
    } else {
        std::env::current_dir()
            .context("failed to determine current directory")?
            .join(destination)
    };
    Ok(resolve_existing_ancestors(absolute))
}

pub fn sanitize_install_name(value: &str) -> String {
    let mut name = normalize_skill_name(value);
    if name.is_empty() || name == "." || name == ".." {
        name = "skill".to_owned();
    }

    name = name.trim_matches(['.', ' ']).to_owned();
    if name.is_empty() {
        name = "skill".to_owned();
    }

    let upper = name.to_ascii_uppercase();
    let reserved = matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || matches!(
            upper.as_str(),
            "COM1"
                | "COM2"
                | "COM3"
                | "COM4"
                | "COM5"
                | "COM6"
                | "COM7"
                | "COM8"
                | "COM9"
                | "LPT1"
                | "LPT2"
                | "LPT3"
                | "LPT4"
                | "LPT5"
                | "LPT6"
                | "LPT7"
                | "LPT8"
                | "LPT9"
        );
    if reserved {
        name.insert(0, '_');
    }

    name.chars().take(80).collect()
}

pub fn path_exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

pub fn preflight_moves(plans: &[MovePlan]) -> Result<()> {
    let mut sources = BTreeSet::new();
    let mut destinations = BTreeSet::new();
    for plan in plans {
        if !path_exists(&plan.from) {
            bail!("source no longer exists: {}", plan.from.display());
        }
        if path_exists(&plan.to) {
            bail!(
                "destination already exists: {}; resolve the conflict before continuing",
                plan.to.display()
            );
        }

        let source_key = normalized_path_text(&plan.from);
        if !sources.insert(source_key) {
            bail!("multiple operations move {}", plan.from.display());
        }

        let destination_key = normalized_path_text(&plan.to);
        if !destinations.insert(destination_key) {
            bail!("multiple operations target {}", plan.to.display());
        }
    }
    Ok(())
}

fn rollback_moves(completed: &[MovePlan]) -> Vec<String> {
    let mut errors = Vec::new();
    for plan in completed.iter().rev() {
        if let Some(parent) = plan.from.parent() {
            if let Err(error) = fs::create_dir_all(parent) {
                errors.push(format!(
                    "could not recreate {} during rollback: {error}",
                    parent.display()
                ));
                continue;
            }
        }
        if let Err(error) = fs::rename(&plan.to, &plan.from) {
            errors.push(format!(
                "could not restore {} from {}: {error}",
                plan.from.display(),
                plan.to.display()
            ));
        }
    }
    errors
}

pub fn rollback_error(
    error: anyhow::Error,
    context: String,
    rollback_errors: Vec<String>,
) -> anyhow::Error {
    let error = error.context(context);
    if rollback_errors.is_empty() {
        error
    } else {
        anyhow!(
            "{error:#}; rollback also reported: {}",
            rollback_errors.join("; ")
        )
    }
}

fn copy_directory_inner(
    source_root: &Path,
    source: &Path,
    destination: &Path,
    visited: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    let canonical_source = source
        .canonicalize()
        .with_context(|| format!("failed to resolve {}", source.display()))?;
    if !canonical_source.starts_with(source_root) {
        bail!(
            "refused to copy symlink outside skill directory: {}",
            source.display()
        );
    }
    if !visited.insert(canonical_source.clone()) {
        bail!("symlink cycle detected at {}", source.display());
    }

    fs::create_dir_all(destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;

    for entry in fs::read_dir(&canonical_source)
        .with_context(|| format!("failed to read {}", canonical_source.display()))?
    {
        let entry = entry?;
        let file_name = entry.file_name();
        if file_name.to_string_lossy().eq_ignore_ascii_case(".git") {
            continue;
        }

        let source_path = entry.path();
        let destination_path = destination.join(&file_name);
        let metadata = fs::symlink_metadata(&source_path)
            .with_context(|| format!("failed to inspect {}", source_path.display()))?;

        if metadata.file_type().is_symlink() {
            let target = source_path
                .canonicalize()
                .with_context(|| format!("failed to resolve symlink {}", source_path.display()))?;
            if !target.starts_with(source_root) {
                bail!(
                    "refused to copy symlink outside skill directory: {}",
                    source_path.display()
                );
            }
            if target.is_dir() {
                copy_directory_inner(source_root, &target, &destination_path, visited)?;
            } else if target.is_file() {
                copy_file(&target, &destination_path)?;
            } else {
                bail!("unsupported symlink target: {}", source_path.display());
            }
        } else if metadata.is_dir() {
            copy_directory_inner(source_root, &source_path, &destination_path, visited)?;
        } else if metadata.is_file() {
            copy_file(&source_path, &destination_path)?;
        } else {
            return Err(anyhow!(
                "unsupported filesystem entry in skill: {}",
                source_path.display()
            ));
        }
    }

    visited.remove(&canonical_source);
    Ok(())
}

fn copy_file(source: &Path, destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, destination).with_context(|| {
        format!(
            "failed to copy {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(())
}
