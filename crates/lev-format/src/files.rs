// SPDX-License-Identifier: MIT OR Apache-2.0
//! Resolving a `file:` body from disk.
//!
//! `lev-core` is pure, so a [`Body::File`] reference is resolved here — the crate that
//! owns the on-disk format — before a request reaches an engine or the load path.
//! Resolving at load time means the bytes are read **once**, not once per iteration of
//! a load run, and that the engine never needs filesystem access at all.
//!
//! A path is relative to the collection directory and is not allowed to leave it. A
//! request file is plain text in a repository, often from a pull request: opening it
//! must not be enough to read `../../.ssh/id_rsa`, so the path is canonicalized (which
//! resolves `..` and symlinks) and then checked to still be under the root.

use std::path::Path;

use lev_core::{Body, MultipartPart, MultipartValue, RequestDef};

use crate::error::FormatError;

/// The largest body file that will be read.
///
/// The bytes are held for the life of a run and every virtual user sends a copy, so a
/// 2 GiB "fixture" would be a memory-exhaustion bug rather than a test.
pub const MAX_BODY_FILE_BYTES: u64 = 32 * 1024 * 1024;

/// Replace `file:` bodies with the bytes they name, read from `root`.
///
/// Both a whole-body `file:` and the `file:` value of a multipart part are resolved.
/// The content type is taken from the file's extension; an explicit request header or
/// part `contentType` still wins, so an unusual fixture can always be labelled by hand.
/// A request without a file body is left untouched.
pub fn resolve_body_files(root: &Path, request: &mut RequestDef) -> Result<(), FormatError> {
    match &request.body {
        Body::File(template) => {
            let reference = template.source();
            let content = read_body_file(root, reference)?;
            let content_type = content_type_for(Path::new(reference));
            request.body = Body::Bytes {
                content_type: Some(content_type),
                content,
            };
        }
        Body::Multipart(parts) => {
            let mut resolved = Vec::with_capacity(parts.len());
            for part in parts {
                let value = match &part.value {
                    MultipartValue::File {
                        path,
                        filename,
                        content_type,
                    } => {
                        let reference = path.source();
                        let content = read_body_file(root, reference)?;
                        let filename = filename
                            .clone()
                            .unwrap_or_else(|| default_filename(reference));
                        let content_type = content_type
                            .clone()
                            .unwrap_or_else(|| content_type_for(Path::new(reference)));
                        MultipartValue::Bytes {
                            filename,
                            content_type: Some(content_type),
                            content,
                        }
                    }
                    value => value.clone(),
                };
                resolved.push(MultipartPart {
                    name: part.name.clone(),
                    value,
                });
            }
            request.body = Body::Multipart(resolved);
        }
        _ => {}
    }
    Ok(())
}

fn default_filename(reference: &str) -> String {
    Path::new(reference)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(reference)
        .to_string()
}

fn read_body_file(root: &Path, reference: &str) -> Result<Vec<u8>, FormatError> {
    if reference.contains("{{") {
        return Err(FormatError::InvalidFileBody(format!(
            "`{reference}` must be a literal path: a file is read before any variable is \
             resolved"
        )));
    }

    let relative = Path::new(reference);
    if relative.is_absolute() {
        return Err(FormatError::InvalidFileBody(format!(
            "`{reference}` must be relative to the collection directory"
        )));
    }

    let root = canonical(root)?;
    let resolved = canonical(&root.join(relative))?;
    if !resolved.starts_with(&root) {
        return Err(FormatError::InvalidFileBody(format!(
            "`{reference}` escapes the collection directory"
        )));
    }

    let metadata = std::fs::metadata(&resolved).map_err(|source| FormatError::Io {
        path: resolved.display().to_string(),
        source,
    })?;
    if metadata.is_dir() {
        return Err(FormatError::InvalidFileBody(format!(
            "`{reference}` is a directory, not a file"
        )));
    }
    if metadata.len() > MAX_BODY_FILE_BYTES {
        return Err(FormatError::InvalidFileBody(format!(
            "`{reference}` is {} bytes, larger than the {} byte limit",
            metadata.len(),
            MAX_BODY_FILE_BYTES
        )));
    }

    std::fs::read(&resolved).map_err(|source| FormatError::Io {
        path: resolved.display().to_string(),
        source,
    })
}

fn canonical(path: &Path) -> Result<std::path::PathBuf, FormatError> {
    // An empty path is the current directory spelled the awkward way: a collection
    // root discovered from a relative request path (`levi request requests/x.yaml`)
    // is `""`, and `canonicalize` needs a real path. Canonicalizing also makes the
    // containment check below compare absolute paths, so `..` cannot slip past it.
    let path = if path.as_os_str().is_empty() {
        Path::new(".")
    } else {
        path
    };
    path.canonicalize().map_err(|source| FormatError::Io {
        path: path.display().to_string(),
        source,
    })
}

/// A body file's content type, from its extension.
///
/// A miss is `application/octet-stream`, never a guess at something a target might
/// execute as. The list is deliberately short: these are the extensions that turn up
/// as API fixtures, and anything else is honestly unlabelled.
fn content_type_for(path: &Path) -> String {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    let mime = match extension.as_deref() {
        Some("json") => "application/json",
        Some("txt" | "log") => "text/plain",
        Some("csv") => "text/csv",
        Some("html" | "htm") => "text/html",
        Some("xml") => "application/xml",
        Some("yaml" | "yml") => "application/yaml",
        Some("js") => "text/javascript",
        Some("pdf") => "application/pdf",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("zip") => "application/zip",
        Some("gz") => "application/gzip",
        Some("mp4") => "video/mp4",
        _ => "application/octet-stream",
    };
    mime.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lev_core::{Method, MultipartPart, MultipartValue};

    fn request_with_file(reference: &str) -> RequestDef {
        RequestDef {
            id: "upload".to_string(),
            name: "Upload".to_string(),
            method: Method::Post,
            url: lev_core::Template::parse("/upload").unwrap(),
            headers: Vec::new(),
            body: Body::File(lev_core::Template::parse(reference).unwrap()),
            auth: None,
            scripts: Default::default(),
            assertions: Vec::new(),
            timeout: None,
        }
    }

    /// A throwaway directory with `requests/` and `fixtures/`, as a collection is laid
    /// out. Returns the root; everything under it is removed by the caller's drop of
    /// `TempDir`.
    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir = std::env::temp_dir().join(format!("levi-files-{label}-{unique}"));
            std::fs::create_dir_all(dir.join("fixtures")).expect("create temp dir");
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn write(&self, relative: &str, bytes: &[u8]) {
            std::fs::write(self.0.join(relative), bytes).expect("write fixture");
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_text_file_becomes_its_bytes_with_a_guessed_content_type() {
        let dir = TempDir::new("text");
        dir.write("fixtures/order.json", b"{\"sku\":\"A1\"}");

        let mut request = request_with_file("fixtures/order.json");
        resolve_body_files(dir.path(), &mut request).expect("resolve");

        assert_eq!(
            request.body,
            Body::Bytes {
                content_type: Some("application/json".to_string()),
                content: b"{\"sku\":\"A1\"}".to_vec(),
            }
        );
    }

    #[test]
    fn a_binary_file_survives_byte_for_byte() {
        let dir = TempDir::new("binary");
        let png = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0xff, 0x00];
        dir.write("fixtures/logo.png", &png);

        let mut request = request_with_file("fixtures/logo.png");
        resolve_body_files(dir.path(), &mut request).expect("resolve");

        assert_eq!(
            request.body,
            Body::Bytes {
                content_type: Some("image/png".to_string()),
                content: png.to_vec(),
            }
        );
    }

    #[test]
    fn an_unknown_extension_is_labelled_honestly() {
        let dir = TempDir::new("unknown");
        dir.write("fixtures/blob.weird", b"?");

        let mut request = request_with_file("fixtures/blob.weird");
        resolve_body_files(dir.path(), &mut request).expect("resolve");

        let Body::Bytes { content_type, .. } = &request.body else {
            panic!("expected bytes, got {:?}", request.body);
        };
        assert_eq!(content_type.as_deref(), Some("application/octet-stream"));
    }

    #[test]
    fn a_path_that_escapes_the_collection_is_refused() {
        let dir = TempDir::new("escape");
        // A file one level *above* the collection root — the shape of the attack.
        let neighbour = dir.path().parent().unwrap().join("levi-secret.txt");
        std::fs::write(&neighbour, b"secret").expect("write neighbour");
        dir.write("fixtures/inside.txt", b"public");

        let mut request = request_with_file("../levi-secret.txt");
        let error = resolve_body_files(dir.path(), &mut request).expect_err("must refuse");
        assert!(error.to_string().contains("escapes"), "{error}");

        // Nothing was read, so the body is still the unresolved reference.
        assert!(matches!(request.body, Body::File(_)));
        let _ = std::fs::remove_file(&neighbour);
    }

    #[test]
    fn an_absolute_path_is_refused() {
        let dir = TempDir::new("absolute");
        // Built at runtime so the test is absolute on every platform.
        let absolute = dir.path().join("fixtures/order.json");
        let mut request = request_with_file(&absolute.to_string_lossy());
        let error = resolve_body_files(dir.path(), &mut request).expect_err("must refuse");
        assert!(error.to_string().contains("relative"), "{error}");
    }

    #[test]
    fn a_template_in_a_path_is_refused() {
        let dir = TempDir::new("template");
        let mut request = request_with_file("fixtures/{{name}}.json");
        let error = resolve_body_files(dir.path(), &mut request).expect_err("must refuse");
        assert!(error.to_string().contains("literal"), "{error}");
    }

    #[test]
    fn a_missing_file_is_reported_with_its_path() {
        let dir = TempDir::new("missing");
        let mut request = request_with_file("fixtures/absent.json");
        let error = resolve_body_files(dir.path(), &mut request).expect_err("must fail");
        assert!(error.to_string().contains("absent.json"), "{error}");
    }

    #[test]
    fn a_directory_is_not_a_body() {
        let dir = TempDir::new("dir");
        std::fs::create_dir_all(dir.path().join("fixtures/nested")).expect("mkdir");

        let mut request = request_with_file("fixtures/nested");
        let error = resolve_body_files(dir.path(), &mut request).expect_err("must fail");
        assert!(error.to_string().contains("directory"), "{error}");
    }

    #[test]
    fn a_request_without_a_file_body_is_untouched() {
        let dir = TempDir::new("untouched");
        let mut request = request_with_file("fixtures/ignored.json");
        request.body = Body::Raw {
            content_type: None,
            content: "hello".to_string(),
        };
        resolve_body_files(dir.path(), &mut request).expect("resolve");
        assert_eq!(
            request.body,
            Body::Raw {
                content_type: None,
                content: "hello".to_string(),
            }
        );
    }

    #[test]
    fn a_multipart_file_part_is_resolved_to_bytes() {
        let dir = TempDir::new("multipart");
        let png = [0x89, 0x50, 0x4e, 0x47, 0xff];
        dir.write("fixtures/logo.png", &png);

        let mut request = request_with_file("unused");
        request.body = Body::Multipart(vec![
            MultipartPart {
                name: "title".to_string(),
                value: MultipartValue::Text("{{title}}".to_string()),
            },
            MultipartPart {
                name: "file".to_string(),
                value: MultipartValue::File {
                    path: lev_core::Template::parse("fixtures/logo.png").unwrap(),
                    filename: None,
                    content_type: None,
                },
            },
        ]);

        resolve_body_files(dir.path(), &mut request).expect("resolve");

        let Body::Multipart(parts) = &request.body else {
            panic!("expected multipart, got {:?}", request.body);
        };
        assert_eq!(parts[0].value, MultipartValue::Text("{{title}}".into()));
        assert_eq!(
            parts[1].value,
            MultipartValue::Bytes {
                filename: "logo.png".to_string(),
                content_type: Some("image/png".to_string()),
                content: png.to_vec(),
            }
        );
    }
}
