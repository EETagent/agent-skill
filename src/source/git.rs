use std::{
    path::{Component, Path, PathBuf},
    process::{Command, Output},
};

use anyhow::{bail, Context, Result};

use super::{
    parser::validate_git_reference,
    redaction::{redact_sensitive_text, redact_source},
};
use crate::{path_utils::ensure_safe_relative_path, terminal::sanitize_multiline};

pub(super) fn resolve_browser_reference(
    clone_url: &str,
    reference: &mut Option<String>,
    subpath: &mut Option<PathBuf>,
) -> Result<()> {
    let (Some(initial_reference), Some(initial_subpath)) =
        (reference.as_deref(), subpath.as_deref())
    else {
        return Ok(());
    };

    let advertised = advertised_git_references(clone_url)?;
    let Some((resolved_reference, resolved_subpath)) =
        match_advertised_reference(initial_reference, initial_subpath, &advertised)
    else {
        return Ok(());
    };

    validate_git_reference(Some(&resolved_reference))?;
    if let Some(path) = resolved_subpath.as_deref() {
        ensure_safe_relative_path(path, "repository subpath")?;
    }
    *reference = Some(resolved_reference);
    *subpath = resolved_subpath;
    Ok(())
}

pub(super) fn clone_repository(
    url: &str,
    reference: Option<&str>,
    destination: &Path,
) -> Result<()> {
    let mut clone = Command::new("git");
    clone
        .args(["clone", "--depth", "1", "--no-tags"])
        .arg(url)
        .arg(destination);
    run_checked_git(&mut clone, "clone", url)?;

    if let Some(reference) = reference {
        let mut fetch = Command::new("git");
        fetch
            .arg("-C")
            .arg(destination)
            .args(["fetch", "--depth", "1", "origin"])
            .arg(reference);
        run_checked_git(&mut fetch, "fetch requested ref from", url)?;

        let mut checkout = Command::new("git");
        checkout
            .arg("-C")
            .arg(destination)
            .args(["checkout", "--detach", "FETCH_HEAD"]);
        run_checked_git(&mut checkout, "check out requested ref from", url)?;
    }

    Ok(())
}

fn advertised_git_references(url: &str) -> Result<Vec<String>> {
    let mut command = Command::new("git");
    command
        .args(["ls-remote", "--heads", "--tags", "--refs"])
        .arg(url);
    let output = run_checked_git(&mut command, "list refs from", url)?;

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_once('\t').map(|(_, name)| name))
        .filter_map(|name| {
            name.strip_prefix("refs/heads/")
                .or_else(|| name.strip_prefix("refs/tags/"))
        })
        .map(str::to_owned)
        .collect())
}

fn match_advertised_reference(
    initial_reference: &str,
    initial_subpath: &Path,
    advertised: &[String],
) -> Option<(String, Option<PathBuf>)> {
    let mut combined = initial_reference.to_owned();
    for component in initial_subpath.components() {
        let Component::Normal(component) = component else {
            continue;
        };
        combined.push('/');
        combined.push_str(&component.to_string_lossy());
    }

    let matched = advertised
        .iter()
        .filter(|candidate| {
            **candidate == combined
                || combined
                    .strip_prefix(candidate.as_str())
                    .is_some_and(|remainder| remainder.starts_with('/'))
        })
        .max_by_key(|candidate| candidate.len())?;
    let remainder = combined
        .strip_prefix(matched.as_str())
        .unwrap_or_default()
        .trim_start_matches('/');
    let subpath = (!remainder.is_empty()).then(|| {
        remainder
            .split('/')
            .fold(PathBuf::new(), |mut path, component| {
                path.push(component);
                path
            })
    });
    Some((matched.clone(), subpath))
}

fn run_checked_git(command: &mut Command, operation: &str, url: &str) -> Result<Output> {
    let output = command
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .context("failed to run git; install Git and ensure it is available on PATH")?;
    ensure_git_success(&output, operation, url)?;
    Ok(output)
}

fn ensure_git_success(output: &Output, operation: &str, url: &str) -> Result<()> {
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let detail = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    let detail = sanitize_git_detail(detail, url);
    bail!(
        "git could not {operation} {}: {}",
        redact_source(url),
        detail.trim()
    )
}

fn sanitize_git_detail(detail: &str, url: &str) -> String {
    // Redact on both sides of normalization. The first pass protects exact
    // raw subprocess output; the second catches credentials that terminal
    // control sequences may have split before they were removed.
    let detail = redact_sensitive_text(detail, url);
    let detail = sanitize_multiline(&detail);
    redact_sensitive_text(&detail, url)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{match_advertised_reference, sanitize_git_detail};

    #[test]
    fn terminal_sequences_cannot_split_credentials_around_redaction() {
        let source = "https://user:secret@example.com/repository?token=value";
        let detail = concat!(
            "fatal: https://u\u{001b}[31mser:se\u{001b}[0mcret@example.com/",
            "repository?token=va\u{001b}[1mlue"
        );

        let detail = sanitize_git_detail(detail, source);

        assert!(!detail.contains("user"));
        assert!(!detail.contains("secret"));
        assert!(!detail.contains("value"));
        assert!(!detail.contains('\u{001b}'));
    }

    #[test]
    fn longest_advertised_ref_wins_before_the_repository_subpath() {
        let advertised = vec![
            "feature".to_owned(),
            "feature/foo".to_owned(),
            "release/old".to_owned(),
        ];

        let resolved =
            match_advertised_reference("feature", Path::new("foo/skills/alpha"), &advertised);

        assert_eq!(
            resolved,
            Some((
                "feature/foo".to_owned(),
                Some(PathBuf::from("skills/alpha"))
            ))
        );
    }
}
