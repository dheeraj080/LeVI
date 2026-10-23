// SPDX-License-Identifier: MIT OR Apache-2.0
//! Response content decoding.
//!
//! The transport advertises `gzip`, `deflate` and `br` and decodes them before the
//! body is handed to the caller. Decoding happens after the wire body has been read
//! in full, because that is the only point where the headers and the bytes are both
//! available and because the interactive path buffers anyway.
//!
//! A compressed body is attacker-controlled in the same way any body is, so a
//! decompressed size cap exists even when the caller sets no response limit. Without
//! one, a few kilobytes of gzip can ask a client for gigabytes of memory.

use std::io::Read;

use bytes::Bytes;

use crate::error::HttpError;

/// The most a compressed body may expand to when the caller set no explicit limit.
pub(crate) const MAX_DECOMPRESSED_BYTES: u64 = 128 * 1024 * 1024;

/// Decode every coding in `content_encoding`, innermost first.
pub(crate) fn decode(
    content_encoding: &str,
    body: Bytes,
    max: Option<u64>,
) -> Result<Bytes, HttpError> {
    let limit = max.unwrap_or(MAX_DECOMPRESSED_BYTES);
    let codings: Vec<&str> = content_encoding
        .split(',')
        .map(str::trim)
        .filter(|coding| !coding.is_empty())
        .collect();

    let mut decoded = body;
    for coding in codings.iter().rev() {
        let coding = coding.to_ascii_lowercase();
        decoded = match coding.as_str() {
            "identity" => continue,
            "gzip" | "x-gzip" => Bytes::from(read_capped(
                flate2::read::GzDecoder::new(decoded.as_ref()),
                limit,
                &coding,
            )?),
            "deflate" => {
                // RFC 9110 says `deflate` is the zlib wrapper, but raw DEFLATE was
                // common enough historically that browsers and clients try both.
                match read_capped(
                    flate2::read::ZlibDecoder::new(decoded.as_ref()),
                    limit,
                    &coding,
                ) {
                    Ok(bytes) => Bytes::from(bytes),
                    Err(_) => Bytes::from(read_capped(
                        flate2::read::DeflateDecoder::new(decoded.as_ref()),
                        limit,
                        &coding,
                    )?),
                }
            }
            "br" => Bytes::from(read_capped(
                brotli::Decompressor::new(decoded.as_ref(), 4096),
                limit,
                &coding,
            )?),
            other => return Err(HttpError::UnsupportedEncoding(other.to_string())),
        };
    }
    Ok(decoded)
}

fn read_capped(mut reader: impl Read, limit: u64, coding: &str) -> Result<Vec<u8>, HttpError> {
    let mut out = Vec::new();
    let mut buffer = [0u8; 16 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| HttpError::Body(format!("failed to decode `{coding}`: {error}")))?;
        if read == 0 {
            return Ok(out);
        }
        if out.len() as u64 + read as u64 > limit {
            return Err(HttpError::ResponseTooLarge { limit });
        }
        out.extend_from_slice(&buffer[..read]);
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn gzip(input: &[u8]) -> Vec<u8> {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(input).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn gzip_is_decoded() {
        let encoded = gzip(b"{\"ok\":true}");
        let decoded = decode("gzip", Bytes::from(encoded), None).unwrap();
        assert_eq!(&decoded[..], b"{\"ok\":true}");
    }

    #[test]
    fn two_codings_are_decoded_in_reverse_order() {
        // gzip then brotli: `Content-Encoding: gzip, br` means the body is
        // brotli(gzip(payload)), so decoding takes br off first and then gzip.
        let gzipped = gzip(b"hello");
        let mut compressed = Vec::new();
        {
            let mut writer = brotli::CompressorWriter::new(&mut compressed, 4096, 5, 22);
            writer.write_all(&gzipped).unwrap();
        }
        let decoded = decode("gzip, br", Bytes::from(compressed), None).unwrap();
        assert_eq!(&decoded[..], b"hello");
    }

    #[test]
    fn an_unknown_coding_is_an_error_not_a_guess() {
        let error = decode("zstd", Bytes::from_static(b"?"), None).unwrap_err();
        assert!(
            matches!(error, HttpError::UnsupportedEncoding(_)),
            "{error:?}"
        );
    }

    #[test]
    fn a_decompression_bomb_is_bounded() {
        let encoded = gzip(&vec![b'a'; 64 * 1024]);
        let error = decode("gzip", Bytes::from(encoded), Some(1024)).unwrap_err();
        assert!(
            matches!(error, HttpError::ResponseTooLarge { .. }),
            "{error:?}"
        );
    }
}
