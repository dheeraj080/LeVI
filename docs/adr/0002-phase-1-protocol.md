# ADR 0002 — Phase 1: the protocol boundary

- **Status:** accepted
- **Date:** 2026-10-13
- **Phase:** 1

## Context

Phase 1 builds the interface that is most expensive to change and that the UI will be
written against: `lev-protocol`. The plan commits to "one boundary, three transports,
transport-varying hosting" (§3, §4.1, §4.9) and to freezing protocol v0 here.

Five questions had to be answered before any of it could be written.

## Decisions

### 1. `lev-protocol` is a pure DTO crate — it cannot see `lev-core`

The plan says `lev-protocol` defines its own DTOs and "never leaks core types", while
also saying both `lev-format` and `lev-protocol` "convert to and from the core types"
(§4.1 rule 3).

We resolved the tension in favour of the stronger guarantee: the conversion functions
live in **`lev-engine`**, and `lev-protocol` depends only on `serde`, `serde_json`,
`tokio` and `thiserror` — never on `lev-core`. A domain type therefore *cannot* reach
the wire, because the crate that owns the wire types cannot name one. A reviewer is no
longer the mechanism; the compiler is. The cost is one extra `mod convert` in
`lev-engine`, which is where the protocol and the domain are supposed to meet anyway.

### 2. Redaction happens in the engine, before anything crosses the boundary

In Phase 0 the CLI held the resolved secrets and redacted its own output. That does not
survive a spawned daemon: the client must never receive a secret, and a target may echo
a token straight back in a response body.

So redaction moved *into* the engine. Every path that could carry a secret — the request
echo, response headers, and the response body — is scrubbed before it is serialized into
a DTO. Consequences:

- `requestResult` carries a **redacted** echo of the request as actually sent, which is
  also exactly what a UI wants to render.
- `ResponseDto` is redacted by construction; there is no unredacted variant.
- A client never needs the secrets at all, so an in-process client and a remote agent
  have identical capabilities. Verified by test: a target that reflects the credential
  produces `***REDACTED***` on the wire.

### 3. v0 freezes the interactive path; the load messages are reserved

The plan lists load messages (`loadRunRequest`, `snapshot`, `runFinished`, …) in the
protocol. We deliberately did **not** freeze them in v0. Freezing a message shape that
has never been exercised is how protocols rot, and `lev-load` does not exist until
Phase 3. They are documented as reserved in `PROTOCOL.md` and will arrive in a minor
bump — which is non-breaking by the rules in decision 4.

What *is* frozen: the envelope (`type`-tagged, camelCase JSON), the handshake, version
negotiation, `sendRequest` / `requestResult`, `log`, `error`, `stop`, and the error-code
vocabulary.

### 4. Versioning is strict on majors and lenient on minors

Majors must match exactly; within a major both sides speak the lower minor. A mismatch
is answered with exactly one `error` (`protocolVersionMismatch`) followed by a clean
close — never a hang, never a panic. This is the mechanism that lets the UI and an older
engine coexist, and it is tested.

### 5. A non-2xx status is a successful request

A 500 from the target is an *observation*, not a failure: it travels in `ResponseDto`
with `status: 500`. Only a request that could not be sent at all is an `error`. The
Phase 0 CLI behaved this way, and Phase 3's `http_req_failed` depends on the
distinction. `sendRequest` never fails on status.

## Consequences

- The engine is the single place that knows both representations; `lev-engine` owns
  `convert`.
- `levi request` now goes *through* the protocol (in-process transport) rather than
  calling `lev-http` directly. There is no shortcut path to drift out of sync, so most
  CLI regressions are protocol regressions.
- `lev-metrics` lands as a dependency-free **scaffold**: metric names (k6-compatible),
  the run lifecycle, and `MetricSnapshot`. It has no consumer until Phase 3 and does
  not yet depend on `lev-core`, because a snapshot contains no domain types.
- `levi validate` was added to the CLI skeleton: it parses a collection directory
  (`collection.yaml`, `environments/*.yaml`, `requests/*.yaml`) and reports every
  failure with its path, exit code 2. Cheap, and it is the first thing a CI job wants.
- `hyper-rustls`'s split read/write and `tokio::io::split` proved easy to misuse in
  tests; both NDJSON framing tests originally deadlocked by writing to one half of a
  duplex and reading the other. The transport is fine; the lesson is that a hung test
  must be caught by a bounded timeout, not by waiting.

## Deferred

- OS-keychain `SecretStore` (secrets still come from `LEVI_SECRET_<NAME>`).
  **Done later:** the keychain answers names the environment does not have, with the
  values folded into the redactor as they resolve — see ADR 0005 §7.
- Instrumented connector timings (`dnsMs` / `connectMs` / `tlsMs` are still absent).
  **Done later:** they are measured at the connector now, and reported only by a
  request that opened a connection — see ADR 0005 §5.
- The `env` DTO carries the **merged** `collection < environment` scope, with the merge
  done by the client. Precedence merging is trivial config assembly, but if it grows
  (runtime/iteration scopes, §4.8) it should move server-side.
- Load messages, snapshots, and threshold evaluation (Phase 3).
