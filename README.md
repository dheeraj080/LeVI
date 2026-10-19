# Levi

**The request you already wrote is the load test.**

Levi is a local-first, git-native API client with load testing built in. There is no
separate load-testing tool and no rewriting requests in k6 syntax: the same request,
the same scripts, the same environment — fired once for interactive debugging, or tens
of thousands of times per second for a load run.

> **Status: Phase 3 (load testing, first slice).** The engine is built as a standalone
> Rust CLI first, per the build plan. `levi request` works end to end *through the
> protocol boundary*, with real HTTP, per-phase timing, and secrets redacted inside the
> engine; `levi serve --stdio` speaks the same protocol over NDJSON. Protocol v0.2 is
> frozen - see [`PROTOCOL.md`](PROTOCOL.md). Requests may carry JavaScript pre/post hooks
> and declarative assertions, run in a sandboxed QuickJS runtime with a CPU budget and a
> memory limit; a runaway script fails one request and never hangs the run. `levi run`
> now drives a closed (constant-VU) load scenario, reports per-worker HDR percentiles,
> and judges the run against a scenario's `thresholds` — a failed one exits 1, `abortOn`
> stops the run as soon as it fails, and Ctrl-C ends it gracefully with a partial report.
> Still to come in Phase 3: open/ramping executors, coordinated-omission correction,
> more reporters, and `levi selftest`.

## Layout

```
crates/
  lev-core/      pure domain model - no I/O, no serde
  lev-script/    the QuickJS sandbox: the lev.* API, limits, pooled contexts
  lev-format/    collection format: on-disk DTOs + (de)serialization
  lev-http/      the single hyper-based HTTP transport
  lev-protocol/  the wire protocol: versioned DTOs + transports (cannot see lev-core)
  lev-engine/    the facade: DTO <-> core conversions, redaction, hooks, the serve loop
  lev-load/      the load engine: executors, per-worker HDR metrics, reporters
  lev-metrics/   the metric shapes: MetricSnapshot, RunReport, k6-compatible names
  lev-cli/       the `levi` binary
examples/smoke/ a runnable collection
spec/            the public collection-format spec + conformance corpus
```

Four rules keep the crates honest:

1. `lev-core` has **no I/O** and **no `serde`** — it is never serialized.
2. `lev-format` owns the on-disk representation behind a `DocumentFormat` boundary.
3. `lev-protocol` owns the wire representation and **cannot see `lev-core`**, so a
   domain type cannot leak onto the wire.
4. One HTTP path (`hyper`, never `reqwest`) so interactive and load behaviour cannot
   diverge.

See `spec/README.md` for the on-disk format and [`PROTOCOL.md`](PROTOCOL.md) for the wire
protocol.

## Quickstart

```sh
# Start a local target (a JSON echo server)
pwsh scripts/dev-server.ps1 -Port 8787

# Set the secret the smoke collection references — or store it in the OS keychain
# once with `levi secret set smoke_token` and leave the environment alone
export LEVI_SECRET_smoke_token=supersecret-token      # Windows: $env:LEVI_SECRET_smoke_token = '...'

cargo run -p lev-cli -- request examples/smoke/requests/get-root.yaml --show-body

# Run a load scenario against the same target
cargo run -p lev-cli -- run examples/smoke/load/smoke.yaml --out report.json

# Parse a collection directory and report every problem
cargo run -p lev-cli -- validate examples/smoke

# Serve the protocol over stdio (one JSON message per line; see PROTOCOL.md)
cargo run -p lev-cli -- serve --stdio
```

`levi request` sends a real `sendRequest` to an engine over the in-process transport, so
the CLI exercises the same boundary the UI will. Output shows the resolved request
(secrets masked), the status, a per-phase timing breakdown, the redacted response, any
script `log:` lines, and the checks - redaction happens in the engine, before anything
crosses the boundary. The command exits **1** when a check failed, which is what a CI job
gates on.

Requests are bounded: `--timeout` (or a request's own `timeout:`) cuts a send off at a
deadline, so a target that stops answering costs one iteration instead of hanging the
tool, and `--retries` spends a second request on a target that refused the first - off
unless asked for, on idempotent methods only, and never on `levi run`, whose report has to
count every request the target saw. The reasoning is in
[`docs/adr/0005-reliability-floor.md`](docs/adr/0005-reliability-floor.md).

Operational logs - a retry, an `abortOn` firing, a session starting and ending - go to
**stderr**, never to stdout, which belongs to the result and to `serve`'s NDJSON channel.
They are quiet by default (`warn`); `LEVI_LOG=info`, `LEVI_LOG=debug` or
`LEVI_LOG=lev_http=debug` turn them on.

`levi run` compiles a scenario against the collection, then drives it with the load
engine: worker threads, each running a pool of virtual users, feeding per-worker HDR
histograms that the aggregator merges four times a second. It prints requests, failures,
throughput, and latency percentiles, and writes the same numbers as JSON with `--out`.
The scenario format is in [`spec/README.md`](spec/README.md); the design is in
[`docs/adr/0004-phase-3-load-spine.md`](docs/adr/0004-phase-3-load-spine.md).

## Development

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace   # or: cargo test --workspace
```

CI runs fmt, clippy (`-D warnings`), tests, and `cargo-deny` (license/advisory policy).

### Exit codes (a contract for CI)

| Code | Meaning |
|---|---|
| 0 | Success |
| 1 | A check or a threshold failed |
| 2 | Configuration / validation error |
| 3 | Engine error |
| 4 | Aborted (Ctrl-C, or `abortOn` with no failed threshold to report) |

A failed threshold takes precedence over an abort: `abortOn` fires *because* a threshold
failed, so that run exits 1 — the signal CI gates on — with `outcome: aborted` and
`stoppedBy` recorded in the report.

## License

Dual-licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at
your option. The collection format spec in `spec/` is licensed permissively so it can
become a standard. Contributions are accepted under the same terms.
