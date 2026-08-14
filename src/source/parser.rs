use std::{
    env,
    ffi::OsStr,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Context, Result};
use url::Url;

use crate::path_utils::{ensure_safe_relative_path, resolve_existing_ancestors};

const SOURCE_ALIASES: &[(&str, &str)] = &[
    ("coinbase/agentWallet", "coinbase/agentic-wallet-skills"),
    ("vercel-labs/vercel-skills", "vercel-labs/agent-skills"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedSource {
    Local {
        path: PathBuf,
    },
    Git {
        clone_url: String,
        reference: Option<String>,
        subpath: Option<PathBuf>,
        skill_filter: Option<String>,
    },
}

pub fn parse_source(input: &str) -> Result<ParsedSource> {
    let input = input.trim();
    if input.is_empty() {
        bail!("source cannot be empty");
    }

    let expanded_local = expand_tilde(input);
    if looks_like_local_path(input) || expanded_local.exists() {
        return Ok(ParsedSource::Local {
            path: absolute_path(expanded_local)?,
        });
    }

    if let Ok(url) = Url::parse(input) {
        if url.scheme() == "file" {
            let path = url
                .to_file_path()
                .map_err(|_| anyhow!("invalid file URL"))?;
            return Ok(ParsedSource::Local { path });
        }
    }

    let (without_fragment, fragment_ref, fragment_skill) = parse_fragment(input);
    validate_git_reference(fragment_ref.as_deref())?;
    let mut source = canonical_source_alias(without_fragment).to_owned();

    if let Some(rest) = source.strip_prefix("github:") {
        source = rest.to_owned();
    } else if let Some(rest) = source.strip_prefix("gitlab:") {
        source = format!("https://gitlab.com/{rest}");
    }

    if source.starts_with("git@") || source.starts_with("ssh://") {
        return Ok(ParsedSource::Git {
            clone_url: source,
            reference: fragment_ref,
            subpath: None,
            skill_filter: fragment_skill,
        });
    }

    if let Ok(url) = Url::parse(&source) {
        if matches!(url.scheme(), "http" | "https" | "git") {
            if is_github_host(&url) {
                return parse_github_url(&url, fragment_ref, fragment_skill);
            }
            if is_gitlab_host(&url) {
                return parse_gitlab_url(&url, fragment_ref, fragment_skill);
            }

            return Ok(ParsedSource::Git {
                clone_url: source,
                reference: fragment_ref,
                subpath: None,
                skill_filter: fragment_skill,
            });
        }
    }

    parse_github_shorthand(&source, fragment_ref, fragment_skill)
}

pub(super) fn browser_url_needs_ref_resolution(input: &str) -> bool {
    let (base, fragment_ref, _) = parse_fragment(input);
    if fragment_ref.is_some() {
        return false;
    }

    let Ok(url) = Url::parse(base) else {
        return false;
    };
    let segments = path_segments(&url);

    (is_github_host(&url) && browser_kind_at(&segments, 2).is_some())
        || (is_gitlab_host(&url) && gitlab_browser_kind_index(&segments).is_some())
}

fn parse_github_shorthand(
    input: &str,
    fragment_ref: Option<String>,
    fragment_skill: Option<String>,
) -> Result<ParsedSource> {
    if input.contains(':') || input.starts_with('.') || input.starts_with('/') {
        bail!("unrecognized source format");
    }

    let (path_part, suffix_skill) = split_skill_suffix(input);
    let path_part = canonical_source_alias(path_part);
    let segments = path_part
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if segments.len() < 2 {
        bail!(
            "GitHub shorthand must be owner/repository, optionally followed by a subpath or @skill"
        );
    }

    let owner = segments[0];
    let repository = segments[1].trim_end_matches(".git");
    validate_repository_segment(owner, "owner")?;
    validate_repository_segment(repository, "repository")?;

    let subpath = (segments.len() > 2).then(|| path_from_segments(&segments[2..]));
    validate_relative_subpath(subpath.as_deref())?;

    Ok(ParsedSource::Git {
        clone_url: format!("https://{}/{owner}/{repository}.git", github_host()),
        reference: fragment_ref,
        subpath,
        skill_filter: fragment_skill.or(suffix_skill),
    })
}

fn parse_github_url(
    url: &Url,
    fragment_ref: Option<String>,
    fragment_skill: Option<String>,
) -> Result<ParsedSource> {
    let segments = path_segments(url);
    if segments.len() < 2 {
        bail!("GitHub URL must identify an owner and repository");
    }

    let owner = segments[0];
    let repository = segments[1].trim_end_matches(".git");
    validate_repository_segment(owner, "owner")?;
    validate_repository_segment(repository, "repository")?;

    let browser_target = parse_browser_target(&segments, 2);
    let reference = fragment_ref.or_else(|| {
        browser_target
            .as_ref()
            .and_then(|target| target.reference.clone())
    });
    let subpath = browser_target.and_then(|target| target.subpath);
    validate_git_reference(reference.as_deref())?;
    validate_relative_subpath(subpath.as_deref())?;

    Ok(ParsedSource::Git {
        clone_url: repository_clone_url(url, &[owner, repository]),
        reference,
        subpath,
        skill_filter: fragment_skill,
    })
}

fn parse_gitlab_url(
    url: &Url,
    fragment_ref: Option<String>,
    fragment_skill: Option<String>,
) -> Result<ParsedSource> {
    let segments = path_segments(url);
    let separator = segments.iter().position(|segment| *segment == "-");
    let repository_end = separator.unwrap_or(segments.len());
    if repository_end < 2 {
        bail!("GitLab URL must identify a namespace and repository");
    }

    let repository_segments = &segments[..repository_end];
    let Some(repository) = repository_segments.last() else {
        bail!("GitLab URL must identify a repository");
    };
    let repository = repository.trim_end_matches(".git");
    let namespace = &repository_segments[..repository_segments.len() - 1];
    for segment in namespace {
        validate_repository_segment(segment, "namespace")?;
    }
    validate_repository_segment(repository, "repository")?;

    let browser_target = separator.and_then(|index| parse_browser_target(&segments, index + 1));
    let reference = fragment_ref.or_else(|| {
        browser_target
            .as_ref()
            .and_then(|target| target.reference.clone())
    });
    let subpath = browser_target.and_then(|target| target.subpath);
    validate_git_reference(reference.as_deref())?;
    validate_relative_subpath(subpath.as_deref())?;

    let mut repository_path = namespace.to_vec();
    repository_path.push(repository);
    Ok(ParsedSource::Git {
        clone_url: repository_clone_url(url, &repository_path),
        reference,
        subpath,
        skill_filter: fragment_skill,
    })
}

#[derive(Debug)]
struct BrowserTarget {
    reference: Option<String>,
    subpath: Option<PathBuf>,
}

fn parse_browser_target(segments: &[&str], kind_index: usize) -> Option<BrowserTarget> {
    let kind = browser_kind_at(segments, kind_index)?;
    let reference = segments
        .get(kind_index + 1)
        .map(|value| (*value).to_owned());
    let mut subpath = path_from_segments(segments.get(kind_index + 2..).unwrap_or_default());

    if kind == "blob" && subpath.file_name() == Some(OsStr::new("SKILL.md")) {
        subpath.pop();
    }

    Some(BrowserTarget {
        reference,
        subpath: (!subpath.as_os_str().is_empty()).then_some(subpath),
    })
}

fn browser_kind_at<'a>(segments: &'a [&str], index: usize) -> Option<&'a str> {
    segments
        .get(index)
        .copied()
        .filter(|kind| matches!(*kind, "tree" | "blob"))
}

fn gitlab_browser_kind_index(segments: &[&str]) -> Option<usize> {
    let separator = segments.iter().position(|segment| *segment == "-")?;
    browser_kind_at(segments, separator + 1).map(|_| separator + 1)
}

fn repository_clone_url(url: &Url, repository_segments: &[&str]) -> String {
    let mut clone_url = url.clone();
    clone_url.set_path(&format!("/{}.git", repository_segments.join("/")));
    // Browser fragments are selectors or document anchors, never part of a
    // clone URL. Query parameters may carry authentication and must survive.
    clone_url.set_fragment(None);
    clone_url.to_string()
}

fn parse_fragment(input: &str) -> (&str, Option<String>, Option<String>) {
    let Some((base, fragment)) = input.split_once('#') else {
        return (input, None, None);
    };
    if is_document_anchor(base, fragment) {
        return (input, None, None);
    }
    if fragment.is_empty() {
        return (base, None, None);
    }

    let (reference, skill) = match fragment.split_once('@') {
        Some((reference, skill)) => (reference, Some(skill)),
        None => (fragment, None),
    };
    (
        base,
        (!reference.is_empty()).then(|| reference.to_owned()),
        skill.filter(|skill| !skill.is_empty()).map(str::to_owned),
    )
}

fn is_document_anchor(input: &str, fragment: &str) -> bool {
    // A blob fragment belongs to the rendered document unless it uses the
    // manager's explicit `ref@skill` / `@skill` selector syntax.
    if fragment.contains('@') {
        return false;
    }

    let Ok(url) = Url::parse(input) else {
        return false;
    };
    let segments = path_segments(&url);

    (is_github_host(&url) && browser_kind_at(&segments, 2) == Some("blob"))
        || (is_gitlab_host(&url)
            && gitlab_browser_kind_index(&segments)
                .is_some_and(|index| browser_kind_at(&segments, index) == Some("blob")))
}

fn split_skill_suffix(input: &str) -> (&str, Option<String>) {
    let Some(index) = input.rfind('@') else {
        return (input, None);
    };
    if !input[..index].contains('/') {
        return (input, None);
    }

    let skill = &input[index + 1..];
    if skill.is_empty() {
        (input, None)
    } else {
        (&input[..index], Some(skill.to_owned()))
    }
}

fn canonical_source_alias(input: &str) -> &str {
    SOURCE_ALIASES
        .iter()
        .find(|(alias, _)| *alias == input)
        .map(|(_, canonical)| *canonical)
        .unwrap_or(input)
}

fn looks_like_local_path(input: &str) -> bool {
    input == "."
        || input == ".."
        || input.starts_with("./")
        || input.starts_with("../")
        || input.starts_with(".\\")
        || input.starts_with("..\\")
        || input.starts_with('/')
        || input.starts_with('\\')
        || input.starts_with('~')
        || is_windows_absolute_path(input)
}

fn is_windows_absolute_path(input: &str) -> bool {
    let bytes = input.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\')
}

fn expand_tilde(input: &str) -> PathBuf {
    if input == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(input));
    }
    if let Some(relative) = input
        .strip_prefix("~/")
        .or_else(|| input.strip_prefix("~\\"))
    {
        if let Some(home) = dirs::home_dir() {
            return home.join(relative);
        }
    }
    PathBuf::from(input)
}

fn absolute_path(path: PathBuf) -> Result<PathBuf> {
    let path = if path.is_absolute() {
        path
    } else {
        env::current_dir()
            .context("could not determine current directory")?
            .join(path)
    };
    Ok(resolve_existing_ancestors(path))
}

fn validate_relative_subpath(path: Option<&Path>) -> Result<()> {
    if let Some(path) = path {
        ensure_safe_relative_path(path, "repository subpath")?;
    }
    Ok(())
}

fn validate_repository_segment(value: &str, label: &str) -> Result<()> {
    let invalid = value.is_empty()
        || matches!(value, "." | "..")
        || value.chars().any(|character| {
            character.is_control()
                || character.is_whitespace()
                || matches!(character, '/' | '\\' | ':' | '@' | '#')
        });
    if invalid {
        bail!("invalid repository {label}: {value:?}");
    }
    Ok(())
}

pub(super) fn validate_git_reference(reference: Option<&str>) -> Result<()> {
    let Some(reference) = reference else {
        return Ok(());
    };
    let invalid = reference.is_empty()
        || reference.len() > 255
        || reference.starts_with('-')
        || reference.starts_with('/')
        || reference.ends_with('/')
        || reference.ends_with('.')
        || reference.ends_with(".lock")
        || reference == "@"
        || reference
            .split('/')
            .any(|component| component.starts_with('.'))
        || reference.contains("..")
        || reference.contains("@{")
        || reference.contains("//")
        || reference.chars().any(|character| {
            character.is_control()
                || character.is_whitespace()
                || matches!(character, '\\' | '~' | '^' | ':' | '?' | '*' | '[')
        });
    if invalid {
        bail!("unsafe or invalid Git reference: {reference:?}");
    }
    Ok(())
}

fn path_from_segments(segments: &[&str]) -> PathBuf {
    segments.iter().fold(PathBuf::new(), |mut path, segment| {
        path.push(segment);
        path
    })
}

fn path_segments(url: &Url) -> Vec<&str> {
    url.path_segments()
        .map(|segments| {
            segments
                .filter(|segment| !segment.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn is_github_host(url: &Url) -> bool {
    url.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("github.com")
            || host.eq_ignore_ascii_case("www.github.com")
            || host.eq_ignore_ascii_case(&github_host())
    })
}

fn is_gitlab_host(url: &Url) -> bool {
    url.host_str()
        .is_some_and(|host| host.eq_ignore_ascii_case("gitlab.com"))
}

fn github_host() -> String {
    let configured = env::var("GH_HOST")
        .ok()
        .map(|host| host.trim().to_owned())
        .filter(|host| !host.is_empty());
    let Some(configured) = configured else {
        return "github.com".to_owned();
    };

    let Ok(url) = Url::parse(&format!("https://{configured}")) else {
        return "github.com".to_owned();
    };
    let valid = url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.path() == "/"
        && url.query().is_none()
        && url.fragment().is_none();
    if valid {
        url.host_str().unwrap_or("github.com").to_owned()
    } else {
        "github.com".to_owned()
    }
}
