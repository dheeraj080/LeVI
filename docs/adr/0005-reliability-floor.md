# ADR 0005 - Reliability floor: deadlines, file bodies, graceful stop, retries

- **Status:** accepted
- **Date:** 2026-10-19
- **Phase:** 1 (reliability floor, first release train)

## Context

The plan's Phase 1 is the *reliability floor*: the set of behaviours that make Levi
trustworthy enough to run against somebody else's server and to trust its own numbers.
It is not a feature phase. Each item below is something the code already claimed, in a
doc or in its own type names, without doing it.

Four gaps were named and closed:

1. nothing bounded a request. A target that accepted a connection and then went quiet
   hung `levi request` forever, and hung a load run's virtual users with it;
2. a `file:` body was parsed, validated and then never read - the request went out with
   no body at all;
3. `levi serve` could only stop by being killed, which tears a session down mid-dispatch;
4. a transient `503` from a load balancer was a hard failure with no second chance, so a
   flaky hop looked like a broken service.

The rule for this phase was that a gap is closed by *implementing the behaviour*, never
by softening the claim. Where that was not possible, the claim was amended in the same
change - and the amendment is recorded here.

## Decisions

### 1. Deadlines are resolved once, at load, and travel with the request

A request has one deadline, decided before anything is sent:

```text
--timeout flag  >  request `timeout:`  >  scenario `timeout:`  >  transport default (30s)
```

The transport has three separate clocks, because they fail for different reasons: a
**connect** timeout (10s) bounds TCP/TLS establishment so a black-holed route fails fast
and says *connect* rather than *timeout*; a **request** timeout bounds the whole exchange
(dispatch through the last body byte); and a **pool idle** timeout (90s) closes
connections a target or a middlebox has already forgotten. The deadline covers the body
too - a target that dribbles bytes forever is the same failure as one that never answers.

The flag is applied by the CLI at load time, so every layer downstream sees one resolved
deadline instead of re-deriving one from a partially-populated config.

**Consequence:** `levi request --timeout 0` is rejected at parse (exit `2`), because
"unbounded" is the thing this phase exists to remove. A zero in a collection file is
*not* rejected yet - that validation belongs with the rest of the format checks, and is
noted as deferred below.

**Wire:** `sendRequest.request.timeoutMs` is optional and additive; a deadline that
expires is reported as `error { code: "engine" }`, since the target failed to answer and
no new error code says that better. `PROTOCOL_VERSION` went `0.2 -> 0.3` for this and the
body kind below; the field is absent on a v0.2 client, and such an engine applies its own
bounded default.

### 2. A `file:` body is read at load time, by the side that owns the collection

The file is opened, canonicalised and confined **when the collection is loaded** - not
per iteration, and not by the engine. Two reasons:

- the client is the side that knows the collection root and the side a user is watching,
  so a fixture path is resolved where it can be explained;
- a load run renders its body once and reuses the bytes, so opening a file per iteration
  would put a filesystem call in the hot loop for no gain.

The path is confined to the collection root: `..`, absolute paths and symlinks that
escape are refused, `{{` in a path is refused (a path is not a template), and the file is
capped at 32 MiB. Content type comes from the extension, and an explicit `Content-Type`
header still wins. The bytes travel as `Body::Bytes` / `BodyDto::Bytes { contentType?,
base64 }`, so an engine needs no filesystem access at all and can run where no collection
is checked out. A `file` body that still reaches an engine is refused: the engine cannot
tell a fixture from `/etc/passwd`.

`levi validate` resolves file bodies, so a missing or escaping fixture is a load error
(exit `2`) rather than a surprise during a run.

### 3. `levi serve` stops *between* messages

`Engine::serve_with_shutdown` checks the stop signal between dispatches, never during
one. An in-flight request is always answered, and the session then emits a final `log`
saying why it ended, so the client learns more than "the pipe closed". This is the
cheapest possible definition of graceful, and it is the one that matches the loop's
shape: the engine dispatches one message at a time, so there is no partial dispatch to
cancel.

`stop.graceful` is recorded in that final log even though `graceful: false` currently has
nothing extra to cut short - the distinction stays visible on the wire instead of being
quietly dropped.

### 4. Retrying a transient failure is a *client* policy, and never a load-run policy

Retries live in the transport (`ClientConfig`), where the deadline lives, and not in a
collection:

- a request file describes *what* to send; how hard to try again is a statement about
  the client's own time and about the load it is willing to put on the target;
- `levi run` therefore has **no retries at all** - not "off by default", but unpinned
  from the config it builds. A load report says "the target saw N requests", and a retry
  loop would fold two refusals and one success into a single sample with a single
  latency, hiding exactly the failures the run exists to measure.

Retries are off unless asked for, on a `Retry-After`-aware exponential backoff (200ms
doubling to 5s, `Retry-After` honoured up to 30s). Only transient failures count: `429`,
`502`, `503`, `504`, a refused connection, a reset, a deadline. `500` is deliberately
excluded - the target *processed* that request and failed, and repeating it may fail the
same way or, worse, succeed twice.

Only **idempotent** methods are retried by default (`GET`, `HEAD`, `OPTIONS`, `PUT`,
`DELETE`). Lifting that for `POST`/`PATCH` is possible via an explicit, named flag,
because a retried `POST` can create two orders: the danger is named at the call site
rather than assumed away.

Every response carries `attempts`, and the CLI prints it when it is more than one. A
retried success is 3 requests that the target saw, and a report that presents it as one
slow request is the kind of quiet lie this phase is about removing. Timings describe the
attempt that answered; the waits in between are not the server's latency.

**Consequence:** a retry is a second request to somebody else's service. This is why the
policy is set by whoever runs the command, is forced off in the load spine, and is
counted where a user can see it.

### 5. Connection phases are measured at the connector, and only when it was used
`dnsMs`, `connectMs` and `tlsMs` existed on the wire before they existed in the client,
and stayed omitted until a connector could measure them. A pooled client establishes a
connection once and reuses it, so the phases are a property of a *connection*: a request
served from the pool reports none of them, and reporting zero would be a measurement-
shaped lie. When a request does open a connection, it reports what that cost, split at
the one seam that can see each phase — the resolver for DNS, the plain connector for
TCP, the TLS connector for the handshake, with the handshake measured as the span the
plain connector did not already claim.

The carrier is a task-local rather than a field, because attribution is the whole
problem: a connector may be polled on a task other than the request that made it, and a
task-local can only ever be read by the task that installed it. Two requests that share
one connection therefore never both claim its phases, and a request that merely waited
for somebody else's connection claims nothing at all.

### 6. Logs go to stderr, because stdout belongs to something else

`levi serve`'s stdout is a machine channel (NDJSON), and every other command prints its
result there, so the log subscriber is built with an explicit stderr writer instead of a
default: a stray line on stdout does not merely degrade the log, it corrupts a protocol
somebody else is parsing. The level is a `tracing` filter — `LEVI_LOG`, falling back to
`RUST_LOG` — and the default is `warn`, so a working tool is silent. What breaks that
silence is worth a line: a retry, an `abortOn` firing, a session starting and ending.

Two kinds of line are deliberately *not* emitted. The load path logs nothing per request:
ten thousand a second is noise nobody can read, and the run's own counters are the record.
And a transport failure is logged as a fixed classification rather than as the error's
own message, because a message can carry the URL and a URL can carry a token — a log line
must never be a way around the redaction the engine does.

### 7. Secrets come from the environment, then the keychain, and redaction follows them

The spec says a `secret:` reference is resolved from the OS keychain and never stored;
until now only `LEVI_SECRET_*` existed, which made the spec wrong. Both exist now, in a
fixed order: the environment first, the keychain second. That order keeps every existing
`LEVI_SECRET_*` working, keeps CI (which has no keychain) on the fast path, and means a
keychain failure can only break a command that actually references a keychain-only
secret — never every command.

The part that took design was redaction. The redactor is built from values known up
front, and a keychain has no "list" to build anything from — so the store reports each
value the first time it resolves it, and the engine folds it into the redactor *before*
the request is echoed. Values are resolved once per process and cached after that: a
load run must not become a keychain round-trip per request, and "resolved once per run"
is also what ADR 0001 promised.

What is deliberately absent: `levi secret list` (credential stores cannot enumerate
reliably, and one that could would leak which names exist) and `levi secret get`
(printing a secret is what a keychain exists to prevent). The value for
`levi secret set` comes from stdin, never from an argument — an argument lands in the
shell history and in `ps` output.

## Deferred, on the record

- **`serve`-wide defaults** (`levi serve` has no `--timeout`/`--retries` of its own): a
  serve-wide default belongs in `levi.toml` with the rest of the project config, not in
  a second flag with different precedence rules.
- **Zero deadlines in a collection file** are not rejected; only the flag is.
- **Coordinated-omission correction**, **open/ramping executors**, **scripts and
  assertions under load** - Phase 3 work, see ADR 0004.
