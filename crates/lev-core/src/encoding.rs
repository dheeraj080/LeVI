// SPDX-License-Identifier: MIT OR Apache-2.0
//! Base64: the encoding a body needs to cross a text-only boundary.
//!
//! A body is bytes, but the collection format is YAML and the wire protocol is JSON.
//! Both spell a binary body in base64, so the encoding lives here, next to the model,
//! and every representation agrees on one implementation.

use base64::Engine as _;

use crate::error::CoreError;

/// Encode `bytes` as standard, padded base64.
pub fn base64_encode(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Decode standard, padded base64.
///
/// Whitespace and the URL-safe alphabet are rejected: the text was produced by
/// [`base64_encode`] or by a tool that means the same thing, and guessing at
/// near-misses would hide a typo rather than report it.
pub fn base64_decode(text: &str) -> Result<Vec<u8>, CoreError> {
    base64::engine::general_purpose::STANDARD
        .decode(text)
        .map_err(|error| CoreError::InvalidBase64(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips_arbitrary_bytes() {
        for bytes in [
            Vec::new(),
            b"hello".to_vec(),
            vec![0x00, 0xff, 0xfe, 0x80, 0x7f],
            (0u8..=255).collect::<Vec<_>>(),
        ] {
            let text = base64_encode(&bytes);
            assert_eq!(base64_decode(&text).unwrap(), bytes, "{text}");
        }
    }

    #[test]
    fn malformed_base64_is_reported_not_guessed() {
        let error = base64_decode("not base64!!").expect_err("must fail");
        assert!(matches!(error, CoreError::InvalidBase64(_)), "{error}");
    }
}
