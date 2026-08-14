use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Context, Result};

mod agents;

pub use agents::{AgentSpec, GlobalLocation};

use crate::{
    model::{normalized_path_text, Scope, SkillRoot},
    path_utils::resolve_existing_ancestors,
};
use agents::{AGENTS, LEGACY_LOCAL_ROOTS};

#[derive(Debug, Clone)]
pub struct Environment {
    pub project_root: PathBuf,
    pub home_dir: PathBuf,
    /// XDG-style config root used by agents such as Amp, Goose, and OpenCode.
    pub config_home: PathBuf,
    /// Native platform config root (AppData on Windows, Application Support on macOS).
    pub native_config_home: Option<PathBuf>,
}

impl Environment {
    pub fn discover(project_root: Option<PathBuf>) -> Result<Self> {
        let home_dir =
            dirs::home_dir().ok_or_else(|| anyhow!("could not determine home directory"))?;
        let project_root = resolve_project_root(project_root)?;
        let config_home = env::var_os("XDG_CONFIG_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .unwrap_or_else(|| home_dir.join(".config"));

        let native_config_home = dirs::config_dir();

        Ok(Self {
            project_root,
            home_dir,
            config_home,
            native_config_home,
        })
    }
}

#[derive(Debug, Clone)]
pub struct PathRegistry {
    environment: Environment,
    roots: Vec<SkillRoot>,
}

impl PathRegistry {
    pub fn discover(project_root: Option<PathBuf>, extra_roots: Vec<PathBuf>) -> Result<Self> {
        Self::discover_with_roots(project_root, extra_roots, Vec::new())
    }

    pub fn discover_with_roots(
        project_root: Option<PathBuf>,
        extra_local_roots: Vec<PathBuf>,
        extra_global_roots: Vec<PathBuf>,
    ) -> Result<Self> {
        Self::from_environment_with_roots(
            Environment::discover(project_root)?,
            extra_local_roots,
            extra_global_roots,
        )
    }

    pub fn from_environment(environment: Environment, extra_roots: Vec<PathBuf>) -> Result<Self> {
        Self::from_environment_with_roots(environment, extra_roots, Vec::new())
    }

    pub fn from_environment_with_roots(
        environment: Environment,
        extra_local_roots: Vec<PathBuf>,
        extra_global_roots: Vec<PathBuf>,
    ) -> Result<Self> {
        let environment = resolve_environment_paths(environment);
        let mut roots = BTreeMap::<String, SkillRoot>::new();

        for spec in AGENTS {
            add_root(
                &mut roots,
                environment.project_root.join(spec.local_skills_dir),
                Scope::Local,
                spec.display_name,
            );

            for global_path in resolve_global_paths(spec.global_skills_dir, &environment) {
                add_root(&mut roots, global_path, Scope::Global, spec.display_name);
            }
        }

        for relative in LEGACY_LOCAL_ROOTS {
            add_root(
                &mut roots,
                environment.project_root.join(relative),
                Scope::Local,
                "Legacy / alternate layout",
            );
        }

        add_eve_subagent_roots(&mut roots, &environment.project_root);

        for root in extra_local_roots {
            let root = make_absolute(root)?;
            add_root(&mut roots, root, Scope::Local, "Custom local");
        }
        for root in extra_global_roots {
            let root = make_absolute(root)?;
            add_root(&mut roots, root, Scope::Global, "Custom global");
        }

        #[cfg(unix)]
        add_root(
            &mut roots,
            PathBuf::from("/etc/codex/skills"),
            Scope::Global,
            "Codex system",
        );

        let mut roots = roots.into_values().collect::<Vec<_>>();
        for root in &mut roots {
            root.agents.sort();
            root.agents.dedup();
        }
        roots.sort_by(|left, right| {
            left.scope
                .label()
                .cmp(right.scope.label())
                .then_with(|| left.path.cmp(&right.path))
        });

        Ok(Self { environment, roots })
    }

    pub fn from_roots(environment: Environment, roots: Vec<SkillRoot>) -> Self {
        Self { environment, roots }
    }

    pub fn environment(&self) -> &Environment {
        &self.environment
    }

    pub fn roots(&self) -> &[SkillRoot] {
        &self.roots
    }

    pub fn root_by_id(&self, id: &str) -> Option<&SkillRoot> {
        self.roots.iter().find(|root| root.id == id)
    }

    pub fn target_paths(
        &self,
        scope: Scope,
        agents: &[String],
        custom_root: Option<&Path>,
    ) -> Result<Vec<PathBuf>> {
        if let Some(custom_root) = custom_root {
            let root = make_absolute(custom_root.to_path_buf())?;
            return Ok(vec![resolve_existing_ancestors(root)]);
        }

        if agents.is_empty() {
            let root = match scope {
                Scope::Local => self.environment.project_root.join(".agents/skills"),
                Scope::Global => self.environment.home_dir.join(".agents/skills"),
            };
            return Ok(vec![resolve_existing_ancestors(root)]);
        }

        let mut paths = BTreeMap::<String, PathBuf>::new();
        for requested in agents {
            let spec = find_agent(requested).ok_or_else(|| {
                anyhow!(
                    "unknown agent {requested:?}; use `skillctl paths` to inspect known targets"
                )
            })?;

            let path = match scope {
                Scope::Local => self.environment.project_root.join(spec.local_skills_dir),
                Scope::Global => preferred_global_path(spec, &self.environment)?,
            };
            let path = resolve_existing_ancestors(path);
            paths.insert(normalized_path_text(&path), path);
        }

        Ok(paths.into_values().collect())
    }
}

fn resolve_environment_paths(environment: Environment) -> Environment {
    Environment {
        project_root: resolve_existing_ancestors(environment.project_root),
        home_dir: resolve_existing_ancestors(environment.home_dir),
        config_home: resolve_existing_ancestors(environment.config_home),
        native_config_home: environment
            .native_config_home
            .map(resolve_existing_ancestors),
    }
}

pub fn agents() -> &'static [AgentSpec] {
    AGENTS
}

pub fn find_agent(name: &str) -> Option<&'static AgentSpec> {
    AGENTS.iter().find(|spec| {
        spec.key.eq_ignore_ascii_case(name) || spec.display_name.eq_ignore_ascii_case(name)
    })
}

pub fn known_local_relative_roots() -> Vec<&'static str> {
    let mut roots = BTreeSet::new();
    roots.extend(AGENTS.iter().map(|spec| spec.local_skills_dir));
    roots.extend(LEGACY_LOCAL_ROOTS.iter().copied());
    roots.into_iter().collect()
}

pub fn resolve_project_root(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(explicit) = explicit {
        let absolute = make_absolute(explicit)?;
        if !absolute.exists() {
            bail!("project root does not exist: {}", absolute.display());
        }
        if !absolute.is_dir() {
            bail!("project root is not a directory: {}", absolute.display());
        }
        return Ok(resolve_existing_ancestors(absolute));
    }

    let current = env::current_dir().context("could not determine current directory")?;
    let home = dirs::home_dir().map(resolve_existing_ancestors);
    let project_root = find_project_root_from(&current, home.as_deref()).unwrap_or(current);

    Ok(resolve_existing_ancestors(project_root))
}

fn find_project_root_from(current: &Path, home: Option<&Path>) -> Option<PathBuf> {
    let project_markers = [
        ".git",
        "Cargo.toml",
        "package.json",
        "pyproject.toml",
        "go.mod",
        "skills-lock.json",
    ];
    let skill_markers = known_local_relative_roots();

    for ancestor in current.ancestors() {
        if project_markers
            .iter()
            .any(|marker| ancestor.join(marker).exists())
        {
            return Some(ancestor.to_path_buf());
        }

        let is_home = home.is_some_and(|home| ancestor == home);
        if !is_home
            && skill_markers
                .iter()
                .filter(|marker| is_unambiguous_project_skill_layout(marker))
                .any(|marker| ancestor.join(marker).exists())
        {
            return Some(ancestor.to_path_buf());
        }
    }

    None
}

fn is_unambiguous_project_skill_layout(layout: &str) -> bool {
    layout == "agent/skills"
        || Path::new(layout)
            .components()
            .next()
            .and_then(|component| component.as_os_str().to_str())
            .is_some_and(|component| component.starts_with('.'))
}

pub fn make_absolute(path: PathBuf) -> Result<PathBuf> {
    let path = expand_tilde_path(path);
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(env::current_dir()
            .context("could not determine current directory")?
            .join(path))
    }
}

fn expand_tilde_path(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy().into_owned();
    if text == "~" {
        return dirs::home_dir().unwrap_or(path);
    }

    let relative = text.strip_prefix("~/").or_else(|| text.strip_prefix("~\\"));
    match (dirs::home_dir(), relative) {
        (Some(home), Some(relative)) => home.join(relative),
        _ => path,
    }
}

pub fn shorten_path(path: &Path, environment: &Environment) -> String {
    if environment.project_root != environment.home_dir {
        if let Ok(relative) = path.strip_prefix(&environment.project_root) {
            if relative.as_os_str().is_empty() {
                return ".".to_owned();
            }
            return format!(".{}{}", std::path::MAIN_SEPARATOR, relative.display());
        }
    }

    if let Ok(relative) = path.strip_prefix(&environment.home_dir) {
        if relative.as_os_str().is_empty() {
            return "~".to_owned();
        }
        return format!("~{}{}", std::path::MAIN_SEPARATOR, relative.display());
    }

    path.display().to_string()
}

fn add_root(
    roots: &mut BTreeMap<String, SkillRoot>,
    path: PathBuf,
    scope: Scope,
    agent_name: &str,
) {
    let path = resolve_existing_ancestors(path);
    let key = normalized_path_text(&path);
    match roots.get_mut(&key) {
        Some(root) => {
            root.agents.push(agent_name.to_owned());
            // If the project root is the home directory, a universal path such
            // as ~/.agents/skills can be discovered through both local and
            // global definitions. Keep one physical root and classify it as
            // global, matching the project-root detection rule.
            if root.scope == Scope::Local && scope == Scope::Global {
                let agents = std::mem::take(&mut root.agents);
                *root = SkillRoot::new(path, Scope::Global, agents);
            }
        }
        None => {
            roots.insert(
                key,
                SkillRoot::new(path, scope, vec![agent_name.to_owned()]),
            );
        }
    }
}

fn add_eve_subagent_roots(roots: &mut BTreeMap<String, SkillRoot>, project_root: &Path) {
    let subagents_dir = project_root.join("agent/subagents");
    let Ok(entries) = fs::read_dir(subagents_dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name().to_string_lossy().into_owned();
            add_root(
                roots,
                path.join("skills"),
                Scope::Local,
                &format!("Eve subagent: {name}"),
            );
        }
    }
}

fn resolve_global_paths(location: GlobalLocation, environment: &Environment) -> Vec<PathBuf> {
    let paths = match location {
        GlobalLocation::Home(relative) => vec![environment.home_dir.join(relative)],
        GlobalLocation::Config(relative) => {
            let mut paths = vec![environment.config_home.join(relative)];
            if let Some(native) = &environment.native_config_home {
                paths.push(native.join(relative));
            }
            paths
        }
        GlobalLocation::EnvHome {
            variable,
            fallback,
            suffix,
        } => {
            let fallback = environment.home_dir.join(fallback).join(suffix);
            let mut paths = env::var_os(variable)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .filter(|base| base.is_absolute())
                .map(|base| vec![base.join(suffix), fallback.clone()])
                .unwrap_or_else(|| vec![fallback]);
            paths.dedup();
            paths
        }
        GlobalLocation::OpenClaw => [".openclaw", ".clawdbot", ".moltbot"]
            .into_iter()
            .map(|directory| environment.home_dir.join(directory).join("skills"))
            .collect(),
        GlobalLocation::None => Vec::new(),
    };

    deduplicate_paths(paths)
}

fn deduplicate_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = BTreeSet::new();
    paths
        .into_iter()
        .filter(|path| seen.insert(normalized_path_text(path)))
        .collect()
}

fn preferred_global_path(spec: &AgentSpec, environment: &Environment) -> Result<PathBuf> {
    let paths = resolve_global_paths(spec.global_skills_dir, environment);
    let Some(default) = paths.first() else {
        bail!(
            "{} does not define a global skills directory",
            spec.display_name
        );
    };

    let preferred = if matches!(spec.global_skills_dir, GlobalLocation::OpenClaw) {
        paths
            .iter()
            .find(|path| path.exists() || path.parent().is_some_and(|parent| parent.exists()))
            .unwrap_or(default)
    } else {
        default
    };

    Ok(preferred.clone())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::find_project_root_from;

    #[test]
    fn registered_hidden_skill_layout_marks_the_project_root() {
        let temporary = tempdir().expect("temporary directory");
        let project = temporary.path().join("project");
        let nested = project.join("src/deep");
        fs::create_dir_all(project.join(".cursor/skills")).expect("Cursor skill layout");
        fs::create_dir_all(&nested).expect("nested working directory");

        assert_eq!(
            find_project_root_from(&nested, Some(temporary.path())),
            Some(project)
        );
    }
}
