use std::{collections::BTreeSet, path::Path};

use anyhow::{bail, Result};

use super::SkillManager;
use crate::model::{normalized_path_text, Catalog, Scope, Skill, SkillState};

/// Filters and ranks installed skills.
#[derive(Debug, Clone, Default)]
pub struct SkillQuery {
    pub text: String,
    pub scope: Option<Scope>,
    pub state: Option<SkillState>,
    /// Root ID, path fragment, or agent name.
    pub root: Option<String>,
}

impl SkillQuery {
    pub fn matches(&self, skill: &Skill) -> bool {
        self.matches_filters(skill) && skill.search_score(&self.text).is_some()
    }

    pub fn ranked_indices(&self, catalog: &Catalog) -> Vec<usize> {
        let mut matches = catalog
            .skills
            .iter()
            .enumerate()
            .filter_map(|(index, skill)| {
                if !self.matches_filters(skill) {
                    return None;
                }
                skill.search_score(&self.text).map(|score| (score, index))
            })
            .collect::<Vec<_>>();

        matches.sort_by_cached_key(|(score, index)| {
            let skill = &catalog.skills[*index];
            (*score, skill.title.to_lowercase(), skill.path.clone())
        });
        matches.into_iter().map(|(_, index)| index).collect()
    }

    pub fn ranked<'a>(&self, catalog: &'a Catalog) -> Vec<&'a Skill> {
        self.ranked_indices(catalog)
            .into_iter()
            .map(|index| &catalog.skills[index])
            .collect()
    }

    fn matches_filters(&self, skill: &Skill) -> bool {
        self.scope.is_none_or(|scope| skill.scope == scope)
            && self.state.is_none_or(|state| skill.state == state)
            && self
                .root
                .as_deref()
                .is_none_or(|root| root_matches(skill, root))
    }
}

impl SkillManager {
    pub fn select(
        &self,
        catalog: &Catalog,
        selectors: &[String],
        all: bool,
        query: &SkillQuery,
    ) -> Result<Vec<Skill>> {
        let candidates = query.ranked(catalog);
        if all {
            return Ok(deduplicate_skills(candidates.into_iter().cloned()));
        }
        if selectors.is_empty() {
            bail!("provide one or more skill names/IDs, or pass --all");
        }

        let mut selected = Vec::new();
        let mut missing = Vec::new();
        for selector in selectors {
            let matches = candidates
                .iter()
                .copied()
                .filter(|skill| selector_matches(skill, selector))
                .cloned()
                .collect::<Vec<_>>();
            if matches.is_empty() {
                missing.push(selector.as_str());
            } else {
                selected.extend(matches);
            }
        }

        if !missing.is_empty() {
            bail!(
                "no matching skill found for: {}",
                missing
                    .iter()
                    .map(|selector| format!("{selector:?}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }

        Ok(deduplicate_skills(selected))
    }
}

fn selector_matches(skill: &Skill, selector: &str) -> bool {
    if skill.exact_selector_match(selector) {
        return true;
    }

    let selector_path = normalize_requested_path(selector);
    normalized_path_text(&skill.relative_path) == selector_path
        || normalized_path_text(&skill.path) == selector_path
}

fn root_matches(skill: &Skill, requested: &str) -> bool {
    let requested_text = requested.trim().to_lowercase();
    if requested_text.is_empty() {
        return false;
    }

    let requested_path = normalize_requested_path(requested);
    skill.root_id.eq_ignore_ascii_case(&requested_text)
        || normalized_path_text(&skill.root_path).contains(&requested_path)
        || skill
            .agents
            .iter()
            .any(|agent| agent.to_lowercase().contains(&requested_text))
}

fn normalize_requested_path(value: &str) -> String {
    normalized_path_text(Path::new(value.trim()))
}

fn deduplicate_skills(skills: impl IntoIterator<Item = Skill>) -> Vec<Skill> {
    let mut seen = BTreeSet::new();
    skills
        .into_iter()
        .filter(|skill| seen.insert(normalized_path_text(&skill.path)))
        .collect()
}
