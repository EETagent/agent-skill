mod common;

use agent_skill::{
    manager::SkillQuery,
    model::{Scope, SkillState},
};
use tempfile::tempdir;

use common::{manager_for_root, write_skill};

#[test]
fn disabling_and_enabling_moves_the_complete_skill_directory() {
    let temporary = tempdir().expect("temporary directory");
    let project = temporary.path().join("project");
    let home = temporary.path().join("home");
    let root = project.join(".agents/skills");
    let skill_dir = root.join("alpha");
    write_skill(&skill_dir, "alpha", "Alpha", "Description", "body");
    std::fs::write(skill_dir.join("asset.txt"), "asset").expect("write skill asset");
    let manager = manager_for_root(&project, &home, &root, Scope::Local);

    let skill = manager.refresh().skills[0].clone();
    let original_id = skill.id.clone();
    let disabled = manager
        .change_state(&[skill], SkillState::Disabled)
        .expect("disable skill");
    let disabled_path = project.join(".agents/.skills-disabled/alpha");

    assert_eq!(disabled.len(), 1);
    assert!(!skill_dir.exists());
    assert!(disabled_path.join("SKILL.md").is_file());
    assert!(disabled_path.join("asset.txt").is_file());

    let disabled_catalog = manager.refresh();
    let disabled_skill = manager
        .select(
            &disabled_catalog,
            std::slice::from_ref(&original_id),
            false,
            &SkillQuery {
                state: Some(SkillState::Disabled),
                ..SkillQuery::default()
            },
        )
        .expect("select disabled skill by its original ID")
        .into_iter()
        .next()
        .expect("disabled skill in catalog");
    assert_eq!(disabled_skill.id, original_id);
    manager
        .change_state(&[disabled_skill], SkillState::Enabled)
        .expect("enable skill");

    assert!(skill_dir.join("SKILL.md").is_file());
    assert!(!disabled_path.exists());
}

#[test]
fn normal_remove_moves_to_reversible_trash() {
    let temporary = tempdir().expect("temporary directory");
    let project = temporary.path().join("project");
    let home = temporary.path().join("home");
    let root = project.join(".agents/skills");
    let skill_dir = root.join("alpha");
    write_skill(&skill_dir, "alpha", "Alpha", "Description", "body");
    let manager = manager_for_root(&project, &home, &root, Scope::Local);

    let skill = manager.refresh().skills[0].clone();
    let changes = manager.remove(&[skill], false).expect("remove skill");
    let archived = changes[0]
        .destination
        .as_ref()
        .expect("reversible archive path");

    assert!(!skill_dir.exists());
    assert!(archived.join("SKILL.md").is_file());
    assert!(archived.starts_with(project.join(".agents/.skills-trash")));
}

#[test]
fn state_change_plans_preflight_existing_destinations_without_mutating() {
    let temporary = tempdir().expect("temporary directory");
    let project = temporary.path().join("project");
    let home = temporary.path().join("home");
    let root = project.join(".agents/skills");
    let enabled = root.join("alpha");
    let disabled = project.join(".agents/.skills-disabled/alpha");
    write_skill(&enabled, "alpha", "Enabled alpha", "Description", "enabled");
    write_skill(
        &disabled,
        "alpha",
        "Disabled alpha",
        "Description",
        "disabled",
    );
    let manager = manager_for_root(&project, &home, &root, Scope::Local);
    let skill = manager
        .refresh()
        .skills
        .into_iter()
        .find(|skill| skill.state == SkillState::Enabled)
        .expect("enabled skill");

    let error = manager
        .plan_state_change(&[skill], SkillState::Disabled)
        .expect_err("dry-run planning should reject the same conflict as apply");

    assert!(error.to_string().contains("destination already exists"));
    assert!(enabled.join("SKILL.md").is_file());
    assert!(disabled.join("SKILL.md").is_file());
}
