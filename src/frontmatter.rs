use std::{fs::File, path::Path};

use anyhow::{Context, Result};
use serde_yaml::{Mapping, Value};

pub use crate::model::SkillMetadata as ParsedSkillDocument;

use crate::{io_utils::read_limited, terminal::sanitize_inline};

const MAX_PREVIEW_BYTES: usize = 2 * 1024 * 1024;

pub fn read_skill_document(skill_md: &Path) -> Result<ParsedSkillDocument> {
    read_skill_document_in_directory(skill_md, skill_md.parent().unwrap_or(skill_md))
}

pub(crate) fn read_skill_document_in_directory(
    skill_md: &Path,
    skill_directory: &Path,
) -> Result<ParsedSkillDocument> {
    let file =
        File::open(skill_md).with_context(|| format!("failed to open {}", skill_md.display()))?;
    let mut read = read_limited(file, MAX_PREVIEW_BYTES)
        .with_context(|| format!("failed to read {}", skill_md.display()))?;

    let invalid_utf8 = match std::str::from_utf8(&read.bytes) {
        Ok(_) => false,
        Err(error) if read.truncated && error.error_len().is_none() => {
            read.bytes.truncate(error.valid_up_to());
            false
        }
        Err(_) => true,
    };
    let mut warnings = Vec::new();

    if read.truncated {
        warnings.push(format!(
            "preview truncated at {} MiB",
            MAX_PREVIEW_BYTES / 1024 / 1024
        ));
    }

    let content = String::from_utf8_lossy(&read.bytes).into_owned();
    if invalid_utf8 {
        warnings
            .push("SKILL.md is not valid UTF-8; preview uses replacement characters".to_owned());
    }

    Ok(parse_skill_document(&content, skill_directory, warnings))
}

pub fn parse_skill_document(
    raw: &str,
    skill_directory: &Path,
    mut warnings: Vec<String>,
) -> ParsedSkillDocument {
    let raw = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let (frontmatter, body) = split_frontmatter(raw);
    let mapping = parse_frontmatter(frontmatter, &mut warnings);

    let folder_name = skill_directory
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("skill");
    let heading = first_level_one_heading(body);

    let name = mapping
        .as_ref()
        .and_then(|mapping| scalar_string(mapping, "name", &mut warnings))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| folder_name.to_owned());
    let title = mapping
        .as_ref()
        .and_then(|mapping| scalar_string(mapping, "title", &mut warnings))
        .filter(|value| !value.is_empty())
        .or(heading)
        .unwrap_or_else(|| name.clone());
    let description = mapping
        .as_ref()
        .and_then(|mapping| scalar_string(mapping, "description", &mut warnings))
        .unwrap_or_default();

    if frontmatter.is_none() {
        warnings.push("missing YAML frontmatter".to_owned());
    }
    if mapping
        .as_ref()
        .and_then(|mapping| mapping.get(Value::String("name".to_owned())))
        .is_none()
    {
        warnings.push("missing frontmatter field `name`; using directory name".to_owned());
    }
    if description.is_empty() {
        warnings.push("missing or empty frontmatter field `description`".to_owned());
    }

    ParsedSkillDocument {
        name: sanitize_inline(&name),
        title: sanitize_inline(&title),
        description: sanitize_inline(&description),
        content: raw.to_owned(),
        warnings,
    }
}

fn parse_frontmatter(frontmatter: Option<&str>, warnings: &mut Vec<String>) -> Option<Mapping> {
    let yaml = frontmatter?;
    match serde_yaml::from_str::<Value>(yaml) {
        Ok(Value::Mapping(mapping)) => Some(mapping),
        Ok(Value::Null) => None,
        Ok(_) => {
            warnings.push("frontmatter must be a YAML mapping".to_owned());
            None
        }
        Err(error) => {
            warnings.push(format!("invalid YAML frontmatter: {error}"));
            None
        }
    }
}

fn split_frontmatter(raw: &str) -> (Option<&str>, &str) {
    let Some(first_line_end) = raw.find('\n') else {
        return (None, raw);
    };
    if raw[..first_line_end].trim_end_matches('\r') != "---" {
        return (None, raw);
    }

    let yaml_start = first_line_end + 1;
    let mut offset = yaml_start;
    for line in raw[yaml_start..].split_inclusive('\n') {
        let line_without_newline = line.strip_suffix('\n').unwrap_or(line);
        if line_without_newline.trim_end_matches('\r') == "---" {
            let body_start = offset + line.len();
            return (Some(&raw[yaml_start..offset]), &raw[body_start..]);
        }
        offset += line.len();
    }

    (None, raw)
}

fn scalar_string(mapping: &Mapping, key: &str, warnings: &mut Vec<String>) -> Option<String> {
    let value = mapping.get(Value::String(key.to_owned()))?;
    match value {
        Value::String(value) => Some(value.trim().to_owned()),
        Value::Null => None,
        _ => {
            warnings.push(format!("frontmatter field `{key}` must be a string"));
            None
        }
    }
}

fn first_level_one_heading(body: &str) -> Option<String> {
    body.lines().find_map(|line| {
        let heading = line.trim_start().strip_prefix("# ")?.trim();
        (!heading.is_empty()).then(|| heading.to_owned())
    })
}
