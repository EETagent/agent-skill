use std::{fs, path::Path};

use agent_skill::frontmatter::{parse_skill_document, read_skill_document};
use pretty_assertions::assert_eq;
use tempfile::tempdir;

#[test]
fn parses_metadata_and_preserves_preview_content() {
    let raw = concat!(
        "---\n",
        "name: clean-code\n",
        "title: Clean Code\n",
        "description: Keep changes readable\n",
        "---\n\n# Ignored fallback\n\nBody\n",
    );
    let document = parse_skill_document(raw, Path::new("skills/clean-code"), Vec::new());

    assert_eq!(document.name, "clean-code");
    assert_eq!(document.title, "Clean Code");
    assert_eq!(document.description, "Keep changes readable");
    assert_eq!(document.content, raw);
    assert!(document.warnings.is_empty());
}

#[test]
fn falls_back_to_heading_and_directory_name() {
    let raw = "# Human title\n\nBody\n";
    let document = parse_skill_document(raw, Path::new("skills/folder-name"), Vec::new());

    assert_eq!(document.name, "folder-name");
    assert_eq!(document.title, "Human title");
    assert_eq!(document.description, "");
    assert!(document
        .warnings
        .iter()
        .any(|warning| warning.contains("missing YAML frontmatter")));
}

#[test]
fn sanitizes_terminal_sequences_in_metadata() {
    let raw = concat!(
        "---\n",
        "name: \"safe\\u001b[31m-name\"\n",
        "title: \"Title\\u001b]0;owned\\u0007\"\n",
        "description: \"line\\n break\"\n",
        "---\n",
    );
    let document = parse_skill_document(raw, Path::new("skills/safe"), Vec::new());

    assert_eq!(document.name, "safe-name");
    assert_eq!(document.title, "Title");
    assert_eq!(document.description, "line break");
}

#[test]
fn bounds_large_skill_document_reads() {
    let temporary = tempdir().expect("temporary directory");
    let skill = temporary.path().join("large");
    fs::create_dir_all(&skill).expect("create skill directory");

    let mut content = concat!(
        "---\n",
        "name: large\n",
        "description: large preview fixture\n",
        "---\n",
    )
    .to_owned();
    content.push_str(&"x".repeat(2 * 1024 * 1024 + 4096));
    let skill_md = skill.join("SKILL.md");
    fs::write(&skill_md, content).expect("write large fixture");

    let document = read_skill_document(&skill_md).expect("read bounded preview");

    assert!(document.content.len() <= 2 * 1024 * 1024);
    assert!(document
        .warnings
        .iter()
        .any(|warning| warning.contains("preview truncated")));
}
