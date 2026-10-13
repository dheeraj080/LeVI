# Levi wire protocol — v0

**Status: frozen v0** (Phase 1). Until v1 the rules below may only be changed in
backwards-compatible ways. The crate that implements this is `lev-protocol`.

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
{ "type": "hello", "protocolVersion": { "major": 0, "minor": 1 },
  "capabilities": ["interactive"] }
```

Capabilities: `interactive`, `load`, `graphql`, `grpc`. A client must not rely on a
capability the engine did not advertise.

### `ready`

```json
{ "type": "ready", "version": { "major": 0, "minor": 1 }, "engine": "0.0.0" }
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
    "auth": { "kind": "bearer", "token": "{{api_token}}" }
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
  }
}
```

`request` is the request **as actually sent** — resolved, and redacted. This is what a
UI renders as "what went out", and it never contains a secret.

`timings` carries milliseconds. `dnsMs`, `connectMs` and `tlsMs` are omitted until the
instrumented connector lands.

### `stop`

```json
{ "type": "stop", "graceful": true }
```

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
| `config` | The request, collection or environment is invalid |
| `engine` | The request could not be sent (transport, TLS, DNS, timeout) |
| `unsupported` | Valid, but not implemented by this build |
| `internal` | A bug in the engine |

Clients branch on `code`, never on `msg`.

## 5. Invariants

1. **No secret value ever crosses the boundary.** Secrets appear only as *names*
   (`{ "secret": "..." }`) and are resolved inside the engine. Every path that could
   carry a secret — the request echo, response headers, and the response body — is
   redacted by the engine *before* it is serialized. A target that reflects a token
   back cannot leak it to a client.
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

- `crates/lev-protocol/src/` — versioning, serde shape, transport framing.
- `crates/lev-engine/tests/protocol.rs` — the boundary end to end against a
  `wiremock` target, plus version-mismatch and wrong-direction handling.
- `crates/lev-cli/tests/stdio.rs` — the same conversation with the **real binary**
  spawned as a child process, over NDJSON.
