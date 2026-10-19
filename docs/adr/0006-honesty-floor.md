# ADR 0006 - Honesty floor: say only what the code does, and do what the spec says

- **Status:** accepted
- **Date:** 2026-10-19
- **Phase:** 2 (spec ↔ code reconciliation, first release train)

## Context

After the reliability floor ([ADR 0005](0005-reliability-floor.md)) the next risk was not a
missing feature but a **claim that had outgrown its code**. `PROTOCOL.md`, `spec/README.md`
and `README.md` are the product: a user writes a collection against the spec, and a client
is written against the protocol. Every sentence in them was re-read against the
implementation, claim by claim.

Most held. Five did not, and the rule was the same as in phase 1: **fix the side that is
wrong**. Where the claim described behaviour worth having, the behaviour was implemented;
where the claim described something the design never built, the prose was corrected, and
the comments that repeated it went with it.

The five gaps:

1. the threshold metric table listed `checks`, `checks_failed` and `script_errors` as
   usable, but a load run records none of them — the rule would have compared `0`;
2. the body format said a body takes "exactly one key", and the untagged enum silently
   took the first key that matched and dropped the rest;
3. `levi validate` claimed to "report errors" for a collection directory, and never
   opened `load/`;
4. the protocol promised that a bad message is *answered*, and the engine dropped the
   session silently on a line it could not parse;
5. the documented variable precedence had five layers; two of them do not exist.

## Decisions

### 1. A threshold over a metric no run measures is a parse error

`Threshold::parse` now refuses `checks`, `checks_failed` and `script_errors` with a message
that says *why*: a load run evaluates neither scripts nor assertions yet, so the counter is
always `0` and `checks_failed count==0` would pass on every run while measuring nothing. A
green threshold that verified nothing is worse than no threshold at all.

The `ThresholdMetric` variants stay. The names are still recognised, so the error can name
the metric it refused, and the vocabulary is frozen before the load engine fills the
counters in — that is an M3 change, not an M2 one. `ThresholdMetric::is_measured()` is the
single place that decides, so lifting the refusal is one line and one test.

The refusal lives at the **parse boundary**, which is where a scenario is read
(`lev-format` calls `Threshold::parse` once per expression). `lev-load` still maps all
seven metrics, so the type stays total and a directly constructed `Threshold` — the fields
are public — still evaluates deterministically. No scenario file can reach that path.

### 2. A body says exactly one thing, and deserialization enforces it

`BodyDto` derived `#[serde(untagged)]`. An untagged enum takes the first variant that
matches, so

```yaml
body:
  json: '{"sku": "{{$uuid}}"}'
  file: fixtures/logo.png
```

parsed as JSON and dropped the file reference without a word. The fix is a hand-written
`Deserialize` backed by a private, all-optional `BodyFields` struct with
`deny_unknown_fields`: it can see every key at once, so it counts them.

- exactly one key → that variant;
- none, or more than one → an error naming what it found (`found \`json\` and \`file\``),
  not a silent pick;
- a misspelled key → `unknown field`, not "nothing matched".

`Serialize` is still derived (with `untagged`), and the existing `convert.rs` pattern
matches are untouched: only the *reader* got stricter, which is where a footgun lives.

### 3. `levi validate` reads the load scenarios too

A scenario is a collection document, so `validate` parses every `load/*.yaml` with the same
`load_scenario` the run path uses — which already covers its syntax, its `thresholds` and
its `abortOn` rules, and therefore the new refusal from decision 1.

Once *every* file parses on its own, `validate` also compiles each scenario against the
collection and the request set, so a step that names a request nobody wrote is reported
here. `run` would say the same thing, but a run has already started by then, and catching
it from `validate` in CI is the whole point of the command. Cross-file checks stay behind
the "everything parsed" gate on purpose: reporting "unknown request" on top of three
syntax errors is noise, not help.

### 4. A line that is not a protocol message is answered, not dropped

`PROTOCOL.md` §4 said a message that arrives the wrong way "produces `error` and **the
session continues**", and invariant 4 said a protocol mistake is *survivable*. In fact a
line that did not parse surfaced as a transport error, propagated out of the loop, and
closed the stream with nothing on the wire — the client saw an EOF and had to guess.

The engine now classifies `TransportError::Decode` and answers `badMessage`:

- **after the handshake**, the session continues. The framing is one message per line, so
  a garbled line costs the line and nothing more, and the client learns what it did wrong
  instead of watching a pipe close;
- **before the handshake**, `badMessage` and then a clean close, because there is no
  session to keep alive yet and the first message must be `hello`. The second message is
  never lost either way: it is a whole line.

The doc in §4, the error table and invariant 4 now spell out both halves, because the
difference is the part a client author has to know.

### 5. The variable precedence chain is the one that exists

The docs listed `global < collection < environment < runtime < iteration`. Only two layers
are read from disk — the collection's `vars`, then the environment overlay that wins over
them — and a script may add values at run time with `lev.env.set`. There is no process-wide
layer and no per-iteration layer. The prose was corrected in `spec/README.md`, and the same
wrong chain was removed from the comments that had repeated it in `lev-core`,
`lev-protocol`, `lev-load` and `lev-cli`.

### 6. One spelling per error code

`ErrorCode::as_str` returned PascalCase (`BadMessage`) while the wire form is camelCase
(`badMessage`), and `Display` renders `as_str`, so a user reading a remote error saw a
token the docs never told them to match. `as_str` returns the wire spelling now, and a test
pins `to_string()` to serde's own output, so the two cannot drift.

## Prose corrected alongside the code

- `PROTOCOL.md`: the `hello` example showed `minor: 2`; the capability paragraph implied an
  engine that advertises something (the client declares what it can speak, this engine
  implements one mode and advertises nothing); the body-kind list was missing v0.3's
  `bytes`; `checks` order (script outcomes first, then declarative assertions); the retried
  transient set gained "a response body that stopped part-way"; §8's `PROTOCOL_VERSION is
  0.2` was turned into the past tense it is.
- `README.md`: "Protocol v0.2 is frozen" → v0.3; "Phase 3 (load testing, first slice)" →
  Phase 3, with what has landed and what has not; "per-worker HDR percentiles" → percentiles
  merged from per-worker histograms; the layout tree gained `docs/adr/` and `scripts/`; the
  `spec/` license sentence now says what the license actually says.
- `spec/README.md`: `load/<name>.yaml` is a convention passed to `run`, not a discovered
  directory; the `file` path rule describes the canonicalise-and-confine check that exists
  (`..` that climbs out, including through a symlink) instead of "no `..`"; the `lev.*`
  table lists `lev.expect`'s matchers, since it claims to be the whole surface.

## Verified true, left alone

The audit did not only find gaps. These claims were checked against the code and hold:
the `file` body is refused at prepare time with a message pointing at the loader; the
`attempts` / `timings` semantics; §2's versioning rules; the message and DTO field shapes;
the `responseTimeUnderMs` on-disk key against the `responseTimeUnder` wire shape; the
redaction claim; the 4 Hz aggregation.

## Deferred, on the record

- **Engine-side capability advertisement** needs an optional capability to exist before it
  means anything; `Hello.capabilities` is a client statement of intent until then.
- **`checks`, `checks_failed` and `script_errors` under load** — M3, which is what lifts
  decision 1.
- **Zero deadlines in a collection file** are still not rejected — carried from ADR 0005.
