# ADR 0003 - Phase 2: scripting

- **Status:** accepted
- **Date:** 2026-10-13
- **Phase:** 2

## Context

Phase 2 adds the feature that makes Levi more than a pretty `curl`: user-authored
JavaScript that runs before a request is sent and after a response arrives, plus
declarative assertions on the response. The plan calls for "rquickjs sandbox, `lev.*`
API, interrupt + memory limits, pooled contexts; checks/assertions -> metrics; pre/post
hooks wired into `lev-http`" (§Phase 2), with the exit criterion:

> a request with a pre-request script, a post-response script, and three assertions; a
> deliberately infinite-looping script is killed by CPU budget without killing the run.

"Without killing the run" is the sentence that shaped every decision below. A load tool
that a bad script can hang is not a load tool. The engine's one job is to keep running,
so the sandbox has to be a *contained* failure domain: a thrown error, a runaway loop,
or an unbounded allocation must never poison the pooled runtime or the session.

## Decisions

### 1. `rquickjs`, not `boa`, and it builds with no `libclang`

`boa` is pure Rust but slower and further from spec parity. `rquickjs` wraps the real
QuickJS engine, which is small, fast and has the two hooks this phase needs: a memory
limit and an **interrupt handler** that QuickJS calls on every backward branch.

The usual objection to `rquickjs` is the `bindgen`/`libclang` build dependency. That is
only true with the `bindgen` feature **off defaults**: `rquickjs 0.9` ships
pre-generated bindings, so it builds with the default feature set on a machine with no
LLVM at all. This was verified with a throwaway probe *before* committing to it, which
also confirmed the two mechanisms this phase depends on:

- `Runtime::set_memory_limit(n)` kills a runaway allocator with an ordinary error.
- `Runtime::set_interrupt_handler(f)` stops `while (true) {}` in ~200 ms, with no hang.

### 2. The `lev.*` API is native primitives + a JavaScript prelude

The native (Rust) surface is deliberately tiny and deals only in JSON strings and
primitives: `emitCheck`, `log`, `randInt`, `randFloat`, `uuid`, `randString`, `envGet`,
`envSet`, `varsJson`, `requestJson`, `responseJson`. Everything ergonomic - `lev.test`,
`lev.expect` with its matchers, `lev.random`, `lev.env`, `lev.log`, `lev.request`,
`lev.response` - is built in `prelude.js` *on top of* those primitives.

Two reasons. First, it keeps the FFI surface small enough to audit: the whole
Rust<->JS boundary is eleven functions that fundamentally move strings. Second, it makes
the API grow in the cheap language: adding a matcher or a helper, or eventually a `pm.*`
compatibility shim, is a change to a JavaScript file, not to the bindings.

`lev.expect` matchers throw; `lev.test` catches the throw and records a failed check.
That is what makes `lev.expect(...).toBe(...)` inside a test read naturally without a
test framework.

### 3. A pooled runtime and context, owned by a worker thread

Creating a QuickJS runtime per iteration is the overhead the plan warns about, so a
`Sandbox` creates its `Runtime` and `Context` **once**, on a dedicated thread, and
reuses them for every invocation for its whole life. The public type is `Send + Sync`
(an `mpsc::Sender<Job>` plus a `JoinHandle`), so the engine can hold an
`Arc<Sandbox>` and hand it to a blocking worker.

`Sandbox::run` is synchronous and blocking; the async engine calls it through
`tokio::task::spawn_blocking`. That keeps the sandbox free of any async runtime
coupling (and works under both the current-thread and multi-thread runtimes that tests
use) and keeps the QuickJS call stack off the executor threads.

A failure is contained: the worker catches the error, replies, and loops. The next job
runs on the same clean runtime. This is directly tested - a `while (true)` timeout is
immediately followed by a normal script **on the same sandbox**.

### 4. The CPU budget is a per-job watchdog thread, not a clock read in the handler

The interrupt handler must be extremely cheap: QuickJS calls it on every loop
iteration, potentially millions of times per second. So it does the cheapest possible
thing - it loads one `AtomicBool`. A watchdog thread, spawned per job, flips that bool
if the job outlives its budget (`recv_timeout`).

The rejected alternative was for the handler to compare the clock itself (against a
process-start `Instant`). That is a `clock_gettime` on every branch, which is far more
work than a relaxed atomic load. A per-job thread spawn is tens of microseconds and
this is not yet the hot path; a persistent watchdog serviced over a channel is noted as
a Phase 3 optimisation if per-iteration cost ever matters.

A success always wins the race: if the script finishes just as the watchdog fires, the
result is still treated as success.

### 5. A script failure is a *configuration* error

Every `EngineError::Script` maps to `ErrorCode::Config`, and therefore to CLI exit code
2. A script is part of the request definition; a broken one - a syntax error, a thrown
type error, an infinite loop - is the author's mistake, not an engine fault. This also
keeps the wire contract simple: no new `ErrorCode` variant was needed.

The consequence for classification: QuickJS reports a *syntax* error as an ordinary
exception with no distinct marker (the message is e.g. `unexpected token in expression:
';'`), and `rquickjs` exposes no compile-only eval. Rather than pretend to a precision
the engine does not have, `ScriptError::Compile` vs `Runtime` is chosen by a small
marker heuristic and only affects the human-readable message - both map to `config`.
`Timeout` and `Memory` are detected reliably (from the interrupt flag and the
out-of-memory message respectively) and carry their own messages.

### 6. Assertions are data in `lev-core`; evaluation is in `lev-engine`

`Assertion` lives in `lev-core` (it is domain, not wire, not on-disk), but *evaluating*
one cannot: a `jsonPath` assertion has to parse JSON, and rule 1 says `lev-core` owns
no `serde`. So the evaluator lives in `lev-engine::checks`, which already depends on
`serde_json` for the protocol.

The `jsonPath` resolver is a deliberate subset - dot paths over objects and numeric
array indices (`id`, `user.name`, `items.0.sku`). No wildcards, filters, or slices.
That covers the overwhelming majority of what a check wants to express, and it can grow
later without a format break because a path is just a string in the document.

The envelope keys in YAML are untagged and human-first; the wire union is `kind`-tagged
like every other union in the protocol, so the on-disk and on-wire shapes stay
independent.

`ResponseTimeUnder` compares against the measured total time. `TimingsDto` carries the
`responseTimeUnderMs` budget on the disk side and the same assertion crosses the wire as
`{ "kind": "responseTimeUnder", "maxMs": 500 }`.

### 7. Scripts may see secrets - and cannot leak them

`lev.env` exposes the fully resolved variable scope, secrets included. This looks like
a violation of invariant 1 ("no secret value ever crosses the boundary") but is not:
the sandbox runs *inside* the engine, has no filesystem and no network, and every check
name, check message, and log line it produces passes through the engine's `Redactor`
before it is serialized into a DTO.

So a post-response script can legitimately do `lev.expect(lev.response.json.token)
.toBe(lev.env.get('api_token'))`, and it *still* cannot leak: a script that logs the
secret, or names a failed check after it, produces `***REDACTED***` on the wire. This is
verified by an engine test that logs and checks the secret and asserts the serialized
result contains neither the secret nor anything but the mask.

### 8. Protocol v0.2 by minor bump

`RequestDto` gains `scripts` (optional) and `assertions` (optional, empty omitted);
`RequestResult` gains `checks` and `logs` (both optional, empty omitted). All four are
backwards-compatible additions, so this is a minor bump to `0.2` under ADR 0002
decision 4 - an old client ignores the new fields, a new client against an old engine
sees an empty checks list.

### 9. One documented request-shape caveat

`lev.request.headers` is exposed to JavaScript as a plain object (`name -> value`),
because that is what is pleasant to use. A pre-request script that touches headers
therefore collapses a duplicated request header name to a single entry. Duplicate
request headers are rare, and this is documented rather than worked around (the
alternative, exposing a list, makes the common case ugly for a corner case). Response
headers are unaffected; a post-response script reads them from the response object.

## Consequences

- `lev-script` depends only on `lev-core` (plus `rquickjs`, `serde_json`, `thiserror`),
  as the plan's crate graph requires. It never sees a protocol DTO.
- The engine's dispatch is now: resolve -> **pre-request hook** (may rewrite) -> send ->
  **post-response hook** -> **evaluate assertions** -> redact checks and logs. The echo
  is computed *after* the pre-request hook, so it shows what actually went out.
- `Client::send_request` returns a `RequestOutcome` (echo, response, checks, logs)
  instead of a tuple; the CLI prints checks, prints script logs as `log:` lines, and
  exits 1 when any check failed. Exit code 1 now means "a check failed" (thresholds,
  which share the code, arrive in Phase 3).
- `lev-metrics` gains `checks`, `checks_failed`, and `script_errors` counters and
  matching k6-style metric names, so "checks/assertions -> metrics" is real as soon as
  the load path needs it.
- The runtime is pooled per `Sandbox`, so an engine that never uses scripts pays only
  one thread and one runtime at start-up.

## Deferred

- Per-iteration pooling under load: the current model is one pooled runtime per engine.
  Phase 3's load path will want a small pool across VUs; the watchdog-per-job spawn is
  the obvious first thing to replace with a persistent timer.
- A broader matcher library and a `pm.*` compatibility prelude (Phase 5 importers will
  want it).
- `jsonPath` wildcards/filters, and a first-class JSON-schema check.
- Duplicate request headers under a pre-request script (see decision 9).
- Surfacing script errors as a *metric in a running load test* rather than a per-request
  error (Phase 3).
