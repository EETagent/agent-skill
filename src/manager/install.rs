use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use tempfile::TempDir;

use super::{operation_timestamp, reserve_unique_path};
use crate::{
    fsops::{
        copy_directory_secure, path_exists, remove_path, rollback_error, sanitize_install_name,
    },
    model::{normalized_path_text, InstallableSkill, Scope, SkillRoot},
    paths::PathRegistry,
    source::{redact_source, PreparedSource},
};

#[derive(Debug, Clone)]
pub struct InstallTarget {
    pub scope: Scope,
    pub agents: Vec<String>,
    pub custom_root: Option<PathBuf>,
    pub replace: bool,
}

impl Default for InstallTarget {
    fn default() -> Self {
        Self {
            scope: Scope::Local,
            agents: Vec::new(),
            custom_root: None,
            replace: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallChange {
    pub skill: String,
    pub title: String,
    #[serde(serialize_with = "crate::model::json_path::serialize")]
    pub source: PathBuf,
    #[serde(serialize_with = "crate::model::json_path::serialize")]
    pub destination: PathBuf,
    #[serde(serialize_with = "crate::model::json_path::serialize_option")]
    pub replaced: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallReport {
    pub source: String,
    pub installed: Vec<InstallChange>,
}

pub(super) fn execute(
    registry: &PathRegistry,
    prepared: &PreparedSource,
    selected_indices: &[usize],
    target: &InstallTarget,
) -> Result<InstallReport> {
    let selected = select_skills(&prepared.skills, selected_indices)?;
    let target_roots =
        registry.target_paths(target.scope, &target.agents, target.custom_root.as_deref())?;
    if target_roots.is_empty() {
        bail!("no install target was resolved");
    }

    let plans = plan_installs(&selected, &target_roots, target.scope, target.replace)?;
    let staged = stage_installs(plans)?;
    commit_installs(&staged)?;

    Ok(InstallReport {
        source: redact_source(&prepared.original),
        installed: staged.into_iter().map(StagedInstall::into_change).collect(),
    })
}

#[derive(Debug, Clone)]
struct PlannedInstall {
    skill: String,
    title: String,
    source: PathBuf,
    destination: PathBuf,
    backup: Option<PathBuf>,
}

#[derive(Debug)]
struct StagedInstall {
    plan: PlannedInstall,
    staged_path: PathBuf,
    _temporary_directory: TempDir,
}

impl StagedInstall {
    fn into_change(self) -> InstallChange {
        InstallChange {
            skill: self.plan.skill,
            title: self.plan.title,
            source: self.plan.source,
            destination: self.plan.destination,
            replaced: self.plan.backup,
        }
    }
}

fn plan_installs(
    skills: &[&InstallableSkill],
    target_roots: &[PathBuf],
    scope: Scope,
    replace: bool,
) -> Result<Vec<PlannedInstall>> {
    let timestamp = operation_timestamp();
    let mut destinations = BTreeSet::new();
    let mut backups = BTreeSet::new();
    let mut plans = Vec::new();

    for target_root in target_roots {
        let storage = SkillRoot::new(target_root.clone(), scope, Vec::new());
        for skill in skills {
            let directory_name = sanitize_install_name(&skill.name);
            let destination = target_root.join(&directory_name);
            let destination_key = normalized_path_text(&destination);
            if !destinations.insert(destination_key) {
                bail!(
                    "multiple selected skills would install to {}",
                    destination.display()
                );
            }

            let destination_exists = path_exists(&destination);
            if destination_exists && !replace {
                bail!(
                    "destination already exists: {}; pass --replace to archive and replace it",
                    destination.display()
                );
            }

            let backup = destination_exists.then(|| {
                reserve_unique_path(
                    storage
                        .trash_path
                        .join(&timestamp)
                        .join("replaced")
                        .join(&directory_name),
                    &mut backups,
                )
            });

            plans.push(PlannedInstall {
                skill: skill.name.clone(),
                title: skill.title.clone(),
                source: skill.path.clone(),
                destination,
                backup,
            });
        }
    }

    Ok(plans)
}

fn stage_installs(plans: Vec<PlannedInstall>) -> Result<Vec<StagedInstall>> {
    let mut staged = Vec::with_capacity(plans.len());

    for (index, plan) in plans.into_iter().enumerate() {
        let target_root = plan
            .destination
            .parent()
            .ok_or_else(|| anyhow!("install destination has no parent"))?;
        fs::create_dir_all(target_root)
            .with_context(|| format!("failed to create install root {}", target_root.display()))?;

        // Staging inside the target container keeps the final rename on the
        // same filesystem, including when the skills root is a mount point.
        let temporary_directory = tempfile::Builder::new()
            .prefix(&format!(".skillctl-install-{index}-"))
            .tempdir_in(target_root)
            .with_context(|| {
                format!(
                    "failed to create staging directory in {}",
                    target_root.display()
                )
            })?;
        let staged_path = temporary_directory.path().join("skill");
        copy_directory_secure(&plan.source, &staged_path)?;

        staged.push(StagedInstall {
            plan,
            staged_path,
            _temporary_directory: temporary_directory,
        });
    }

    Ok(staged)
}

fn commit_installs(staged: &[StagedInstall]) -> Result<()> {
    let mut committed = Vec::new();

    for index in 0..staged.len() {
        let current = &staged[index];
        if let Some(backup) = &current.plan.backup {
            archive_existing_install(backup, &current.plan.destination).map_err(|error| {
                let rollback_errors = rollback_installs(staged, &committed);
                rollback_error(
                    error,
                    format!(
                        "failed to archive existing skill {}",
                        current.plan.destination.display()
                    ),
                    rollback_errors,
                )
            })?;
        }

        if let Err(error) = fs::rename(&current.staged_path, &current.plan.destination) {
            let mut rollback_errors = restore_current_backup(current);
            rollback_errors.extend(rollback_installs(staged, &committed));
            return Err(rollback_error(
                error.into(),
                format!(
                    "failed to commit staged skill to {}",
                    current.plan.destination.display()
                ),
                rollback_errors,
            ));
        }

        committed.push(index);
    }

    Ok(())
}

fn archive_existing_install(backup: &Path, destination: &Path) -> Result<()> {
    if let Some(parent) = backup.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!("failed to create replacement archive {}", parent.display())
        })?;
    }
    fs::rename(destination, backup).with_context(|| {
        format!(
            "failed to move {} to {}",
            destination.display(),
            backup.display()
        )
    })
}

fn restore_current_backup(staged: &StagedInstall) -> Vec<String> {
    let Some(backup) = &staged.plan.backup else {
        return Vec::new();
    };

    match fs::rename(backup, &staged.plan.destination) {
        Ok(()) => Vec::new(),
        Err(error) => vec![format!(
            "could not restore {}: {error}",
            staged.plan.destination.display()
        )],
    }
}

fn rollback_installs(staged: &[StagedInstall], committed: &[usize]) -> Vec<String> {
    let mut errors = Vec::new();
    for index in committed.iter().rev().copied() {
        let plan = &staged[index].plan;
        if path_exists(&plan.destination) {
            if let Err(error) = remove_path(&plan.destination) {
                errors.push(format!(
                    "could not remove partial install {}: {error:#}",
                    plan.destination.display()
                ));
                continue;
            }
        }
        if let Some(backup) = &plan.backup {
            if path_exists(backup) {
                if let Err(error) = fs::rename(backup, &plan.destination) {
                    errors.push(format!(
                        "could not restore {}: {error}",
                        plan.destination.display()
                    ));
                }
            }
        }
    }
    errors
}

fn select_skills<'a>(
    skills: &'a [InstallableSkill],
    selected_indices: &[usize],
) -> Result<Vec<&'a InstallableSkill>> {
    if selected_indices.is_empty() {
        bail!("no skills selected for installation");
    }

    let mut unique = BTreeSet::new();
    selected_indices
        .iter()
        .copied()
        .filter(|index| unique.insert(*index))
        .map(|index| {
            skills
                .get(index)
                .ok_or_else(|| anyhow!("selected skill index {index} is out of range"))
        })
        .collect()
}
