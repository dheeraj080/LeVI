# Levi

**The request you already wrote is the load test.**

Levi is a local-first, git-native API client with load testing built in. There is no
separate load-testing tool and no rewriting requests in k6 syntax: the same request,
the same scripts, the same environment — fired once for interactive debugging, or tens
of thousands of times per second for a load run.

> **Status: Phase 1 (protocol + interactive engine).** The engine is built as a
> standalone Rust CLI first, per the build plan. `levi request` works end to end *through
> the protocol boundary*, with real HTTP, per-phase timing, and secrets redacted inside
> the engine; `levi serve --stdio` speaks the same protocol over NDJSON. Protocol v0 is
> frozen — see [`PROTOCOL.md`](PROTOCOL.md). `levi run` and load testing land in Phase 3.

## Layout

```
crates/
  lev-core/      pure domain model — no I/O, no serde
  lev-format/    collection format: on-disk DTOs + (de)serialization
  lev-http/      the single hyper-based HTTP transport
  lev-protocol/  the wire protocol: versioned DTOs + transports (cannot see lev-core)
  lev-engine/    the facade: DTO <-> core conversions, redaction, the serve loop
  lev-metrics/   metrics scaffold: MetricSnapshot + k6-compatible metric names
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

# Set the secret the smoke collection references
export LEVI_SECRET_smoke_token=supersecret-token      # Windows: $env:LEVI_SECRET_smoke_token = '...'

cargo run -p lev-cli -- request examples/smoke/requests/get-root.yaml --show-body

# Parse a collection directory and report every problem
cargo run -p lev-cli -- validate examples/smoke

# Serve the protocol over stdio (one JSON message per line; see PROTOCOL.md)
cargo run -p lev-cli -- serve --stdio
```

`levi request` sends a real `sendRequest` to an engine over the in-process transport, so
the CLI exercises the same boundary the UI will. Output shows the resolved request
(secrets masked), the status, a per-phase timing breakdown, and the redacted response —
redaction happens in the engine, before anything crosses the boundary.

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
| 1 | Threshold failed |
| 2 | Configuration / validation error |
| 3 | Engine error |
| 4 | Aborted (user or `abortOn`) |

## License

Dual-licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at
your option. The collection format spec in `spec/` is licensed permissively so it can
become a standard. Contributions are accepted under the same terms.
