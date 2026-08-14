mod common;

use std::collections::BTreeSet;

use agent_skill::{
    model::{Scope, SkillRoot},
    paths::{agents, shorten_path, PathRegistry},
};
use tempfile::tempdir;

use common::environment;

#[test]
fn mirrors_all_agent_definitions_from_the_reference_registry() {
    let definitions = agents();
    let unique = definitions
        .iter()
        .map(|agent| agent.key)
        .collect::<BTreeSet<_>>();

    assert_eq!(definitions.len(), 76);
    assert_eq!(unique.len(), definitions.len());
    assert!(definitions.iter().any(|agent| agent.key == "claude-code"));
    assert!(definitions.iter().any(|agent| agent.key == "codex"));
    assert!(definitions.iter().any(|agent| agent.key == "openclaw"));
    assert!(definitions.iter().any(|agent| agent.key == "universal"));
}

#[test]
fn aggregates_agents_that_share_the_same_physical_root() {
    let temporary = tempdir().expect("temporary directory");
    let project = temporary.path().join("project");
    let home = temporary.path().join("home");
    let registry = PathRegistry::from_environment(environment(&project, &home), Vec::new())
        .expect("build path registry");
    let shared = registry
        .roots()
        .iter()
        .find(|root| root.scope == Scope::Local && root.path == project.join(".agents/skills"))
        .expect("shared local .agents root");

    assert!(shared.agents.iter().any(|agent| agent == "Amp"));
    assert!(shared.agents.iter().any(|agent| agent == "Codex"));
    assert!(shared.agents.iter().any(|agent| agent == "GitHub Copilot"));
}

#[test]
fn classifies_a_shared_home_and_project_root_as_global_once() {
    let temporary = tempdir().expect("temporary directory");
    let home = temporary.path().join("home");
    let registry = PathRegistry::from_environment(environment(&home, &home), Vec::new())
        .expect("build path registry");
    let shared_path = home.join(".agents/skills");
    let matches = registry
        .roots()
        .iter()
        .filter(|root| root.path == shared_path)
        .collect::<Vec<_>>();

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].scope, Scope::Global);
    assert!(matches[0].agents.iter().any(|agent| agent == "Universal"));
    assert!(matches[0].agents.iter().any(|agent| agent == "Codex"));
}

#[test]
fn resolves_agent_specific_and_custom_install_targets() {
    let temporary = tempdir().expect("temporary directory");
    let project = temporary.path().join("project");
    let home = temporary.path().join("home");
    let registry = PathRegistry::from_environment(environment(&project, &home), Vec::new())
        .expect("build path registry");

    let local = registry
        .target_paths(Scope::Local, &["claude-code".to_owned()], None)
        .expect("Claude local target");
    let global = registry
        .target_paths(Scope::Global, &["opencode".to_owned()], None)
        .expect("OpenCode global target");
    let custom = registry
        .target_paths(
            Scope::Global,
            &[],
            Some(&temporary.path().join("custom/skills")),
        )
        .expect("custom target");

    let expected_project = project.canonicalize().expect("canonical project root");
    let expected_home = home.canonicalize().expect("canonical home directory");
    let expected_temporary = temporary
        .path()
        .canonicalize()
        .expect("canonical temporary directory");

    assert_eq!(local, vec![expected_project.join(".claude/skills")]);
    assert_eq!(global, vec![expected_home.join(".config/opencode/skills")]);
    assert_eq!(custom, vec![expected_temporary.join("custom/skills")]);
}

#[test]
fn derives_hidden_sibling_stores_without_nesting_disabled_skills() {
    let root = SkillRoot::new(
        std::path::PathBuf::from("project/.agents/skills"),
        Scope::Local,
        Vec::new(),
    );

    assert_eq!(
        root.disabled_path,
        std::path::PathBuf::from("project/.agents/.skills-disabled")
    );
    assert_eq!(
        root.trash_path,
        std::path::PathBuf::from("project/.agents/.skills-trash")
    );
}

#[test]
fn shortens_project_paths_before_home_paths() {
    let temporary = tempdir().expect("temporary directory");
    let home = temporary.path().join("home");
    let project = home.join("work/project");
    let environment = environment(&project, &home);
    let separator = std::path::MAIN_SEPARATOR;

    assert_eq!(
        shorten_path(&project.join(".agents/skills/demo"), &environment),
        format!(".{separator}.agents{separator}skills{separator}demo")
    );
    assert_eq!(
        shorten_path(&home.join(".config/agents/skills"), &environment),
        format!("~{separator}.config{separator}agents{separator}skills")
    );
}

#[test]
fn uses_home_notation_when_project_root_is_home() {
    let temporary = tempdir().expect("temporary directory");
    let home = temporary.path().join("home");
    let environment = environment(&home, &home);
    let separator = std::path::MAIN_SEPARATOR;

    assert_eq!(
        shorten_path(&home.join(".agents/skills"), &environment),
        format!("~{separator}.agents{separator}skills")
    );
}

#[cfg(unix)]
#[test]
fn resolves_environment_and_missing_roots_through_symlinked_ancestors() {
    use std::{fs, os::unix::fs::symlink};

    let temporary = tempdir().expect("temporary directory");
    let real_project = temporary.path().join("real-project");
    let project_alias = temporary.path().join("project-alias");
    let real_home = temporary.path().join("real-home");
    let home_alias = temporary.path().join("home-alias");
    fs::create_dir_all(&real_project).expect("create real project");
    fs::create_dir_all(&real_home).expect("create real home");
    symlink(&real_project, &project_alias).expect("create project alias");
    symlink(&real_home, &home_alias).expect("create home alias");

    let registry =
        PathRegistry::from_environment(environment(&project_alias, &home_alias), Vec::new())
            .expect("build path registry");
    let expected_project = real_project.canonicalize().expect("canonical real project");
    let expected_home = real_home.canonicalize().expect("canonical real home");
    let expected_root = expected_project.join(".agents/skills");

    assert_eq!(registry.environment().project_root, expected_project);
    assert_eq!(registry.environment().home_dir, expected_home);
    assert_eq!(
        registry.environment().config_home,
        expected_home.join(".config")
    );

    let shared = registry
        .roots()
        .iter()
        .find(|root| root.scope == Scope::Local && root.path == expected_root)
        .expect("resolved shared local root");
    assert!(shared.agents.iter().any(|agent| agent == "Universal"));
}
