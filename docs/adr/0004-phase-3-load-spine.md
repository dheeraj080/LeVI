# ADR 0004 - Phase 3, slice 1: the load spine

- **Status:** accepted
- **Date:** 2026-10-14
- **Phase:** 3 (first slice)

## Context

Phase 3 is "the wedge": the point at which Levi stops being a request runner and starts
being a load generator. The plan's Phase 3 bullet list is wide — closed **and** open
executors, staged ramps, thresholds with `abortOn`, graceful stop, JUnit/HTML/CSV/
Prometheus reporters, `levi selftest`, and a 30-minute run at 10k RPS with flat memory.

Building all of that before anything is measurable is how a load tool ends up with a
beautiful architecture and numbers nobody trusts. So this slice builds the **smallest
end-to-end vertical**: a scenario file, a closed executor, a real metrics pipeline, and
`levi run` producing a report — proving the spine before widening it.

The exit criterion for this slice is deliberately modest and testable: *a paused-clock
run produces the exact iteration count in zero wall time, and an injected-latency
target reports a p50 near the latency it was told to add.*

## Decisions

### 1. A vertical slice, not the whole phase

In scope: the `Scenario` format and domain type, the closed `ConstantVus` executor,
per-worker HDR histograms, a 4 Hz aggregator, a `RunReport`, and `levi run` with a JSON
reporter.

Explicitly deferred, and named here so the gaps are on the record rather than
discovered: open/ramping executors, coordinated-omission correction, thresholds and
`abortOn`, graceful stop beyond the planned duration, scripts and assertions under load,
run persistence, instrumented DNS/connect/TLS timings (landed afterwards, ADR 0005 §5),
and `levi selftest`.

The consequence is honest and user-visible: a completed run exits `0` even if requests
failed, because "failed" only means something once thresholds exist to define it. The
usage text says so.

### 2. Compile once, render per iteration

`prepare_request` used to re-parse `Body::Json` / `Body::Raw` templates and clone the
whole `VarSet` on **every** call. Fine for one interactive send; real overhead at 10k
RPS.

Rather than add a second, faster path, `lev-core` now exposes `compile_request`, which
parses every template once and returns a `CompiledRequest`, and
`CompiledRequest::prepare(&mut Context)`, which renders it. `prepare_request` — the
interactive path — is reimplemented as compile-then-render, so there is exactly one
resolver and the two paths cannot drift. Existing `prepare` tests were kept unchanged
and still pass, which is the proof the refactor was behavior-preserving.

### 3. One long-lived context per VU

The other latent bug: `Context::with_seed` builds a **fresh** `DynamicFunctions`, so a
reused seed makes `{{$uuid}}`, `{{$randomInt}}` and `{{$timestamp}}` emit the *same*
value every iteration. A load test where every request carries the same "unique" id is a
load test that is silently wrong.

A virtual user therefore creates one `Context` and renders every iteration through it,
so the seeded RNG keeps advancing. Each VU derives its own seed (`seed + vu_index`), so
VUs differ from each other but a whole run is still reproducible from `--seed`.

### 4. A VU is a task; a worker is a thread

Per the plan's concurrency model, `workers` threads are spawned, each with its own
current-thread runtime, and `vus` async tasks are dealt out across them. A VU never
migrates between cores mid-request, which is what keeps the timing-critical path honest,
and thousands of VUs still fit because they are tasks, not threads.

Each worker owns one `WorkerMetrics`: relaxed atomics for counters and a `Mutex` around
a live HDR histogram. The aggregator, running on the coordinating runtime, **swaps** each
worker's histogram for a fresh one every 250 ms and merges the drained delta into the
run histogram. Swapping means the aggregator never holds a worker's lock for longer than
the swap.

### 5. An injectable `Clock`, and a `Sender` seam

The plan asks for scheduler tests that run in microseconds. Two seams make that possible
without a network or a real timer:

- `Clock` — production `TokioClock`; under `tokio::time::pause()` it is virtual, so a
  nominal second of scenario time costs no wall time.
- `Sender` — the executor sends through `Arc<dyn Sender>`, not `HttpClient` directly. A
  test hands it a sender that returns instantly; production uses `HttpSender`, which
  discards response bodies (`HttpClient::send_discarding`).

The paused-clock test then proves the whole loop: N VUs × a known number of iterations,
in no wall time. Getting there surfaced a real bug — with a paused clock, a think-time
sleep that overran the deadline left the outer loop spinning forever — which the clamp
to the deadline now prevents. That bug would have been a mystery hang under a real clock;
the injected clock made it a five-line fix.

### 6. Latency is measured around the call, by the loop

`http_req_duration` is measured by the VU loop, around `Sender::send`, rather than read
from the transport's `Timings`. It times successes and transport failures identically and
respects the injected clock. `send_discarding` still returns per-phase `Timings` (the
foundation for `http_req_waiting`), but the loop does not depend on them.

### 7. `lev-metrics` stays plain data

The metrics crate gains `RunOutcome`, `RunReport`, and batch/latency methods on
`MetricsRecorder`, but stays free of `hdrhistogram` and `serde`. Percentiles are computed
in `lev-load` from the merged histogram and handed in as a `LatencySummary`; the JSON
reporter (also in `lev-load`) owns serialization. Same split as the engine and protocol:
the data type is shared, the encoding is not.

### 8. `http_req_failed` means "did not succeed"

The field previously promised "transport-level" failures only. It now counts a transport
failure *or* a 4xx/5xx status — k6's rule — because a run where every request 500s and
none are "failed" is worse than useless. The doc comment was updated to match.

### 9. No protocol change

`levi run` reports in-process. The load messages (`loadRunRequest` / `snapshot` /
`runFinished`) stay reserved, unversioned, and unused; `PROTOCOL.md` is untouched. A UI
that streams live snapshots is a later slice, and it will get a proper protocol minor
bump then rather than a speculative one now.

## Consequences

- `lev-core` exposes `compile_request`, `CompiledRequest`, and the `Scenario` / `Executor`
  / `Step` domain types; `prepare_request` keeps its signature and behavior.
- `lev-format` gains `ScenarioDto`, `load_scenario`, and `parse_duration` /
  `format_duration` (units `us`/`ms`/`s`/`m`/`h`, or a bare number of seconds). The
  conformance corpus gains `load/checkout.yaml`.
- `lev-http` gains `send_discarding` and `LoadResponse`; the interactive `send` is
  unchanged apart from sharing one request-building helper.
- A new crate, `lev-load`, owns the executor, the metrics pipeline, the `Clock` and
  `Sender` seams, and the reporters. It depends on `lev-core`, `lev-http`, `lev-metrics`,
  `hdrhistogram`, `serde_json`, `tokio`, and `thiserror` — never on the protocol.
- `levi run <scenario>` exists, with `--workers`, `--seed`, `--env`, `--out`, `--quiet`,
  and the documented exit codes.
- `hdrhistogram` is added with `default-features = false`: the default set pulls
  `flate2`/`nom`/`base64` for serialization the load path never uses. Its only new
  transitive dependency is `byteorder` (Unlicense OR MIT), which the license allow-list
  already covers.

## Deferred

- Open and ramping executors, and staged timelines; coordinated-omission correction.
- Thresholds, `abortOn`, and exit code 1 for a failed threshold.
- Graceful stop and `CancellationToken`; a partial report flagged as `Aborted`.
- CSV / JUnit / HTML / Prometheus / OTLP reporters, and a per-snapshot timeseries
  sidecar (the JSON reporter writes the final report only).
- Scripts and assertions under load, and surfacing `script_errors` as a live metric.
- Per-worker connection pools (slice 1 shares one `HttpClient` across workers).
- Run persistence under `~/.levi/runs/<id>/`.
- `levi selftest`, and the 30-minute 10k-RPS flat-memory run that closes the phase.
