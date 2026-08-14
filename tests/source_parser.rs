use std::path::PathBuf;

use agent_skill::source::{parse_source, redact_source, ParsedSource};
use pretty_assertions::assert_eq;

#[test]
fn parses_github_shorthand_with_ref_subpath_and_skill_filter() {
    let parsed = parse_source("owner/repository/skills/rust#feature/ref@rust-skill")
        .expect("parse shorthand");

    match parsed {
        ParsedSource::Git {
            clone_url,
            reference,
            subpath,
            skill_filter,
        } => {
            assert!(clone_url.ends_with("/owner/repository.git"));
            assert_eq!(reference.as_deref(), Some("feature/ref"));
            assert_eq!(subpath, Some(PathBuf::from("skills/rust")));
            assert_eq!(skill_filter.as_deref(), Some("rust-skill"));
        }
        ParsedSource::Local { .. } => panic!("expected Git source"),
    }
}

#[test]
fn parses_github_tree_and_blob_urls() {
    let tree = parse_source("https://github.com/owner/repo/tree/main/skills/alpha")
        .expect("parse tree URL");
    let blob = parse_source("https://github.com/owner/repo/blob/main/skills/alpha/SKILL.md#L10")
        .expect("parse blob URL");

    match tree {
        ParsedSource::Git {
            reference, subpath, ..
        } => {
            assert_eq!(reference.as_deref(), Some("main"));
            assert_eq!(subpath, Some(PathBuf::from("skills/alpha")));
        }
        ParsedSource::Local { .. } => panic!("expected Git source"),
    }
    match blob {
        ParsedSource::Git {
            reference, subpath, ..
        } => {
            assert_eq!(reference.as_deref(), Some("main"));
            assert_eq!(subpath, Some(PathBuf::from("skills/alpha")));
        }
        ParsedSource::Local { .. } => panic!("expected Git source"),
    }
}

#[test]
fn accepts_a_skill_filter_fragment_on_a_blob_url() {
    let parsed =
        parse_source("https://github.com/owner/repo/blob/main/skills/alpha/SKILL.md#@alpha")
            .expect("parse blob URL with skill filter");

    match parsed {
        ParsedSource::Git {
            reference,
            subpath,
            skill_filter,
            ..
        } => {
            assert_eq!(reference.as_deref(), Some("main"));
            assert_eq!(subpath, Some(PathBuf::from("skills/alpha")));
            assert_eq!(skill_filter.as_deref(), Some("alpha"));
        }
        ParsedSource::Local { .. } => panic!("expected Git source"),
    }
}

#[test]
fn treats_markdown_fragments_on_blob_urls_as_document_anchors() {
    let github =
        parse_source("https://github.com/owner/repo/blob/main/skills/alpha/SKILL.md#usage")
            .expect("parse GitHub blob anchor");
    let gitlab = parse_source(
        "https://gitlab.com/group/repo/-/blob/main/skills/alpha/SKILL.md#getting-started",
    )
    .expect("parse GitLab blob anchor");

    for parsed in [github, gitlab] {
        match parsed {
            ParsedSource::Git {
                reference,
                subpath,
                skill_filter,
                ..
            } => {
                assert_eq!(reference.as_deref(), Some("main"));
                assert_eq!(subpath, Some(PathBuf::from("skills/alpha")));
                assert_eq!(skill_filter, None);
            }
            ParsedSource::Local { .. } => panic!("expected Git source"),
        }
    }
}

#[test]
fn preserves_url_authority_and_query_when_normalizing_clone_paths() {
    let sources = [
        (
            "https://user:token@github.com:8443/owner/private/tree/main/skills/alpha?access=secret",
            "/owner/private.git",
        ),
        (
            "https://user:token@gitlab.com:8443/group/private/-/tree/main/skills/alpha?access=secret",
            "/group/private.git",
        ),
    ];

    for (source, expected_path) in sources {
        let ParsedSource::Git { clone_url, .. } = parse_source(source).expect("parse private URL")
        else {
            panic!("expected Git source");
        };
        let clone_url = url::Url::parse(&clone_url).expect("normalized clone URL");

        assert_eq!(clone_url.username(), "user");
        assert_eq!(clone_url.password(), Some("token"));
        assert_eq!(clone_url.port(), Some(8443));
        assert_eq!(clone_url.path(), expected_path);
        assert_eq!(clone_url.query(), Some("access=secret"));
        assert_eq!(clone_url.fragment(), None);
    }
}

#[test]
fn parses_nested_gitlab_namespace() {
    let parsed =
        parse_source("https://gitlab.com/group/subgroup/repository/-/tree/develop/skills/alpha")
            .expect("parse GitLab URL");

    match parsed {
        ParsedSource::Git {
            clone_url,
            reference,
            subpath,
            ..
        } => {
            assert_eq!(
                clone_url,
                "https://gitlab.com/group/subgroup/repository.git"
            );
            assert_eq!(reference.as_deref(), Some("develop"));
            assert_eq!(subpath, Some(PathBuf::from("skills/alpha")));
        }
        ParsedSource::Local { .. } => panic!("expected Git source"),
    }
}

#[test]
fn rejects_unsafe_refs_and_subpaths() {
    assert!(parse_source("owner/repo#../secret").is_err());
    assert!(parse_source("https://github.com/owner/repo/tree/@/skills").is_err());
    assert!(parse_source("owner/repo#.hidden/ref").is_err());
    assert!(parse_source("owner/repo/path/../secret").is_err());
}

#[test]
fn redacts_credentials_query_and_terminal_controls() {
    let redacted =
        redact_source("https://user:secret@example.com/repo.git?token=value\u{001b}[31m");

    assert!(!redacted.contains("user"));
    assert!(!redacted.contains("secret"));
    assert!(!redacted.contains("token=value"));
    assert!(!redacted.contains('\u{001b}'));
    assert!(redacted.contains("REDACTED"));
}

#[cfg(unix)]
#[test]
fn local_sources_resolve_symlinked_existing_ancestors() {
    use std::{fs, os::unix::fs::symlink};

    let temporary = tempfile::tempdir().expect("temporary directory");
    let real_root = temporary.path().join("real-source");
    let source_alias = temporary.path().join("source-alias");
    fs::create_dir_all(&real_root).expect("create real source root");
    symlink(&real_root, &source_alias).expect("create source alias");

    let requested = source_alias.join("missing-skill");
    let parsed = parse_source(
        requested
            .to_str()
            .expect("temporary test path should be UTF-8"),
    )
    .expect("parse local source");

    assert_eq!(
        parsed,
        ParsedSource::Local {
            path: real_root
                .canonicalize()
                .expect("canonical real source")
                .join("missing-skill"),
        }
    );
}
