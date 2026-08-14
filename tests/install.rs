mod common;

use agent_skill::{
    manager::{InstallTarget, SkillManager},
    model::Scope,
    paths::PathRegistry,
};
use tempfile::tempdir;

use common::{environment, write_skill};

#[test]
fn installs_and_archives_a_replaced_version() {
    let temporary = tempdir().expect("temporary directory");
    let project = temporary.path().join("project");
    let home = temporary.path().join("home");
    let source_root = temporary.path().join("source");
    let source_skill = source_root.join("alpha");
    let target_root = project.join("custom-skills");
    write_skill(
        &source_skill,
        "alpha",
        "Alpha",
        "Description",
        "version one",
    );

    let registry = PathRegistry::from_roots(environment(&project, &home), Vec::new());
    let manager = SkillManager::from_registry(registry);
    let prepared = manager
        .prepare_source(source_root.to_str().expect("UTF-8 test path"), false)
        .expect("prepare local source");
    let target = InstallTarget {
        scope: Scope::Local,
        agents: Vec::new(),
        custom_root: Some(target_root.clone()),
        replace: false,
    };

    let first = manager
        .install(&prepared, &[0], &target)
        .expect("first install");
    assert_eq!(first.installed.len(), 1);
    assert!(target_root.join("alpha/SKILL.md").is_file());
    assert!(first.installed[0].replaced.is_none());

    write_skill(
        &source_skill,
        "alpha",
        "Alpha",
        "Description",
        "version two",
    );
    let prepared = manager
        .prepare_source(source_root.to_str().expect("UTF-8 test path"), false)
        .expect("prepare replacement source");
    let replacement = manager
        .install(
            &prepared,
            &[0],
            &InstallTarget {
                replace: true,
                ..target
            },
        )
        .expect("replacement install");
    let backup = replacement.installed[0]
        .replaced
        .as_ref()
        .expect("replacement archive");

    let installed =
        std::fs::read_to_string(target_root.join("alpha/SKILL.md")).expect("read installed skill");
    let archived = std::fs::read_to_string(backup.join("SKILL.md")).expect("read archived skill");
    assert!(installed.contains("version two"));
    assert!(archived.contains("version one"));
}
