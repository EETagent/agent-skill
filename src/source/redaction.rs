use url::Url;

use crate::terminal::sanitize_inline;

pub fn redact_source(input: &str) -> String {
    let redacted = match Url::parse(input) {
        Ok(mut url) => {
            if !url.username().is_empty() {
                let _ = url.set_username("***");
            }
            if url.password().is_some() {
                let _ = url.set_password(Some("***"));
            }
            if url.query().is_some() {
                url.set_query(Some("REDACTED"));
            }
            url.to_string()
        }
        Err(_) => input.to_owned(),
    };
    sanitize_inline(&redacted)
}

pub(super) fn redact_sensitive_text(value: &str, source: &str) -> String {
    let mut redacted = value.replace(source, &redact_source(source));
    if let Ok(url) = Url::parse(source) {
        for secret in [url.username(), url.password().unwrap_or_default()] {
            if !secret.is_empty() {
                redacted = redacted.replace(secret, "***");
            }
        }
        for (_, value) in url.query_pairs() {
            if !value.is_empty() {
                redacted = redacted.replace(value.as_ref(), "***");
            }
        }
    }
    redacted
}
