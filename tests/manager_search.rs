use std::path::PathBuf;

use agent_skill::{
    manager::{SkillManager, SkillQuery},
    model::{normalized_path_text, Catalog, Scope, Skill, SkillMetadata, SkillRoot, SkillState},
    paths::{Environment, PathRegistry},
};
use pretty_assertions::assert_eq;

fn skill(name: &str, title: &str, description: &str, state: SkillState) -> Skill {
    let root = SkillRoot::new(
        PathBuf::from("project/.agents/skills"),
        Scope::Local,
        vec!["Universal".to_owned()],
    );
    Skill::from_metadata(
        SkillMetadata {
            name: name.to_owned(),
            title: title.to_owned(),
            description: description.to_owned(),
            content: String::new(),
            warnings: Vec::new(),
        },
        root.path.join(name),
        PathBuf::from(name),
        &root,
        state,
    )
}

#[test]
fn full_text_title_search_is_case_insensitive_and_token_based() {
    let catalog = Catalog {
        skills: vec![
            skill(
                "rust-review",
                "Senior Rust Review",
                "Ownership and APIs",
                SkillState::Enabled,
            ),
            skill(
                "python-review",
                "Python Review",
                "Typing",
                SkillState::Enabled,
            ),
        ],
        warnings: Vec::new(),
    };
    let query = SkillQuery {
        text: "RUST senior".to_owned(),
        ..SkillQuery::default()
    };

    let matches = query.ranked(&catalog);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].name, "rust-review");
}

#[test]
fn exact_match_tier_stays_ahead_of_positional_tie_breakers() {
    let long_token = "a".repeat(160);
    let query = format!("{long_token} omega");
    let exact = skill("exact", &query, "Description", SkillState::Enabled);
    let reordered = skill(
        "reordered",
        &format!("omega {long_token}"),
        "Description",
        SkillState::Enabled,
    );
    let catalog = Catalog {
        skills: vec![reordered, exact],
        warnings: Vec::new(),
    };

    let matches = SkillQuery {
        text: query,
        ..SkillQuery::default()
    }
    .ranked(&catalog);

    assert_eq!(matches[0].name, "exact");
}

#[test]
fn scope_state_and_root_filters_share_the_same_query_backend() {
    let enabled = skill("enabled", "Enabled", "Description", SkillState::Enabled);
    let disabled = skill("disabled", "Disabled", "Description", SkillState::Disabled);
    let catalog = Catalog {
        skills: vec![enabled, disabled],
        warnings: Vec::new(),
    };
    let query = SkillQuery {
        scope: Some(Scope::Local),
        state: Some(SkillState::Disabled),
        root: Some("Universal".to_owned()),
        ..SkillQuery::default()
    };

    let matches = query.ranked(&catalog);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].name, "disabled");
}

#[cfg(unix)]
#[test]
fn unix_backslashes_remain_distinct_in_ids_and_bulk_selection() {
    let backslash = skill(r"a\b", "Backslash", "Description", SkillState::Enabled);
    let separator = skill("a/b", "Separator", "Description", SkillState::Enabled);
    assert_ne!(backslash.id, separator.id);
    assert_ne!(
        normalized_path_text(&backslash.path),
        normalized_path_text(&separator.path)
    );

    let catalog = Catalog {
        skills: vec![backslash, separator],
        warnings: Vec::new(),
    };
    let environment = Environment {
        project_root: PathBuf::from("project"),
        home_dir: PathBuf::from("home"),
        config_home: PathBuf::from("home/.config"),
        native_config_home: None,
    };
    let manager = SkillManager::from_registry(PathRegistry::from_roots(environment, Vec::new()));
    let selected = manager
        .select(&catalog, &[], true, &SkillQuery::default())
        .expect("bulk selection");

    assert_eq!(selected.len(), 2);
}
