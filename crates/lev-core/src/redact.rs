// SPDX-License-Identifier: MIT OR Apache-2.0
//! Redaction.
//!
//! Redaction is a security requirement, not polish: the same [`Redactor`] is used by
//! every log line, report, and export path. It scrubs sensitive headers by name and
//! every resolved secret value by content, so an accidental echo is caught too.

use std::collections::BTreeSet;

/// Headers masked by default, matched case-insensitively.
const DEFAULT_SENSITIVE_HEADERS: &[&str] = &[
    "authorization",
    "proxy-authorization",
    "cookie",
    "set-cookie",
    "x-api-key",
    "x-auth-token",
    "x-amz-security-token",
];

/// What a masked value is replaced with.
pub const MASK: &str = "***REDACTED***";

/// Scrubs secrets and sensitive headers out of anything about to be shown or stored.
#[derive(Debug, Clone)]
pub struct Redactor {
    sensitive_headers: BTreeSet<String>,
    secrets: Vec<String>,
}

impl Default for Redactor {
    fn default() -> Self {
        Self::new()
    }
}

impl Redactor {
    /// A redactor with the default sensitive-header set and no known secret values.
    pub fn new() -> Self {
        let mut sensitive_headers = BTreeSet::new();
        for header in DEFAULT_SENSITIVE_HEADERS {
            sensitive_headers.insert((*header).to_string());
        }
        Self {
            sensitive_headers,
            secrets: Vec::new(),
        }
    }

    /// A redactor that also scrubs each of these literal secret values.
    pub fn with_secrets<I, S>(secrets: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut redactor = Self::new();
        for secret in secrets {
            redactor.add_secret(secret);
        }
        redactor
    }

    /// Mark an additional header name as sensitive (case-insensitive).
    pub fn add_sensitive_header(&mut self, name: impl Into<String>) {
        self.sensitive_headers
            .insert(name.into().to_ascii_lowercase());
    }

    /// Register a literal secret value that must never appear in output.
    ///
    /// Empty values are ignored: replacing `""` everywhere would be catastrophic.
    pub fn add_secret(&mut self, value: impl Into<String>) {
        let value = value.into();
        if !value.is_empty() {
            self.secrets.push(value);
        }
    }

    pub fn is_sensitive_header(&self, name: &str) -> bool {
        self.sensitive_headers.contains(&name.to_ascii_lowercase())
    }

    /// Replace every known secret value in `input` with [`MASK`].
    pub fn redact(&self, input: &str) -> String {
        let mut out = input.to_string();
        // Longest first: a secret that is a substring of another must be replaced
        // by its longer match to avoid leaving a fragment behind.
        let mut secrets: Vec<&String> = self.secrets.iter().collect();
        secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
        for secret in secrets {
            if out.contains(secret.as_str()) {
                out = out.replace(secret.as_str(), MASK);
            }
        }
        out
    }

    /// Redact a header value: mask it entirely if the header is sensitive,
    /// otherwise scrub any secret content from it.
    pub fn redact_header_value(&self, name: &str, value: &str) -> String {
        if self.is_sensitive_header(name) {
            MASK.to_string()
        } else {
            self.redact(value)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_sensitive_headers_case_insensitively() {
        let redactor = Redactor::new();
        assert_eq!(
            redactor.redact_header_value("Authorization", "Bearer abc"),
            MASK
        );
        assert_eq!(
            redactor.redact_header_value("authorization", "Bearer abc"),
            MASK
        );
        assert_eq!(redactor.redact_header_value("Cookie", "sid=1"), MASK);
    }

    #[test]
    fn leaves_ordinary_headers_alone() {
        let redactor = Redactor::new();
        assert_eq!(
            redactor.redact_header_value("Accept", "application/json"),
            "application/json"
        );
    }

    #[test]
    fn scrubs_known_secret_values_anywhere() {
        let redactor = Redactor::with_secrets(["s3cr3t-token"]);
        assert_eq!(
            redactor.redact("GET /orders?token=s3cr3t-token HTTP/1.1"),
            format!("GET /orders?token={MASK} HTTP/1.1")
        );
    }

    #[test]
    fn scrubs_secrets_in_non_sensitive_headers() {
        let redactor = Redactor::with_secrets(["abc123"]);
        assert_eq!(redactor.redact_header_value("X-Trace", "abc123"), MASK);
    }

    #[test]
    fn ignores_empty_secrets() {
        let redactor = Redactor::with_secrets([""]);
        assert_eq!(redactor.redact("nothing to hide"), "nothing to hide");
    }
}
