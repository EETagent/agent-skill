#[cfg(unix)]
mod unix {
    use std::{fs, os::unix::fs::symlink};

    use agent_skill::fsops::{copy_directory_secure, path_exists};
    use tempfile::tempdir;

    #[test]
    fn failed_copy_removes_the_incomplete_destination() {
        let temporary = tempdir().expect("temporary directory");
        let source = temporary.path().join("source");
        let destination = temporary.path().join("destination");
        let outside = temporary.path().join("outside.txt");
        fs::create_dir_all(&source).expect("create source");
        fs::write(source.join("SKILL.md"), "# Safe\n").expect("write source file");
        fs::write(&outside, "secret\n").expect("write outside file");
        symlink(&outside, source.join("outside-link")).expect("create escaping symlink");

        let error = copy_directory_secure(&source, &destination)
            .expect_err("copy must reject an escaping symlink");

        assert!(format!("{error:#}").contains("outside skill directory"));
        assert!(!path_exists(&destination));
    }

    #[test]
    fn self_copy_detection_resolves_symlinked_existing_ancestors() {
        let temporary = tempdir().expect("temporary directory");
        let source = temporary.path().join("source");
        let source_alias = temporary.path().join("source-alias");
        fs::create_dir_all(&source).expect("create source");
        fs::write(source.join("SKILL.md"), "# Skill\n").expect("write source file");
        symlink(&source, &source_alias).expect("create source alias");
        let destination = source_alias.join("missing/nested-copy");

        let error = copy_directory_secure(&source, &destination)
            .expect_err("copying through a symlinked ancestor must be rejected");

        assert!(format!("{error:#}").contains("inside"));
        assert!(!path_exists(&destination));
    }
}
