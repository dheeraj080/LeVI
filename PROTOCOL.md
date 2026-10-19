# Levi wire protocol - v0

**Status: frozen v0** (Phase 1; **v0.2** adds scripting in Phase 2; **v0.3** adds
per-request deadlines). Until v1 the rules below may only be changed in
backwards-compatible ways. The crate that implements this is `lev-protocol`.

> **v0.2 (minor):** `sendRequest.request` gains optional `scripts` and `assertions`;
> `requestResult` gains optional `checks` and `logs`. All four are additive, so a v0.1
> client ignores them and a v0.2 client against a v0.1 engine sees an empty check list.
> See §8.
>
> **v0.3 (minor):** `sendRequest.request` gains the optional `timeoutMs` deadline in
> milliseconds. Absent means "use the engine's default request timeout" — never
> "unbounded". Additive; see §9.

## 1. Why one protocol

Every client path — the desktop UI, the CLI, and (later) a remote agent — speaks these
messages. Only the *transport* varies:

| Path | Transport | Why |
|---|---|---|
| Single interactive request, imports | **in-process typed channel** | Same boundary, no encoding, instant feedback |
| Load runs | **NDJSON over stdio**, a spawned child | A high-rate run must not OOM or crash the editor |
| Desktop sidecar, remote agents | socket / named pipe / WebSocket | Phase 6 |

The protocol crate deliberately **cannot see `lev-core`**. It depends only on
`serde` and `tokio`, so a domain type cannot leak onto the wire even by accident —
the compiler enforces the rule. Conversions live in `lev-engine`.

## 2. Versioning

```
Version { major: u16, minor: u16 }        // e.g. 0.1
```

- `hello` carries `protocolVersion`; `ready` returns the negotiated one.
- **Majors must match exactly.** Within a major, both sides speak the *lower* minor.
- Adding a message, a variant, or an optional field is a **minor** bump.
- Removing a message or changing its meaning is a **major** bump.
- An incompatible major is answered with exactly one `error`
  (`code: "protocolVersionMismatch"`), after which the engine closes the session. It
  never hangs and never panics.

## 3. Encoding

Every message is one JSON object, internally tagged by `type`, with `camelCase`
field names.

**NDJSON over stdio** (`levi serve --stdio`): one object per line, UTF-8, `\n`
terminated. Blank lines are ignored. Standard output carries protocol messages and
**nothing else** — all human-facing output goes to standard error. This is what keeps
a spawned run machine-readable.

## 4. Messages (v0)

| `type` | Direction | Purpose |
|---|---|---|
| `hello` | client → engine | Mandatory first message |
| `sendRequest` | client → engine | Send one request, await one `requestResult` |
| `stop` | client → engine | Wind down |
| `ready` | engine → client | Handshake succeeded |
| `requestResult` | engine → client | The redacted request echo + response |
| `log` | engine → client | A redacted log line |
| `error` | engine → client | A failure; `code` is the contract |

Direction is enforced by the receiver. A message that arrives the wrong way
produces `error` (`code: "unsupported"`) and **the session continues**.

### `hello`

```json
{ "type": "hello", "protocolVersion": { "major": 0, "minor": 2 },
  "capabilities": ["interactive"] }
```

Capabilities: `interactive`, `load`, `graphql`, `grpc`. A client must not rely on a
capability the engine did not advertise.

### `ready`

```json
{ "type": "ready", "version": { "major": 0, "minor": 3 }, "engine": "0.0.0" }
```

### `sendRequest`

```json
{
  "type": "sendRequest",
  "request": {
    "id": "get-root",
    "name": "GET /",
    "method": "GET",
    "url": "/things/{{thing_id}}",
    "headers": [{ "name": "Accept", "value": "application/json" }],
    "body": { "kind": "json", "content": "{\"id\":\"{{thing_id}}\"}" },
    "auth": { "kind": "bearer", "token": "{{api_token}}" },
    "scripts": {
      "preRequest": "lev.request.headers['X-Trace'] = lev.random.uuid();",
      "postResponse": "lev.test('ok', lev.response.status === 201);"
    },
    "assertions": [
      { "kind": "status", "code": 201 },
      { "kind": "jsonPath", "path": "id", "equals": "{{thing_id}}" }
    ],
    "timeoutMs": 5000
  },
  "env": {
    "vars": { "thing_id": "42", "api_token": { "secret": "smoke_token" } },
    "baseUrl": "https://api.example.com",
    "seed": 7
  }
}
```

`request.url`, headers, body and auth still contain **templates**; the engine resolves
them. `env.vars` is the already-merged `collection < environment` scope; `baseUrl` and
`auth` are the collection defaults the engine applies when the request has none
(`baseUrl` resolves a relative request URL; request-level auth wins).

Body kinds: `json { content }`, `form { fields: [{ name, value }] }`,
`raw { contentType?, content }`, `file { path }`.

Auth kinds: `bearer { token }`, `basic { username, password }`,
`apiKey { name, value, location }` where `location` is `header` (default) or `query`.

A variable value is a literal string (`"42"`) or a secret reference
(`{ "secret": "smoke_token" }`).

`scripts` is optional: `preRequest` runs after resolution and may rewrite the request;
`postResponse` runs after the response arrives. Both are JavaScript source text, run in
a sandbox with a CPU budget and a memory limit (a runaway script fails *that* request
with `code: "config"` and never hangs the session). `assertions` is optional and
evaluated after the post-response script; each is a `kind`-tagged union:

| `kind` | Payload |
|---|---|
| `status` | `{ code }` |
| `header` | `{ name, contains? }` |
| `bodyContains` | `{ text }` |
| `jsonPath` | `{ path, equals? }` |
| `responseTimeUnder` | `{ maxMs }` |

Scripts execute code, so they only travel over the in-process channel or over stdio to a
child the user spawned themselves — never to a remote agent.

### `requestResult`

```json
{
  "type": "requestResult",
  "request": {
    "method": "GET",
    "url": "https://api.example.com/things/42",
    "headers": [
      { "name": "Accept", "value": "application/json" },
      { "name": "authorization", "value": "***REDACTED***" }
    ],
    "bodyLen": 0
  },
  "response": {
    "status": 200,
    "version": "HTTP/1.1",
    "headers": [{ "name": "content-type", "value": "application/json" }],
    "body": "{\"ok\":true}",
    "bodyLen": 11,
    "timings": { "totalMs": 9.09, "ttfbMs": 8.93, "receiveMs": 0.158 }
  },
  "checks": [
    { "name": "status is 201", "passed": true },
    { "name": "json path `id`", "passed": false, "message": "expected `1`, got `2`" }
  ],
  "logs": ["pre-request script ran"]
}
```

`checks` are the declarative assertions plus any `lev.test(...)` outcomes, in order;
`logs` are the script's `lev.log` lines. Both are **redacted** and both are omitted when
empty. A failed check is not an `error`: the `requestResult` still carries the response,
and the client decides what a failed check means (the CLI exits `1`).

`request` is the request **as actually sent** - resolved, and redacted. This is what a
UI renders as "what went out", and it never contains a secret. The echo is computed
*after* the pre-request script, so it reflects any rewrite the script made.

`timings` carries milliseconds. `dnsMs`, `connectMs` and `tlsMs` are omitted until the
instrumented connector lands.

### `stop`

```json
{ "type": "stop", "graceful": true }
```

The session ends after the message in flight — if any — has been answered. The engine
never truncates a `requestResult`, so a client that stops a run still receives the last
reply. Because the engine dispatches one message at a time, a `stop` can only arrive
between dispatches, and `graceful: false` therefore has nothing extra to cut short; the
flag is recorded in the shutdown `log` below so the distinction stays visible on the
wire.

A session that is ending sends a final `log` before the stream closes, so the client
learns *why* it ended instead of just seeing a closed pipe:

```json
{ "type": "log", "level": "info", "msg": "engine stopping: signal" }
```

The cause is `signal` (the host process was asked to stop), `stop`, or
`stop (ungraceful)`. The message is best-effort: if the peer is already gone the send
is dropped, and a failed send never turns a clean stop into an error.

### `log` / `error`

```json
{ "type": "log", "level": "warn", "msg": "..." }
{ "type": "error", "code": "config", "msg": "unresolved variable `token`" }
```

Log levels: `debug`, `info`, `warn`, `error`.

Error codes:

| `code` | Meaning |
|---|---|
| `protocolVersionMismatch` | Major versions differ |
| `badMessage` | Not parseable as a protocol message, or out of order |
| `config` | The request, collection, environment **or script** is invalid (a script that fails to compile, throws, runs away, or exhausts memory lands here) |
| `engine` | The request could not be sent (transport, TLS, DNS, timeout) |
| `unsupported` | Valid, but not implemented by this build |
| `internal` | A bug in the engine |

Clients branch on `code`, never on `msg`.

## 5. Invariants

1. **No secret value ever crosses the boundary.** Secrets appear only as *names*
   (`{ "secret": "..." }`) and are resolved inside the engine. Every path that could
   carry a secret - the request echo, response headers, the response body, **and every
   check name, check message and script log** - is redacted by the engine *before* it is
   serialized. A target that reflects a token back, or a script that logs one, cannot
   leak it to a client.
2. **Aggregates, not samples.** Interactive responses are buffered; load-mode
   snapshots (Phase 3) carry aggregates only.
3. **A rejected handshake is explicit.** One `error`, then a clean close.
4. **A protocol mistake is survivable.** A wrong-direction or unknown message yields
   `error` and the session continues.

## 6. Reserved — not in v0

`loadRunRequest`, `runStarted`, `snapshot`, `runFinished`, `stepEvent`, `configure` and
`attach` are **reserved** for the load path (Phase 3) and remote agents (Phase 6). They
are intentionally *not* frozen yet: freezing a message shape that has never been
stress-tested is how protocols rot. They will be added in a minor bump once `lev-load`
exists. The envelope is already designed to carry them.

## 7. Where this is tested

- `crates/lev-protocol/src/` - versioning, serde shape, transport framing.
- `crates/lev-engine/tests/protocol.rs` - the boundary end to end against a
  `wiremock` target, plus version-mismatch and wrong-direction handling.
- `crates/lev-engine/tests/scripts.rs` - the Phase 2 exit criteria through the boundary:
  a request with two scripts and three assertions, a runaway script killed by the CPU
  budget with the session surviving, a failed check as a non-error, and a script that
  cannot leak a secret through a log or a check.
- `crates/lev-script/tests/sandbox.rs` - the sandbox itself: request mutation, checks,
  the `lev.*` surface, seeded reproducibility, and both limits.
- `crates/lev-cli/tests/stdio.rs` - the same conversation with the **real binary**
  spawned as a child process, over NDJSON.
- `crates/lev-cli/tests/checks.rs` - checks and the exit-code contract (`0` pass, `1`
  check failed, `2` broken script) through the real binary.

## 8. Changes in v0.2

| Field | Direction | Meaning |
|---|---|---|
| `sendRequest.request.scripts` | client -> engine | `{ preRequest?, postResponse? }` JavaScript source |
| `sendRequest.request.assertions` | client -> engine | declarative checks (see §4) |
| `requestResult.checks` | engine -> client | assertion + `lev.test` outcomes, redacted |
| `requestResult.logs` | engine -> client | `lev.log` lines, redacted |

`PROTOCOL_VERSION` is `0.2`. A script failure is reported as `error` with
`code: "config"` — no new error code was introduced.

## 9. Changes in v0.3

| Field | Direction | Meaning |
|---|---|---|
| `sendRequest.request.timeoutMs` | client -> engine | overall deadline in ms; absent = engine default, never unbounded |
| `sendRequest.request.body` | client -> engine | gains the `bytes` kind: `{ "kind": "bytes", "contentType"?, "base64" }` |

`PROTOCOL_VERSION` is `0.3`. A deadline that expires is reported as `error` with
`code: "engine"` (the target failed to answer, not the request definition) — no new
error code was introduced. Engines that predate v0.3 ignore the field and apply their
own default, which is still bounded.

The `bytes` body kind is additive: a body read from a file travels base64, so the
engine needs no filesystem access. The *client* opens the file — it is the side that
knows the collection, and the side a user is watching — so an engine can run where no
collection is checked out. The `file` kind stays in the protocol for a later agent that
can confine paths to a root of its own; until then a `file` body that reaches an engine
is refused, because the engine cannot tell a fixture from `/etc/passwd`.
