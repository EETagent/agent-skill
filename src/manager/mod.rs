use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Result};
use chrono::Utc;

mod install;
mod query;

pub use install::{InstallChange, InstallReport, InstallTarget};
pub use query::SkillQuery;

use crate::{
    discovery::discover_catalog,
    fsops::{
        cleanup_empty_parents, move_paths_transactionally, path_exists, preflight_moves,
        remove_path, MovePlan,
    },
    model::{
        normalized_path_text, Catalog, RemoveChange, Skill, SkillChange, SkillRoot, SkillState,
    },
    path_utils::ensure_safe_relative_path,
    paths::PathRegistry,
    remote::{RemoteClient, RemoteSkill},
    source::{prepare_source, PreparedSource},
};

#[derive(Debug, Clone)]
pub struct SkillManager {
    registry: PathRegistry,
    // Local-only commands should not fail because SKILLS_API_URL is invalid.
    // Tests and embedders may still inject a fixed client with `from_parts`.
    remote: Option<RemoteClient>,
}

impl SkillManager {
    pub fn new(project_root: Option<PathBuf>, extra_roots: Vec<PathBuf>) -> Result<Self> {
        Self::with_roots(project_root, extra_roots, Vec::new())
    }

    pub fn with_roots(
        project_root: Option<PathBuf>,
        extra_local_roots: Vec<PathBuf>,
        extra_global_roots: Vec<PathBuf>,
    ) -> Result<Self> {
        let registry =
            PathRegistry::discover_with_roots(project_root, extra_local_roots, extra_global_roots)?;
        Ok(Self {
            registry,
            remote: None,
        })
    }

    pub fn from_registry(registry: PathRegistry) -> Self {
        Self {
            registry,
            remote: None,
        }
    }

    pub fn from_parts(registry: PathRegistry, remote: RemoteClient) -> Self {
        Self {
            registry,
            remote: Some(remote),
        }
    }

    pub fn registry(&self) -> &PathRegistry {
        &self.registry
    }

    pub fn refresh(&self) -> Catalog {
        discover_catalog(self.registry.roots())
    }

    pub fn prepare_source(&self, source: &str, full_depth: bool) -> Result<PreparedSource> {
        prepare_source(source, full_depth)
    }

    pub fn search_remote(
        &self,
        query: &str,
        owner: Option<&str>,
        limit: usize,
    ) -> Result<Vec<RemoteSkill>> {
        match &self.remote {
            Some(remote) => remote.search(query, owner, limit),
            None => RemoteClient::from_environment()?.search(query, owner, limit),
        }
    }

    pub fn plan_state_change(
        &self,
        skills: &[Skill],
        target_state: SkillState,
    ) -> Result<Vec<SkillChange>> {
        let mut changes = Vec::new();
        let mut seen_sources = BTreeSet::new();

        for skill in skills {
            if skill.state == target_state
                || !seen_sources.insert(normalized_path_text(&skill.path))
            {
                continue;
            }

            let root = self.root_for(skill)?;
            validate_skill_relative_path(&skill.relative_path)?;
            changes.push(SkillChange {
                skill: skill.title.clone(),
                from: skill.path.clone(),
                to: root.store_path(target_state).join(&skill.relative_path),
            });
        }

        preflight_moves(&state_change_plans(&changes))?;
        Ok(changes)
    }

    pub fn apply_state_change(&self, changes: &[SkillChange]) -> Result<()> {
        move_paths_transactionally(&state_change_plans(changes))?;
        self.cleanup_sources(changes.iter().map(|change| change.from.as_path()));
        Ok(())
    }

    pub fn change_state(
        &self,
        skills: &[Skill],
        target_state: SkillState,
    ) -> Result<Vec<SkillChange>> {
        let changes = self.plan_state_change(skills, target_state)?;
        self.apply_state_change(&changes)?;
        Ok(changes)
    }

    pub fn plan_remove(&self, skills: &[Skill], purge: bool) -> Result<Vec<RemoveChange>> {
        let timestamp = operation_timestamp();
        let mut reserved_destinations = BTreeSet::new();
        let mut seen_sources = BTreeSet::new();
        let mut changes = Vec::new();

        for skill in skills {
            if !seen_sources.insert(normalized_path_text(&skill.path)) {
                continue;
            }
            if !path_exists(&skill.path) {
                bail!("skill no longer exists: {}", skill.path.display());
            }

            let root = self.root_for(skill)?;
            validate_skill_relative_path(&skill.relative_path)?;
            let destination = (!purge).then(|| {
                let preferred = root
                    .trash_path
                    .join(&timestamp)
                    .join(skill.state.label())
                    .join(&skill.relative_path);
                reserve_unique_path(preferred, &mut reserved_destinations)
            });

            changes.push(RemoveChange {
                skill: skill.title.clone(),
                from: skill.path.clone(),
                destination,
                purged: purge,
            });
        }

        Ok(changes)
    }

    pub fn apply_remove(&self, changes: &[RemoveChange]) -> Result<()> {
        let Some(first_change) = changes.first() else {
            return Ok(());
        };
        if changes
            .iter()
            .any(|change| change.purged != first_change.purged)
        {
            bail!("cannot mix purge and reversible remove operations");
        }

        if first_change.purged {
            for change in changes {
                remove_path(&change.from)?;
            }
        } else {
            let plans = changes
                .iter()
                .map(|change| {
                    let destination = change
                        .destination
                        .clone()
                        .ok_or_else(|| anyhow!("remove destination is missing"))?;
                    Ok(MovePlan::new(change.from.clone(), destination))
                })
                .collect::<Result<Vec<_>>>()?;
            move_paths_transactionally(&plans)?;
        }

        self.cleanup_sources(changes.iter().map(|change| change.from.as_path()));
        Ok(())
    }

    pub fn remove(&self, skills: &[Skill], purge: bool) -> Result<Vec<RemoveChange>> {
        let changes = self.plan_remove(skills, purge)?;
        self.apply_remove(&changes)?;
        Ok(changes)
    }

    pub fn install(
        &self,
        prepared: &PreparedSource,
        selected_indices: &[usize],
        target: &InstallTarget,
    ) -> Result<InstallReport> {
        install::execute(&self.registry, prepared, selected_indices, target)
    }

    fn root_for(&self, skill: &Skill) -> Result<&SkillRoot> {
        self.registry.root_by_id(&skill.root_id).ok_or_else(|| {
            anyhow!(
                "skill root is no longer registered: {}",
                skill.root_path.display()
            )
        })
    }

    fn cleanup_sources<'a>(&self, paths: impl IntoIterator<Item = &'a Path>) {
        for path in paths {
            if let Some(store) = self.containing_store(path) {
                cleanup_empty_parents(path.parent(), store);
            }
        }
    }

    fn containing_store<'a>(&'a self, path: &Path) -> Option<&'a Path> {
        let mut best_match = None;

        for root in self.registry.roots() {
            for state in [SkillState::Enabled, SkillState::Disabled] {
                let store = root.store_path(state);
                if !path.starts_with(store) {
                    continue;
                }

                let should_replace = best_match
                    .map(|current: &Path| store.components().count() > current.components().count())
                    .unwrap_or(true);
                if should_replace {
                    best_match = Some(store);
                }
            }
        }

        best_match
    }
}

fn state_change_plans(changes: &[SkillChange]) -> Vec<MovePlan> {
    changes
        .iter()
        .map(|change| MovePlan::new(change.from.clone(), change.to.clone()))
        .collect()
}

fn validate_skill_relative_path(path: &Path) -> Result<()> {
    ensure_safe_relative_path(path, "skill relative path")
}

pub(super) fn reserve_unique_path(preferred: PathBuf, reserved: &mut BTreeSet<String>) -> PathBuf {
    let parent = preferred.parent().unwrap_or(Path::new("."));
    let file_name = preferred
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("entry");
    let mut suffix = 0usize;

    loop {
        let candidate = if suffix == 0 {
            preferred.clone()
        } else {
            parent.join(format!("{file_name}-{suffix}"))
        };
        let key = normalized_path_text(&candidate);
        if !path_exists(&candidate) && reserved.insert(key) {
            return candidate;
        }
        suffix = suffix.saturating_add(1);
    }
}

pub(super) fn operation_timestamp() -> String {
    Utc::now().format("%Y%m%dT%H%M%S%.3fZ").to_string()
}
