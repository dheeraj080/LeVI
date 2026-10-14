// SPDX-License-Identifier: MIT OR Apache-2.0
//! Round-trip and conformance tests for the collection format.
//!
//! The conformance corpus (`spec/corpus`) is the gate that keeps the on-disk format
//! stable: every document must parse, and must round-trip (parse → render → parse)
//! with no change.

use std::path::{Path, PathBuf};

use lev_format::{format_duration, parse_duration, DocumentFormat, Yaml};

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

#[test]
fn scenario_round_trips() {
    let format = Yaml;
    let source = read(&corpus_dir().join("load/checkout.yaml"));
    let scenario = format.scenario_from_str(&source).unwrap();

    assert_eq!(scenario.steps.len(), 2);
    assert_eq!(
        scenario.steps[0].think_time,
        Some(std::time::Duration::from_millis(500))
    );
    assert_eq!(scenario.steps[1].think_time, None);

    let rendered = format.scenario_to_string(&scenario).unwrap();
    let reparsed = format.scenario_from_str(&rendered).unwrap();
    assert_eq!(scenario, reparsed, "round-trip drift:\n{rendered}");
}

#[test]
fn every_corpus_scenario_round_trips() {
    let format = Yaml;
    for entry in std::fs::read_dir(corpus_dir().join("load")).unwrap() {
        let path = entry.unwrap().path();
        let source = read(&path);
        let scenario = format
            .scenario_from_str(&source)
            .unwrap_or_else(|error| panic!("{}: parse failed: {error}", path.display()));
        let rendered = format.scenario_to_string(&scenario).unwrap();
        let reparsed = format.scenario_from_str(&rendered).unwrap();
        assert_eq!(scenario, reparsed, "round-trip drift in {}", path.display());
    }
}

#[test]
fn durations_parse_and_format_canonically() {
    use std::time::Duration;

    assert_eq!(parse_duration("250ms").unwrap(), Duration::from_millis(250));
    assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
    assert_eq!(parse_duration("2m").unwrap(), Duration::from_secs(120));
    assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3600));
    // A bare number is seconds; a bare `us` is microseconds.
    assert_eq!(parse_duration("45").unwrap(), Duration::from_secs(45));
    assert_eq!(
        parse_duration("1500us").unwrap(),
        Duration::from_millis(1) + Duration::from_micros(500)
    );

    // Canonical form: the largest whole unit.
    assert_eq!(format_duration(Duration::from_millis(250)), "250ms");
    assert_eq!(format_duration(Duration::from_secs(30)), "30s");
    assert_eq!(format_duration(Duration::from_secs(120)), "2m");
    assert_eq!(format_duration(Duration::from_secs(3600)), "1h");
    assert_eq!(format_duration(Duration::from_secs(90)), "90s");
    assert_eq!(format_duration(Duration::ZERO), "0ms");

    for input in ["250ms", "30s", "2m", "1h", "90s", "1500us"] {
        let parsed = parse_duration(input).unwrap();
        let rendered = format_duration(parsed);
        assert_eq!(
            parse_duration(&rendered).unwrap(),
            parsed,
            "{input} did not round-trip through {rendered}"
        );
    }
}

#[test]
fn a_bad_duration_is_a_clear_error() {
    assert!(matches!(
        parse_duration("30 fortnights"),
        Err(lev_format::FormatError::InvalidDuration(_))
    ));
    assert!(matches!(
        parse_duration("soon"),
        Err(lev_format::FormatError::InvalidDuration(_))
    ));
}

#[test]
fn an_executor_and_steps_are_required() {
    let format = Yaml;

    // A missing `executor` key is a structural error — the schema requires it.
    let no_executor = "id: x\nname: X\nsteps:\n  - request: r\n";
    assert!(format.scenario_from_str(no_executor).is_err());

    // An `executor` block with no known variant is a scenario error.
    let empty_executor = "id: x\nname: X\nexecutor: {}\nsteps:\n  - request: r\n";
    assert!(matches!(
        format.scenario_from_str(empty_executor),
        Err(lev_format::FormatError::InvalidScenario(_))
    ));

    let no_steps =
        "id: x\nname: X\nexecutor:\n  constantVus: { vus: 1, duration: 1s }\nsteps: []\n";
    assert!(matches!(
        format.scenario_from_str(no_steps),
        Err(lev_format::FormatError::InvalidScenario(_))
    ));

    let zero_vus =
        "id: x\nname: X\nexecutor:\n  constantVus: { vus: 0, duration: 1s }\nsteps:\n  - request: r\n";
    assert!(matches!(
        format.scenario_from_str(zero_vus),
        Err(lev_format::FormatError::InvalidScenario(_))
    ));
}

#[test]
fn a_bare_number_duration_is_read_as_seconds() {
    let format = Yaml;
    let source = "id: x\nname: X\nexecutor:\n  constantVus:\n    vus: 2\n    duration: 15\nsteps:\n  - request: r\n";
    let scenario = format.scenario_from_str(source).unwrap();
    assert_eq!(
        scenario.executor.duration(),
        std::time::Duration::from_secs(15)
    );
}
