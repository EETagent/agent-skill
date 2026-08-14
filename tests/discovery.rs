mod common;

use std::fs;

use agent_skill::discovery::discover_source_skills;
use pretty_assertions::assert_eq;
use tempfile::tempdir;

use common::write_skill;

#[test]
fn uses_known_containers_before_bounded_fallback() {
    let temporary = tempdir().expect("temporary directory");
    let root = temporary.path();
    write_skill(
        root.join("skills/alpha"),
        "alpha",
        "Alpha",
        "Known",
        "alpha body",
    );
    write_skill(
        root.join("fixtures/deep/beta"),
        "beta",
        "Beta",
        "Fallback",
        "beta body",
    );

    let normal = discover_source_skills(root, None, false).expect("normal discovery");
    let full = discover_source_skills(root, None, true).expect("full-depth discovery");

    assert_eq!(
        normal
            .iter()
            .map(|skill| skill.name.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha"]
    );
    assert_eq!(
        full.iter()
            .map(|skill| skill.name.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha", "beta"]
    );
}

#[test]
fn a_shallow_skill_shadows_nested_skill_documents() {
    let temporary = tempdir().expect("temporary directory");
    let root = temporary.path();
    write_skill(
        root.join("skills/group"),
        "group",
        "Group",
        "Parent",
        "parent",
    );
    write_skill(
        root.join("skills/group/nested"),
        "nested",
        "Nested",
        "Child",
        "child",
    );

    let skills = discover_source_skills(root, None, true).expect("discover source");

    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].name, "group");
}

#[test]
fn ignores_disabled_trash_and_staging_directories() {
    let temporary = tempdir().expect("temporary directory");
    let root = temporary.path();
    write_skill(
        root.join(".skills-disabled/hidden"),
        "hidden",
        "Hidden",
        "No",
        "body",
    );
    write_skill(root.join(".skills-trash/old"), "old", "Old", "No", "body");
    write_skill(
        root.join(".skillctl-install-test/new"),
        "new",
        "New",
        "No",
        "body",
    );

    let skills = discover_source_skills(root, None, true).expect("discover source");
    assert!(skills.is_empty());
}

#[test]
fn discovers_ordinary_skills_with_store_like_suffixes() {
    let temporary = tempdir().expect("temporary directory");
    let root = temporary.path();
    write_skill(
        root.join("skills/foo-disabled"),
        "foo-disabled",
        "Foo Disabled",
        "Ordinary skill",
        "body",
    );
    write_skill(
        root.join("skills/archive-trash"),
        "archive-trash",
        "Archive Trash",
        "Ordinary skill",
        "body",
    );

    let skills = discover_source_skills(root, None, false).expect("discover source");
    let names = skills
        .iter()
        .map(|skill| skill.name.as_str())
        .collect::<Vec<_>>();

    assert_eq!(names, vec!["archive-trash", "foo-disabled"]);
}

#[test]
fn discovers_conventional_skills_inside_local_marketplace_plugins() {
    let temporary = tempdir().expect("temporary directory");
    let root = temporary.path();
    fs::create_dir_all(root.join(".claude-plugin")).expect("create manifest directory");
    fs::write(
        root.join(".claude-plugin/marketplace.json"),
        r#"{
  "metadata": { "pluginRoot": "./plugins" },
  "plugins": [
    { "name": "review", "source": "./review" }
  ]
}"#,
    )
    .expect("write marketplace manifest");
    write_skill(
        root.join("plugins/review/skills/alpha"),
        "alpha",
        "Alpha",
        "Marketplace skill",
        "body",
    );

    let skills = discover_source_skills(root, None, false).expect("discover marketplace skill");

    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].name, "alpha");
}

#[test]
fn ignores_remote_and_escaping_marketplace_sources() {
    let temporary = tempdir().expect("temporary directory");
    let root = temporary.path();
    fs::create_dir_all(root.join(".claude-plugin")).expect("create manifest directory");
    fs::write(
        root.join(".claude-plugin/marketplace.json"),
        r#"{
  "metadata": { "pluginRoot": "./plugins" },
  "plugins": [
    { "name": "remote", "source": { "source": "github", "repo": "owner/repo" } },
    { "name": "escape", "source": "../../outside" }
  ]
}"#,
    )
    .expect("write marketplace manifest");
    write_skill(
        root.join("outside/hidden"),
        "hidden",
        "Hidden",
        "Must not be reached through the manifest",
        "body",
    );
    write_skill(
        root.join("skills/known"),
        "known",
        "Known",
        "Keeps discovery out of the repository-wide fallback",
        "body",
    );

    let skills = discover_source_skills(root, None, false).expect("discover source");

    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].name, "known");
}

#[test]
fn marketplace_entries_without_a_source_do_not_scan_the_plugin_root() {
    let temporary = tempdir().expect("temporary directory");
    let root = temporary.path();
    fs::create_dir_all(root.join(".claude-plugin")).expect("create manifest directory");
    fs::write(
        root.join(".claude-plugin/marketplace.json"),
        r#"{
  "metadata": { "pluginRoot": "./plugins" },
  "plugins": [
    { "name": "missing-source" }
  ]
}"#,
    )
    .expect("write marketplace manifest");
    write_skill(
        root.join("plugins/skills/hidden"),
        "hidden",
        "Hidden",
        "Must not be inferred from a missing source",
        "body",
    );
    write_skill(
        root.join("skills/known"),
        "known",
        "Known",
        "Prevents repository-wide fallback discovery",
        "body",
    );

    let skills = discover_source_skills(root, None, false).expect("discover source");

    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].name, "known");
}

#[cfg(unix)]
#[test]
fn rejects_skill_documents_linked_outside_the_source() {
    use std::os::unix::fs::symlink;

    let temporary = tempdir().expect("temporary directory");
    let repository = temporary.path().join("repository");
    let skill = repository.join("skills/leak");
    fs::create_dir_all(&skill).expect("create skill directory");
    let outside = write_skill(
        temporary.path().join("outside"),
        "outside",
        "Outside",
        "Sensitive host document",
        "host secret",
    );
    symlink(outside, skill.join("SKILL.md")).expect("link escaping skill document");

    let error = discover_source_skills(&repository, None, false)
        .expect_err("escaping SKILL.md must be rejected");
    let message = format!("{error:#}");

    assert!(message.contains("skill document escapes the source repository"));
    assert!(!message.contains("host secret"));
}
