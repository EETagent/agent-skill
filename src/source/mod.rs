use std::{ffi::OsStr, path::PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use tempfile::TempDir;

mod git;
mod parser;
mod redaction;

pub use parser::{parse_source, ParsedSource};
pub use redaction::redact_source;

use crate::{
    discovery::discover_source_skills, model::InstallableSkill, terminal::sanitize_inline,
};

#[derive(Debug)]
pub struct PreparedSource {
    pub original: String,
    pub resolved: String,
    pub skills: Vec<InstallableSkill>,
    _temporary_directory: Option<TempDir>,
}

impl PreparedSource {
    pub fn is_temporary(&self) -> bool {
        self._temporary_directory.is_some()
    }
}

pub fn prepare_source(input: &str, full_depth: bool) -> Result<PreparedSource> {
    let display_input = redact_source(input);
    match parse_source(input)? {
        ParsedSource::Local { path } => prepare_local_source(path, display_input, full_depth),
        ParsedSource::Git {
            clone_url,
            mut reference,
            mut subpath,
            skill_filter,
        } => {
            let temporary_directory = tempfile::Builder::new()
                .prefix("skillctl-source-")
                .tempdir()
                .context("failed to create temporary source directory")?;
            let repository = temporary_directory.path().join("repository");
            if parser::browser_url_needs_ref_resolution(input) {
                git::resolve_browser_reference(&clone_url, &mut reference, &mut subpath)?;
            }
            git::clone_repository(&clone_url, reference.as_deref(), &repository)?;

            let mut skills = discover_source_skills(&repository, subpath.as_deref(), full_depth)?;
            if let Some(filter) = skill_filter {
                skills.retain(|skill| skill.exact_name_match(&filter));
                if skills.is_empty() {
                    bail!("skill {filter:?} was not found in {display_input:?}");
                }
            }
            ensure_skills_found(&display_input, &skills)?;

            Ok(PreparedSource {
                original: display_input,
                resolved: redact_source(&clone_url),
                skills,
                _temporary_directory: Some(temporary_directory),
            })
        }
    }
}

fn prepare_local_source(
    path: PathBuf,
    display_input: String,
    full_depth: bool,
) -> Result<PreparedSource> {
    let base = if path.is_file() {
        if path.file_name() != Some(OsStr::new("SKILL.md")) {
            bail!(
                "local source file must be named SKILL.md: {}",
                path.display()
            );
        }
        path.parent()
            .ok_or_else(|| anyhow!("SKILL.md has no parent directory"))?
            .to_path_buf()
    } else {
        path
    };
    let skills = discover_source_skills(&base, None, full_depth)?;
    ensure_skills_found(&display_input, &skills)?;

    Ok(PreparedSource {
        original: display_input,
        resolved: sanitize_inline(&base.display().to_string()),
        skills,
        _temporary_directory: None,
    })
}

fn ensure_skills_found(input: &str, skills: &[InstallableSkill]) -> Result<()> {
    if skills.is_empty() {
        bail!("no SKILL.md directories were found in {input:?}");
    }
    Ok(())
}
