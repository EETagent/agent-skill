use std::{
    fmt,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

pub(crate) mod json_path {
    use std::path::{Path, PathBuf};

    use serde::{Serialize, Serializer};

    pub fn serialize<S>(path: &Path, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        path.to_string_lossy().serialize(serializer)
    }

    pub fn serialize_option<S>(path: &Option<PathBuf>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        path.as_ref()
            .map(|path| path.to_string_lossy())
            .serialize(serializer)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Local,
    Global,
}

impl Scope {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Global => "global",
        }
    }

    pub const fn short_label(self) -> &'static str {
        match self {
            Self::Local => "L",
            Self::Global => "G",
        }
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillState {
    Enabled,
    Disabled,
}

impl SkillState {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
        }
    }

    pub const fn opposite(self) -> Self {
        match self {
            Self::Enabled => Self::Disabled,
            Self::Disabled => Self::Enabled,
        }
    }
}

impl fmt::Display for SkillState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillRoot {
    pub id: String,
    #[serde(serialize_with = "json_path::serialize")]
    pub path: PathBuf,
    #[serde(serialize_with = "json_path::serialize")]
    pub disabled_path: PathBuf,
    #[serde(serialize_with = "json_path::serialize")]
    pub trash_path: PathBuf,
    pub scope: Scope,
    pub agents: Vec<String>,
}

impl SkillRoot {
    pub fn new(path: PathBuf, scope: Scope, agents: Vec<String>) -> Self {
        let store_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("skills")
            .to_owned();
        let parent = path.parent().unwrap_or(&path).to_path_buf();
        let id = format!(
            "{}-{:016x}",
            scope.label(),
            stable_hash(&normalized_path_text(&path))
        );

        Self {
            id,
            disabled_path: parent.join(format!(".{store_name}-disabled")),
            trash_path: parent.join(format!(".{store_name}-trash")),
            path,
            scope,
            agents,
        }
    }

    pub fn store_path(&self, state: SkillState) -> &Path {
        match state {
            SkillState::Enabled => &self.path,
            SkillState::Disabled => &self.disabled_path,
        }
    }

    pub fn label(&self) -> String {
        if self.agents.is_empty() {
            self.path.display().to_string()
        } else {
            self.agents.join(", ")
        }
    }
}

#[derive(Debug, Clone)]
pub struct SkillMetadata {
    pub name: String,
    pub title: String,
    pub description: String,
    pub content: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub title: String,
    pub description: String,
    #[serde(serialize_with = "json_path::serialize")]
    pub path: PathBuf,
    #[serde(serialize_with = "json_path::serialize")]
    pub relative_path: PathBuf,
    pub root_id: String,
    #[serde(serialize_with = "json_path::serialize")]
    pub root_path: PathBuf,
    pub scope: Scope,
    pub state: SkillState,
    pub agents: Vec<String>,
    pub content: String,
    pub warnings: Vec<String>,
}

impl Skill {
    /// Builds a skill from its individual fields.
    ///
    /// New internal code should prefer [`Self::from_metadata`]; this wrapper
    /// keeps the crate's existing public constructor compatible.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        title: String,
        description: String,
        path: PathBuf,
        relative_path: PathBuf,
        root: &SkillRoot,
        state: SkillState,
        content: String,
        warnings: Vec<String>,
    ) -> Self {
        Self::from_metadata(
            SkillMetadata {
                name,
                title,
                description,
                content,
                warnings,
            },
            path,
            relative_path,
            root,
            state,
        )
    }

    pub fn from_metadata(
        metadata: SkillMetadata,
        path: PathBuf,
        relative_path: PathBuf,
        root: &SkillRoot,
        state: SkillState,
    ) -> Self {
        let id_seed = format!("{}:{}", root.id, normalized_path_text(&relative_path));

        Self {
            id: format!("skill-{:016x}", stable_hash(&id_seed)),
            name: metadata.name,
            title: metadata.title,
            description: metadata.description,
            path,
            relative_path,
            root_id: root.id.clone(),
            root_path: root.path.clone(),
            scope: root.scope,
            state,
            agents: root.agents.clone(),
            content: metadata.content,
            warnings: metadata.warnings,
        }
    }

    pub fn display_path(&self) -> String {
        self.path.display().to_string()
    }

    pub fn searchable_text(&self) -> String {
        format!(
            "{}\n{}\n{}\n{}\n{}",
            self.title,
            self.name,
            self.description,
            self.relative_path.display(),
            self.agents.join(" ")
        )
        .to_lowercase()
    }

    /// Returns a lower score for a better match. Every query token must match.
    pub fn search_score(&self, query: &str) -> Option<usize> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return Some(0);
        }

        let haystack = self.searchable_text();
        let tokens = query.split_whitespace().collect::<Vec<_>>();
        if !tokens.iter().all(|token| haystack.contains(token)) {
            return None;
        }

        let title = self.title.to_lowercase();
        let name = self.name.to_lowercase();
        let tier = if title == query || name == query {
            0
        } else if title.starts_with(&query) || name.starts_with(&query) {
            10
        } else if title.contains(&query) || name.contains(&query) {
            25
        } else {
            100
        };
        let position_score = tokens.into_iter().fold(0usize, |score, token| {
            score.saturating_add(
                title
                    .find(token)
                    .or_else(|| name.find(token))
                    .or_else(|| haystack.find(token))
                    .unwrap_or(1_000),
            )
        });

        // Tier gaps are at least ten points. Capping the tie-break component
        // keeps every exact match ahead of every prefix match, and so on.
        Some(tier + position_score.min(9))
    }

    pub fn exact_selector_match(&self, selector: &str) -> bool {
        if self.id.eq_ignore_ascii_case(selector) {
            return true;
        }

        let selector = normalize_skill_name(selector);
        [
            normalize_skill_name(&self.name),
            normalize_skill_name(&self.title),
            self.relative_path
                .file_name()
                .and_then(|name| name.to_str())
                .map(normalize_skill_name)
                .unwrap_or_default(),
        ]
        .into_iter()
        .any(|candidate| candidate == selector)
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Catalog {
    pub skills: Vec<Skill>,
    pub warnings: Vec<String>,
}

impl Catalog {
    pub fn sort(&mut self) {
        self.skills.sort_by_cached_key(|skill| {
            (
                skill.title.to_lowercase(),
                skill.scope.label(),
                skill.state.label(),
                skill.path.clone(),
            )
        });
    }

    pub fn counts(&self) -> CatalogCounts {
        self.skills
            .iter()
            .fold(CatalogCounts::default(), |mut counts, skill| {
                match skill.state {
                    SkillState::Enabled => counts.enabled += 1,
                    SkillState::Disabled => counts.disabled += 1,
                }
                match skill.scope {
                    Scope::Local => counts.local += 1,
                    Scope::Global => counts.global += 1,
                }
                counts
            })
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct CatalogCounts {
    pub enabled: usize,
    pub disabled: usize,
    pub local: usize,
    pub global: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillChange {
    pub skill: String,
    #[serde(serialize_with = "json_path::serialize")]
    pub from: PathBuf,
    #[serde(serialize_with = "json_path::serialize")]
    pub to: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoveChange {
    pub skill: String,
    #[serde(serialize_with = "json_path::serialize")]
    pub from: PathBuf,
    #[serde(serialize_with = "json_path::serialize_option")]
    pub destination: Option<PathBuf>,
    pub purged: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallableSkill {
    pub name: String,
    pub title: String,
    pub description: String,
    #[serde(serialize_with = "json_path::serialize")]
    pub path: PathBuf,
    #[serde(serialize_with = "json_path::serialize")]
    pub relative_path: PathBuf,
    pub content: String,
    pub warnings: Vec<String>,
}

impl InstallableSkill {
    pub fn new(metadata: SkillMetadata, path: PathBuf, relative_path: PathBuf) -> Self {
        Self {
            name: metadata.name,
            title: metadata.title,
            description: metadata.description,
            path,
            relative_path,
            content: metadata.content,
            warnings: metadata.warnings,
        }
    }

    pub fn exact_name_match(&self, requested: &str) -> bool {
        let requested = normalize_skill_name(requested);
        normalize_skill_name(&self.name) == requested
            || normalize_skill_name(&self.title) == requested
            || self
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .map(normalize_skill_name)
                .is_some_and(|name| name == requested)
    }
}

pub fn normalize_skill_name(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .chars()
        .map(|character| match character {
            character if character.is_alphanumeric() => character,
            '-' | '_' | ' ' => '-',
            _ => '-',
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

pub fn normalized_path_text(path: &Path) -> String {
    #[cfg(windows)]
    {
        path.to_string_lossy().replace('\\', "/").to_lowercase()
    }
    #[cfg(not(windows))]
    {
        path.to_string_lossy().into_owned()
    }
}

pub fn stable_hash(value: &str) -> u64 {
    // FNV-1a is sufficient here: IDs are presentation and lookup helpers, not
    // security boundaries.
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x00000100000001b3;

    value.as_bytes().iter().fold(OFFSET, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(PRIME)
    })
}
