// SPDX-License-Identifier: MIT OR Apache-2.0
//! Round-trip and conformance tests for the collection format.
//!
//! The conformance corpus (`spec/corpus`) is the gate that keeps the on-disk format
//! stable: every document must parse, and must round-trip (parse → render → parse)
//! with no change.

use std::path::{Path, PathBuf};
use std::time::Duration;

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

#[test]
fn thresholds_and_abort_rules_parse_into_the_domain() {
    use lev_core::{Comparison, ThresholdMetric, ThresholdStat};

    let format = Yaml;
    let source = read(&corpus_dir().join("load/timed.yaml"));
    let scenario = format.scenario_from_str(&source).unwrap();

    // Metrics come out grouped and in name order; expressions keep their file order.
    let expressions: Vec<String> = scenario
        .thresholds
        .iter()
        .map(|threshold| format!("{} {}", threshold.metric.name(), threshold))
        .collect();
    assert_eq!(
        expressions,
        [
            "http_req_duration p(95)<500",
            "http_req_duration p(99.9)<1500",
            "http_req_failed rate<0.01",
            "http_reqs count>0",
        ],
        "thresholds did not parse as written"
    );

    let p95 = scenario.thresholds[0];
    assert_eq!(p95.stat, ThresholdStat::Percentile(95.0));
    assert_eq!(p95.comparison, Comparison::Lt);
    assert_eq!(p95.value, 500.0);

    // `rate` is a fraction, so 0.01 is one percent — not 0.01 of a percent.
    let rate = scenario.thresholds[2];
    assert_eq!(rate.stat, ThresholdStat::Rate);
    assert_eq!(rate.value, 0.01);

    assert_eq!(scenario.abort_on, vec![ThresholdMetric::HttpReqFailed]);
}

#[test]
fn a_bytes_body_round_trips_as_base64() {
    use lev_core::Body;

    let format = Yaml;
    let source = "id: up\nname: Up\nmethod: POST\nurl: /upload\nbody:\n  bytes:\n    contentType: image/png\n    base64: iVBORw0KGgo=\n";
    let request = format.request_from_str(source).unwrap();

    let Body::Bytes {
        content_type,
        content,
    } = &request.body
    else {
        panic!("expected a bytes body, got {:?}", request.body);
    };
    assert_eq!(content_type.as_deref(), Some("image/png"));
    assert_eq!(content, &[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

    // Rendering spells the same bytes the same way, so a saved document is stable.
    let rendered = format.request_to_string(&request).unwrap();
    assert!(rendered.contains("base64: iVBORw0KGgo="), "{rendered}");
    assert_eq!(
        format.request_from_str(&rendered).unwrap().body,
        request.body
    );
}

#[test]
fn a_file_body_survives_a_round_trip_unresolved() {
    // The loader resolves a `file` body to bytes; the *document* keeps the reference,
    // so rendering a parsed collection returns what the author wrote.
    let format = Yaml;
    let source = "id: up\nname: Up\nmethod: POST\nurl: /upload\nbody:\n  file: fixtures/logo.png\n";
    let request = format.request_from_str(source).unwrap();

    assert!(matches!(request.body, lev_core::Body::File(_)));
    let rendered = format.request_to_string(&request).unwrap();
    assert!(rendered.contains("file: fixtures/logo.png"), "{rendered}");
}

#[test]
fn timeouts_parse_into_the_domain_and_round_trip() {
    let format = Yaml;

    let request = format
        .request_from_str("id: r\nname: R\nmethod: GET\nurl: https://api.test/slow\ntimeout: 5s\n")
        .unwrap();
    assert_eq!(request.timeout, Some(Duration::from_secs(5)));

    // Absent means "use the default" — the parser must not invent a deadline.
    let bare = format
        .request_from_str("id: r\nname: R\nmethod: GET\nurl: https://api.test/\n")
        .unwrap();
    assert_eq!(bare.timeout, None);

    let scenario = format
        .scenario_from_str(
            "id: s\nname: S\nexecutor:\n  constantVus: { vus: 1, duration: 1s }\nsteps:\n  - request: r\ntimeout: 30s\n",
        )
        .unwrap();
    assert_eq!(scenario.timeout, Some(Duration::from_secs(30)));

    // And the rendered form keeps the authored value, spelled canonically.
    let rendered = format.scenario_to_string(&scenario).unwrap();
    assert!(rendered.contains("timeout: 30s"), "{rendered}");
}

#[test]
fn a_bad_threshold_or_abort_rule_is_a_scenario_error() {
    let format = Yaml;
    let base = "id: x\nname: X\nexecutor:\n  constantVus: { vus: 1, duration: 1s }\nsteps:\n  - request: r\n";

    // A percentile of a counter can never mean anything.
    let bad_stat = format!("{base}thresholds:\n  http_reqs:\n    - p(95)<500\n");
    assert!(matches!(
        format.scenario_from_str(&bad_stat),
        Err(lev_format::FormatError::InvalidScenario(_))
    ));

    // An abort rule needs a threshold to fire on.
    let dangling_abort = format!("{base}abortOn:\n  - http_req_failed\n");
    assert!(matches!(
        format.scenario_from_str(&dangling_abort),
        Err(lev_format::FormatError::InvalidScenario(_))
    ));

    // ... and it has to name a metric we produce.
    let unknown_abort =
        format!("{base}thresholds:\n  http_req_failed:\n    - rate<0.01\nabortOn:\n  - nope\n");
    assert!(matches!(
        format.scenario_from_str(&unknown_abort),
        Err(lev_format::FormatError::InvalidScenario(_))
    ));
}

#[test]
fn a_body_takes_exactly_one_key() {
    let format = Yaml;

    // The one-key form *is* the shape: it parses.
    let single = format
        .request_from_str("id: r\nname: R\nmethod: POST\nurl: /x\nbody:\n  json: |\n    {}\n")
        .unwrap();
    assert!(matches!(single.body, lev_core::Body::Json(_)));

    // Two keys is an error naming both, not a silent pick that drops what the author
    // also wrote.
    let two = format
        .request_from_str(
            "id: r\nname: R\nmethod: POST\nurl: /x\nbody:\n  json: \"{}\"\n  file: fixtures/payload.txt\n",
        )
        .unwrap_err();
    let message = two.to_string();
    assert!(message.contains("exactly one"), "{message}");
    assert!(
        message.contains("`json`") && message.contains("`file`"),
        "{message}"
    );

    // An empty body is the same mistake in the other direction.
    let empty = format
        .request_from_str("id: r\nname: R\nmethod: POST\nurl: /x\nbody: {}\n")
        .unwrap_err();
    assert!(empty.to_string().contains("empty body"), "{empty}");

    // And a misspelled key is named, rather than read as "nothing matched".
    let typo = format
        .request_from_str("id: r\nname: R\nmethod: POST\nurl: /x\nbody:\n  jsons: \"{}\"\n")
        .unwrap_err();
    assert!(typo.to_string().contains("jsons"), "{typo}");
}
