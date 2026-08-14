use std::{
    io::{self, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::Serialize;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{
    manager::SkillManager,
    model::{Catalog, InstallableSkill, Scope, Skill, SkillRoot, SkillState},
    paths::{shorten_path, Environment},
    remote::RemoteSkill,
    source::{redact_source, PreparedSource},
    terminal::sanitize_inline,
};

pub(super) fn print_skill_table(skills: &[&Skill], manager: &SkillManager) {
    if skills.is_empty() {
        println!("No skills.");
        return;
    }

    let available = terminal_width();
    let column_space = available.saturating_sub(10);
    let title_width = (column_space * 2 / 5).clamp(16, 32);
    let name_width = (column_space / 4).clamp(12, 22);
    let path_width = column_space.saturating_sub(title_width + name_width).max(1);

    println!(
        "{} {}  {}  {}  PATH",
        fit_column("ST", 2),
        fit_column("S", 1),
        fit_column("TITLE", title_width),
        fit_column("NAME", name_width),
    );
    for skill in skills {
        let path = sanitize_inline(&shorten_path(&skill.path, manager.registry().environment()));
        let state = match skill.state {
            SkillState::Enabled => "on",
            SkillState::Disabled => "off",
        };
        println!(
            "{} {}  {}  {}  {}",
            fit_column(state, 2),
            fit_column(skill.scope.short_label(), 1),
            fit_column(&sanitize_inline(&skill.title), title_width),
            fit_column(&sanitize_inline(&skill.name), name_width),
            truncate(&path, path_width),
        );
    }
}

pub(super) fn print_installable_table(skills: &[InstallableSkill]) {
    if skills.is_empty() {
        println!("No skills.");
        return;
    }
    for skill in skills {
        println!(
            "{} ({})",
            sanitize_inline(&skill.title),
            sanitize_inline(&skill.name)
        );
        if !skill.description.is_empty() {
            println!("  {}", sanitize_inline(&skill.description));
        }
        println!("  {}", safe_path(&skill.path));
    }
}

pub(super) fn print_remote_table(skills: &[RemoteSkill]) {
    if skills.is_empty() {
        println!("No remote skills.");
        return;
    }
    for skill in skills {
        let source = redact_source(&skill.install_source());
        println!(
            "{}  {} install(s)\n  {}",
            sanitize_inline(&skill.name),
            skill.installs,
            remote_install_hint(&source)
        );
    }
}

#[cfg(unix)]
fn remote_install_hint(source: &str) -> String {
    let source = sanitize_inline(source);
    let quoted = format!("'{}'", source.replace('\'', "'\\''"));
    format!("install: skillctl install -- {quoted}")
}

#[cfg(not(unix))]
fn remote_install_hint(source: &str) -> String {
    // There is no single quoting convention shared by cmd.exe and PowerShell,
    // so avoid presenting remote data as a directly executable command.
    format!("install source: {}", sanitize_inline(source))
}

pub(super) fn print_catalog_warnings(catalog: &Catalog) {
    for warning in &catalog.warnings {
        eprintln!("warning: {}", sanitize_inline(warning));
    }
}

fn terminal_width() -> usize {
    std::env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .map(|width| width.clamp(60, 240))
        .unwrap_or(120)
}

pub(super) fn safe_path(path: &Path) -> String {
    sanitize_inline(&path.display().to_string())
}

fn fit_column(value: &str, width: usize) -> String {
    let mut value = truncate(value, width);
    let padding = width.saturating_sub(UnicodeWidthStr::width(value.as_str()));
    value.extend(std::iter::repeat_n(' ', padding));
    value
}

fn truncate(value: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }

    let character_limit = width.saturating_mul(8).max(64);
    let mut characters = value.chars();
    let mut accepted = Vec::new();
    let mut used_width = 0usize;
    let mut truncated = false;

    for _ in 0..character_limit {
        let Some(character) = characters.next() else {
            break;
        };
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if used_width.saturating_add(character_width) > width {
            truncated = true;
            break;
        }
        accepted.push((character, character_width));
        used_width = used_width.saturating_add(character_width);
    }

    if !truncated && characters.next().is_some() {
        truncated = true;
    }
    if !truncated {
        return accepted
            .into_iter()
            .map(|(character, _)| character)
            .collect();
    }
    if width == 1 {
        return "…".to_owned();
    }

    let target_width = width - 1;
    while used_width > target_width {
        let Some((_, character_width)) = accepted.pop() else {
            break;
        };
        used_width = used_width.saturating_sub(character_width);
    }

    let mut output = accepted
        .into_iter()
        .map(|(character, _)| character)
        .collect::<String>();
    output.push('…');
    output
}

pub(super) fn write_json(value: &impl Serialize) -> Result<()> {
    // Serialize before touching stdout so a serialization failure can never
    // leave behind a truncated JSON document.
    let output = serde_json::to_string_pretty(value).context("failed to serialize JSON")?;
    let stdout = io::stdout();
    let mut writer = stdout.lock();
    writeln!(writer, "{output}").context("failed to write JSON output")?;
    Ok(())
}

#[derive(Debug, Serialize)]
pub(super) struct CatalogOutput {
    skills: Vec<SkillSummary>,
    warnings: Vec<String>,
}

impl CatalogOutput {
    pub(super) fn new(catalog: &Catalog, skills: &[&Skill]) -> Self {
        Self {
            skills: skills
                .iter()
                .map(|skill| SkillSummary::from(*skill))
                .collect(),
            warnings: catalog.warnings.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
struct SkillSummary {
    id: String,
    name: String,
    title: String,
    description: String,
    #[serde(serialize_with = "crate::model::json_path::serialize")]
    path: PathBuf,
    #[serde(serialize_with = "crate::model::json_path::serialize")]
    relative_path: PathBuf,
    root_id: String,
    #[serde(serialize_with = "crate::model::json_path::serialize")]
    root_path: PathBuf,
    scope: Scope,
    state: SkillState,
    agents: Vec<String>,
    warnings: Vec<String>,
}

impl From<&Skill> for SkillSummary {
    fn from(skill: &Skill) -> Self {
        Self {
            id: skill.id.clone(),
            name: skill.name.clone(),
            title: skill.title.clone(),
            description: skill.description.clone(),
            path: skill.path.clone(),
            relative_path: skill.relative_path.clone(),
            root_id: skill.root_id.clone(),
            root_path: skill.root_path.clone(),
            scope: skill.scope,
            state: skill.state,
            agents: skill.agents.clone(),
            warnings: skill.warnings.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
pub(super) struct SourceOutput {
    original: String,
    resolved: String,
    temporary: bool,
    skills: Vec<InstallableSummary>,
}

impl SourceOutput {
    pub(super) fn from_prepared(prepared: &PreparedSource) -> Self {
        Self {
            original: redact_source(&prepared.original),
            resolved: redact_source(&prepared.resolved),
            temporary: prepared.is_temporary(),
            skills: prepared
                .skills
                .iter()
                .map(|skill| InstallableSummary {
                    name: skill.name.clone(),
                    title: skill.title.clone(),
                    description: skill.description.clone(),
                    path: skill.path.clone(),
                    relative_path: skill.relative_path.clone(),
                    warnings: skill.warnings.clone(),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Serialize)]
struct InstallableSummary {
    name: String,
    title: String,
    description: String,
    #[serde(serialize_with = "crate::model::json_path::serialize")]
    path: PathBuf,
    #[serde(serialize_with = "crate::model::json_path::serialize")]
    relative_path: PathBuf,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct FindOutput {
    query: String,
    local: Vec<SkillSummary>,
    remote: Vec<RemoteSkillSummary>,
    remote_error: Option<String>,
}

impl FindOutput {
    pub(super) fn new(
        query: String,
        local: &[&Skill],
        remote: &[RemoteSkill],
        remote_error: Option<String>,
    ) -> Self {
        Self {
            query,
            local: local
                .iter()
                .map(|skill| SkillSummary::from(*skill))
                .collect(),
            remote: remote.iter().map(RemoteSkillSummary::from).collect(),
            remote_error,
        }
    }
}

#[derive(Debug, Serialize)]
struct RemoteSkillSummary {
    name: String,
    slug: String,
    source: String,
    installs: u64,
}

impl From<&RemoteSkill> for RemoteSkillSummary {
    fn from(skill: &RemoteSkill) -> Self {
        Self {
            name: skill.name.clone(),
            slug: skill.slug.clone(),
            source: redact_source(&skill.source),
            installs: skill.installs,
        }
    }
}

#[derive(Debug, Serialize)]
pub(super) struct RootOutput {
    id: String,
    scope: Scope,
    #[serde(serialize_with = "crate::model::json_path::serialize")]
    path: PathBuf,
    #[serde(serialize_with = "crate::model::json_path::serialize")]
    disabled_path: PathBuf,
    #[serde(serialize_with = "crate::model::json_path::serialize")]
    trash_path: PathBuf,
    agents: Vec<String>,
    enabled_store_exists: bool,
    disabled_store_exists: bool,
}

impl From<&SkillRoot> for RootOutput {
    fn from(root: &SkillRoot) -> Self {
        Self {
            id: root.id.clone(),
            scope: root.scope,
            path: root.path.clone(),
            disabled_path: root.disabled_path.clone(),
            trash_path: root.trash_path.clone(),
            agents: root.agents.clone(),
            enabled_store_exists: root.path.exists(),
            disabled_store_exists: root.disabled_path.exists(),
        }
    }
}

pub(super) fn print_root_table(roots: &[RootOutput], environment: &Environment) {
    for root in roots {
        println!(
            "{}  {}  {}",
            root.scope.short_label(),
            if root.enabled_store_exists || root.disabled_store_exists {
                "present"
            } else {
                "missing"
            },
            sanitize_inline(&shorten_path(&root.path, environment))
        );
        println!("  agents: {}", sanitize_inline(&root.agents.join(", ")));
        println!(
            "  disabled: {}",
            sanitize_inline(&shorten_path(&root.disabled_path, environment))
        );
        println!(
            "  trash: {}",
            sanitize_inline(&shorten_path(&root.trash_path, environment))
        );
        println!("  id: {}", sanitize_inline(&root.id));
    }
    println!("{} unique skills roots registered.", roots.len());
}

#[cfg(test)]
mod tests {
    use super::{remote_install_hint, RemoteSkill, RemoteSkillSummary};

    #[cfg(unix)]
    #[test]
    fn remote_install_hint_quotes_the_complete_untrusted_source() {
        let hint = remote_install_hint("owner/repo; touch /tmp/pwned@name'with-quote");

        assert_eq!(
            hint,
            "install: skillctl install -- 'owner/repo; touch /tmp/pwned@name'\\''with-quote'"
        );
    }

    #[test]
    fn remote_json_summary_redacts_source_secrets() {
        let skill = RemoteSkill {
            name: "alpha".to_owned(),
            slug: "owner/repository/alpha".to_owned(),
            source: "https://user:secret@example.com/repository?token=value".to_owned(),
            installs: 42,
        };

        let value = serde_json::to_value(RemoteSkillSummary::from(&skill))
            .expect("serialize remote summary");
        let source = value["source"].as_str().expect("serialized source");

        assert!(!source.contains("user"));
        assert!(!source.contains("secret"));
        assert!(!source.contains("token=value"));
        assert!(source.contains("REDACTED"));
    }
}
