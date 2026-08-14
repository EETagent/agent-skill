use std::{env, time::Duration};

use anyhow::{anyhow, bail, Context, Result};
use reqwest::{blocking::Client, Url};
use serde::{Deserialize, Serialize};

use crate::{io_utils::read_limited, terminal::sanitize_inline};

const DEFAULT_API_BASE: &str = "https://skills.sh";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteSkill {
    pub name: String,
    pub slug: String,
    pub source: String,
    pub installs: u64,
}

impl RemoteSkill {
    /// Returns the repository source accepted by `skillctl install`.
    pub fn repository_source(&self) -> String {
        if !self.source.trim().is_empty() {
            return self.source.clone();
        }

        let repository = self.slug.split('/').take(2).collect::<Vec<_>>().join("/");
        if repository.is_empty() {
            self.slug.clone()
        } else {
            repository
        }
    }

    /// Returns a single source expression that also selects this skill.
    pub fn install_source(&self) -> String {
        let repository = self.repository_source();
        if repository.is_empty() {
            return self.slug.clone();
        }

        if let Some((base, fragment)) = repository.split_once('#') {
            let reference = fragment
                .split_once('@')
                .map_or(fragment, |(reference, _)| reference);
            return if reference.is_empty() {
                format!("{base}#@{}", self.name)
            } else {
                format!("{base}#{reference}@{}", self.name)
            };
        }

        if repository.contains("://") || repository.starts_with("git@") {
            format!("{repository}#@{}", self.name)
        } else {
            let repository = repository
                .rsplit_once('@')
                .filter(|(base, _)| base.contains('/'))
                .map_or(repository.as_str(), |(base, _)| base);
            format!("{repository}@{}", self.name)
        }
    }
}

#[derive(Clone)]
pub struct RemoteClient {
    client: Client,
    base_url: Url,
}

impl std::fmt::Debug for RemoteClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let base_url = redacted_base_url(&self.base_url);
        formatter
            .debug_struct("RemoteClient")
            .field("base_url", &base_url)
            .finish_non_exhaustive()
    }
}

impl RemoteClient {
    pub fn from_environment() -> Result<Self> {
        let base = env::var("SKILLS_API_URL").unwrap_or_else(|_| DEFAULT_API_BASE.to_owned());
        Self::new(&base)
    }

    pub fn new(base_url: &str) -> Result<Self> {
        let mut base_url = Url::parse(base_url).context("invalid skills API URL")?;
        if !matches!(base_url.scheme(), "http" | "https") {
            bail!("skills API URL must use http or https");
        }
        if base_url.host_str().is_none() {
            bail!("skills API URL must include a host");
        }

        base_url.set_query(None);
        base_url.set_fragment(None);
        if !base_url.path().ends_with('/') {
            let path = format!("{}/", base_url.path());
            base_url.set_path(&path);
        }

        let client = Client::builder()
            .user_agent(concat!("skillctl/", env!("CARGO_PKG_VERSION")))
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .context("failed to create HTTP client")?;

        Ok(Self { client, base_url })
    }

    pub fn search(
        &self,
        query: &str,
        owner: Option<&str>,
        limit: usize,
    ) -> Result<Vec<RemoteSkill>> {
        let query = query.trim();
        if query.is_empty() {
            bail!("search query cannot be empty");
        }
        if !(1..=100).contains(&limit) {
            bail!("remote search limit must be between 1 and 100");
        }

        let mut url = self
            .base_url
            .join("api/search")
            .context("failed to build skills search URL")?;
        {
            let mut parameters = url.query_pairs_mut();
            parameters
                .append_pair("q", query)
                .append_pair("limit", &limit.to_string());
            if let Some(owner) = owner.map(str::trim).filter(|owner| !owner.is_empty()) {
                validate_owner(owner)?;
                parameters.append_pair("owner", owner);
            }
        }

        let response = self.client.get(url).send().map_err(|error| {
            let detail = if error.is_timeout() {
                "request timed out"
            } else {
                "connection or transport error"
            };
            anyhow!("skills search request failed: {detail}")
        })?;
        let status = response.status();
        if !status.is_success() {
            bail!("skills search service returned HTTP {status}");
        }

        let response = read_limited(response, MAX_RESPONSE_BYTES)
            .map_err(|_| anyhow!("failed to read skills search response"))?;
        if response.truncated {
            bail!("skills search service response exceeded 2 MiB");
        }
        let body = serde_json::from_slice::<SearchResponse>(&response.bytes)
            .map_err(|_| anyhow!("skills search service returned an invalid JSON response"))?;

        let mut skills = body
            .skills
            .into_iter()
            .map(|skill| RemoteSkill {
                name: sanitize_inline(&skill.name),
                slug: sanitize_inline(&skill.id),
                source: sanitize_inline(&skill.source.unwrap_or_default()),
                installs: skill.installs.unwrap_or_default(),
            })
            .filter(|skill| !skill.name.is_empty() && !skill.slug.is_empty())
            .collect::<Vec<_>>();
        skills.sort_by(|left, right| {
            right
                .installs
                .cmp(&left.installs)
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });
        skills.truncate(limit);
        Ok(skills)
    }
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    skills: Vec<SearchSkillResponse>,
}

#[derive(Debug, Deserialize)]
struct SearchSkillResponse {
    id: String,
    name: String,
    #[serde(default)]
    installs: Option<u64>,
    #[serde(default)]
    source: Option<String>,
}

fn redacted_base_url(url: &Url) -> String {
    let mut redacted = url.clone();
    if !redacted.username().is_empty() {
        let _ = redacted.set_username("***");
    }
    if redacted.password().is_some() {
        let _ = redacted.set_password(Some("***"));
    }
    redacted.to_string()
}

fn validate_owner(owner: &str) -> Result<()> {
    let valid = owner.len() <= 39
        && owner
            .bytes()
            .enumerate()
            .all(|(index, byte)| byte.is_ascii_alphanumeric() || (byte == b'-' && index > 0))
        && !owner.ends_with('-');
    if !valid {
        bail!("owner must be a valid GitHub owner name");
    }
    Ok(())
}
