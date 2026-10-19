// SPDX-License-Identifier: MIT OR Apache-2.0
//! Sandbox tests, including the Phase 2 exit criteria: a runaway script is killed by
//! the CPU budget *without* breaking the pool.

use std::collections::BTreeMap;
use std::time::Duration;

use lev_script::{Limits, Phase, Sandbox, ScriptError, ScriptInput, ScriptRequest, ScriptResponse};

fn sandbox() -> Sandbox {
    Sandbox::new(Limits::default()).expect("sandbox")
}

fn request() -> ScriptRequest {
    ScriptRequest {
        method: "GET".to_string(),
        url: "https://api.test/things".to_string(),
        headers: vec![("Accept".to_string(), "application/json".to_string())],
        body: String::new(),
        timeout: None,
    }
}

fn response() -> ScriptResponse {
    ScriptResponse {
        status: 200,
        headers: vec![("content-type".to_string(), "application/json".to_string())],
        body: "{\"ok\":true,\"id\":7}".to_string(),
        time_ms: 12.5,
    }
}

fn post_input() -> ScriptInput {
    ScriptInput {
        phase: Phase::PostResponse,
        request: request(),
        response: Some(response()),
        vars: BTreeMap::new(),
        seed: 1,
    }
}

#[test]
fn a_pre_request_script_can_mutate_the_request() {
    let sandbox = sandbox();
    let outcome = sandbox
        .run(
            r#"
            lev.request.method = 'POST';
            lev.request.url = lev.request.url + '?page=2';
            lev.request.headers['X-Trace'] = 'abc';
            lev.request.body = '{"ok":true}';
            "#,
            ScriptInput::pre_request(request()),
        )
        .expect("run");

    assert_eq!(outcome.request.method, "POST");
    assert_eq!(outcome.request.url, "https://api.test/things?page=2");
    assert_eq!(
        outcome.request.headers,
        vec![
            ("Accept".to_string(), "application/json".to_string()),
            ("X-Trace".to_string(), "abc".to_string()),
        ]
    );
    assert_eq!(outcome.request.body, "{\"ok\":true}");
}

#[test]
fn checks_are_recorded_with_pass_fail_and_reason() {
    let sandbox = sandbox();
    let outcome = sandbox
        .run(
            r#"
            lev.test('status is 200', function () { lev.expect(lev.response.status).toBe(200); });
            lev.test('body has ok', function () { lev.expect(lev.response.body).toContain('"ok":true'); });
            lev.test('this fails', function () { lev.expect(lev.response.status).toBe(404); });
            lev.test('boolean form', lev.response.status < 500);
            "#,
            post_input(),
        )
        .expect("run");

    assert_eq!(outcome.checks.len(), 4);
    assert!(outcome.checks[0].passed);
    assert!(outcome.checks[1].passed);
    assert!(!outcome.checks[2].passed);
    assert_eq!(outcome.checks[2].name, "this fails");
    assert!(
        outcome.checks[2]
            .message
            .as_deref()
            .unwrap_or_default()
            .contains("404"),
        "{:?}",
        outcome.checks[2]
    );
    assert!(outcome.checks[3].passed);
}

#[test]
fn the_whole_lev_api_is_available() {
    let sandbox = sandbox();
    let outcome = sandbox
        .run(
            r#"
            lev.test('json body is parsed', function () { lev.expect(lev.response.json.ok).toBe(true); });
            lev.test('header readable', function () { lev.expect(lev.response.headers['content-type']).toContain('json'); });
            lev.test('env round-trips', function () {
              lev.env.set('greeting', 'hi');
              lev.expect(lev.env.get('greeting')).toBe('hi');
              lev.expect(lev.env.has('greeting')).toBe(true);
            });
            lev.test('random int in range', function () {
              var n = lev.random.int(10, 20);
              lev.expect(n).toBeGreaterThanOrEqual(10);
              lev.expect(n).toBeLessThanOrEqual(20);
            });
            lev.test('uuid shape', function () { lev.expect(lev.random.uuid()).toHaveLength(36); });
            lev.log('scripts can log');
            "#,
            post_input(),
        )
        .expect("run");

    assert!(
        outcome.checks.iter().all(|check| check.passed),
        "{:?}",
        outcome.checks
    );
    assert_eq!(outcome.vars.get("greeting").map(String::as_str), Some("hi"));
    assert_eq!(outcome.logs, vec!["scripts can log".to_string()]);
}

#[test]
fn random_is_reproducible_under_a_seed() {
    let sandbox = sandbox();
    let script = "lev.env.set('v', String(lev.random.int(0, 1_000_000)));";
    let run = |seed: u64| {
        let mut input = post_input();
        input.seed = seed;
        sandbox.run(script, input).expect("run").vars
    };
    assert_eq!(run(99).get("v"), run(99).get("v"));
    assert_ne!(run(1).get("v"), run(2).get("v"));
}

#[test]
fn a_thrown_error_is_a_runtime_error() {
    let sandbox = sandbox();
    let error = sandbox
        .run("throw new Error('boom');", post_input())
        .expect_err("should fail");
    match error {
        ScriptError::Runtime(message) => assert!(message.contains("boom"), "{message}"),
        other => panic!("expected a runtime error, got {other:?}"),
    }
}

#[test]
fn a_syntax_error_is_a_compile_error() {
    let sandbox = sandbox();
    let error = sandbox
        .run("let x = ;", post_input())
        .expect_err("should fail");
    assert!(
        matches!(error, ScriptError::Compile(_)),
        "expected a compile error, got {error:?}"
    );
}

#[test]
fn an_infinite_loop_is_killed_by_the_cpu_budget_without_breaking_the_sandbox() {
    // The Phase 2 exit criterion, made executable.
    let sandbox =
        Sandbox::new(Limits::with_cpu_budget(Duration::from_millis(200))).expect("sandbox");

    let error = sandbox
        .run("while (true) {}", post_input())
        .expect_err("the runaway must be interrupted");
    assert!(
        matches!(error, ScriptError::Timeout { .. }),
        "expected a timeout, got {error:?}"
    );

    // The pool survives: the very next script runs normally on the same sandbox.
    let outcome = sandbox
        .run(
            "lev.test('still alive', true);",
            ScriptInput::pre_request(request()),
        )
        .expect("the sandbox must still work after a runaway");
    assert_eq!(outcome.checks.len(), 1);
    assert!(outcome.checks[0].passed);
}

#[test]
fn a_runaway_allocation_is_stopped_by_the_memory_limit() {
    let sandbox = Sandbox::new(Limits {
        cpu_budget: Duration::from_secs(30),
        memory_limit: 4 * 1024 * 1024,
    })
    .expect("sandbox");

    let error = sandbox
        .run(
            "var a = []; while (true) { a.push(new ArrayBuffer(1024 * 1024)); }",
            post_input(),
        )
        .expect_err("the allocation must be stopped");
    assert!(
        matches!(error, ScriptError::Memory(_)),
        "expected a memory error, got {error:?}"
    );

    // And the sandbox is still usable.
    sandbox
        .run("lev.test('ok', true);", post_input())
        .expect("still usable");
}

#[test]
fn a_pre_request_script_does_not_see_a_response() {
    let sandbox = sandbox();
    let outcome = sandbox
        .run(
            "lev.test('no response in pre-request', lev.response === null);",
            ScriptInput::pre_request(request()),
        )
        .expect("run");
    assert!(outcome.checks[0].passed, "{:?}", outcome.checks);
}
