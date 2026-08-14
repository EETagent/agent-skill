#![allow(dead_code)]

use std::{
    fs,
    path::{Path, PathBuf},
};

use agent_skill::{
    manager::SkillManager,
    model::{Scope, SkillRoot},
    paths::{Environment, PathRegistry},
};

pub fn write_skill(
    directory: impl AsRef<Path>,
    name: &str,
    title: &str,
    description: &str,
    body: &str,
) -> PathBuf {
    let directory = directory.as_ref();
    fs::create_dir_all(directory).expect("create test skill directory");
    let skill_md = directory.join("SKILL.md");
    fs::write(
        &skill_md,
        format!(
            "---\nname: {name}\ntitle: {title}\ndescription: {description}\n---\n\n# {title}\n\n{body}\n"
        ),
    )
    .expect("write test SKILL.md");
    skill_md
}

pub fn environment(project_root: impl AsRef<Path>, home_dir: impl AsRef<Path>) -> Environment {
    let project_root = project_root.as_ref().to_path_buf();
    let home_dir = home_dir.as_ref().to_path_buf();
    fs::create_dir_all(&project_root).expect("create test project root");
    fs::create_dir_all(&home_dir).expect("create test home directory");

    Environment {
        project_root,
        config_home: home_dir.join(".config"),
        native_config_home: Some(home_dir.join("native-config")),
        home_dir,
    }
}

pub fn manager_for_root(
    project_root: impl AsRef<Path>,
    home_dir: impl AsRef<Path>,
    root: impl AsRef<Path>,
    scope: Scope,
) -> SkillManager {
    let root = SkillRoot::new(
        root.as_ref().to_path_buf(),
        scope,
        vec!["Test agent".to_owned()],
    );
    let registry = PathRegistry::from_roots(environment(project_root, home_dir), vec![root]);
    SkillManager::from_registry(registry)
}
