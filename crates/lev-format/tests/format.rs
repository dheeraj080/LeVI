// SPDX-License-Identifier: MIT OR Apache-2.0
//! Round-trip and conformance tests for the collection format.
//!
//! The conformance corpus (`spec/corpus`) is the gate that keeps the on-disk format
//! stable: every document must parse, and must round-trip (parse → render → parse)
//! with no change.

use std::path::{Path, PathBuf};

use lev_format::{DocumentFormat, Yaml};

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/corpus")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

#[test]
fn request_round_trips() {
    let format = Yaml;
    let source = read(&corpus_dir().join("requests/basic.yaml"));
    let request = format.request_from_str(&source).unwrap();
    let rendered = format.request_to_string(&request).unwrap();
    let reparsed = format.request_from_str(&rendered).unwrap();
    assert_eq!(request, reparsed);
}

#[test]
fn collection_round_trips() {
    let format = Yaml;
    let source = read(&corpus_dir().join("collections/basic.yaml"));
    let collection = format.collection_from_str(&source).unwrap();
    let rendered = format.collection_to_string(&collection).unwrap();
    let reparsed = format.collection_from_str(&rendered).unwrap();
    assert_eq!(collection, reparsed);
}

#[test]
fn every_corpus_document_round_trips() {
    let format = Yaml;

    for entry in std::fs::read_dir(corpus_dir().join("collections")).unwrap() {
        let path = entry.unwrap().path();
        let source = read(&path);
        let collection = format
            .collection_from_str(&source)
            .unwrap_or_else(|error| panic!("{}: parse failed: {error}", path.display()));
        let rendered = format.collection_to_string(&collection).unwrap();
        let reparsed = format.collection_from_str(&rendered).unwrap();
        assert_eq!(
            collection,
            reparsed,
            "round-trip drift in {}",
            path.display()
        );
    }

    for entry in std::fs::read_dir(corpus_dir().join("requests")).unwrap() {
        let path = entry.unwrap().path();
        let source = read(&path);
        let request = format
            .request_from_str(&source)
            .unwrap_or_else(|error| panic!("{}: parse failed: {error}", path.display()));
        let rendered = format.request_to_string(&request).unwrap();
        let reparsed = format.request_from_str(&rendered).unwrap();
        assert_eq!(request, reparsed, "round-trip drift in {}", path.display());
    }
}

#[test]
fn secret_references_are_not_flattened() {
    let format = Yaml;
    let source = read(&corpus_dir().join("collections/basic.yaml"));
    let collection = format.collection_from_str(&source).unwrap();
    let rendered = format.collection_to_string(&collection).unwrap();
    assert!(
        rendered.contains("secret: corpus_token"),
        "a secret reference must survive a round trip, got:\n{rendered}"
    );
}

#[test]
fn rendered_request_is_stable() {
    let format = Yaml;
    let source = read(&corpus_dir().join("requests/basic.yaml"));
    let request = format.request_from_str(&source).unwrap();
    let rendered = format.request_to_string(&request).unwrap();
    insta::assert_snapshot!("rendered_request", rendered);
}

#[test]
fn rendered_collection_is_stable() {
    let format = Yaml;
    let source = read(&corpus_dir().join("collections/basic.yaml"));
    let collection = format.collection_from_str(&source).unwrap();
    let rendered = format.collection_to_string(&collection).unwrap();
    insta::assert_snapshot!("rendered_collection", rendered);
}
