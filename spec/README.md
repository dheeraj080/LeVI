# Levi collection format — spec

The Levi collection format is a **public, versioned** on-disk format: git-friendly
plain text, one request per file. This directory holds the specification and the
**conformance corpus**.

## Layout

```
collection.yaml               # id, name, vars, auth defaults, baseUrl
environments/<env>.yaml       # an overlay of vars
requests/<name>.yaml          # one request per file
load/<name>.yaml              # a load scenario: executor + steps
```

## Syntax

The concrete syntax is YAML, parsed with `serde-saphyr` (pure safe Rust, fuzz-tested not
to panic on malformed input), behind a `DocumentFormat` boundary (`lev-format`). Request
bodies are stored as YAML block scalars (`|`) so they round-trip and diff line-by-line.

```yaml
# collection.yaml
id: smokes
name: Smoke tests
baseUrl: https://api.example.com
vars:
  greeting: hello
  api_token:
    secret: payments_prod_token    # resolved from the OS keychain, never stored
auth:                              # collection default; a request may override it
  type: bearer                     # bearer | basic | apiKey
  token: "{{api_token}}"
```

```yaml
# requests/create-order.yaml
id: create-order
name: Create an order
method: POST
url: /orders
headers:
  - name: Accept
    value: application/json
body:
  json: |
    {"sku": "{{$uuid}}"}
scripts:
  preRequest: |
    lev.request.headers['X-Trace'] = lev.random.uuid();
  postResponse: |
    lev.test('created', lev.response.status === 201);
assertions:
  - status: 201
  - header: { name: content-type, contains: application/json }
  - jsonPath: { path: id, equals: "{{order_id}}" }
```

## Templates

- `{{ name }}` resolves a variable (precedence: global < collection < environment <
  runtime < iteration).
- `{{$uuid}}`, `{{$timestamp}}`, `{{$randomInt}}` are dynamic functions.

## Secrets

A value of the form `{ secret: <name> }` is a reference. The value is resolved from the
OS keychain at send time, zeroized for the run, and never written to disk or logged.

## Scripts

A request may carry two optional JavaScript hooks, authored as block scalars so they
diff line-by-line:

```yaml
scripts:
  preRequest: |
    lev.request.url = lev.request.url + '?trace=' + lev.random.uuid();
  postResponse: |
    lev.test('status is 200', lev.response.status === 200);
```

- **`preRequest`** runs after resolution and before dispatch. It may mutate
  `lev.request` (`method`, `url`, `headers`, `body`); the changes are applied to the
  request that is actually sent.
- **`postResponse`** runs after the response arrives, with `lev.response` bound
  (`status`, `headers`, `body`, and `json` when the body parses as JSON).

Scripts run in a sandboxed QuickJS runtime with a **CPU budget** (default 1s) and a
**memory limit** (default 64 MiB). A runaway script is interrupted; it fails *that*
request and never hangs or crashes the engine. The whole `lev.*` surface is:

| | |
|---|---|
| `lev.request` | the resolved request (mutable in a pre-request script) |
| `lev.response` | the response, with `.json` when the body is JSON (post only) |
| `lev.env.get(name)` / `.set(name, value)` / `.has(name)` / `.all()` | the resolved variable scope |
| `lev.test(name, fnOrBool)` / `lev.expect(value).toBe(...)` | record a check |
| `lev.random.int(min, max)` / `.float()` / `.uuid()` / `.string(len)` / `.pick(arr)` | seeded randomness (`--seed` reproduces a run) |
| `lev.log(message)` | a redacted log line |

Scripts see the resolved scope **including secrets**, but every check name, check
message, and log line is redacted by the engine before it is shown or crosses the
protocol — a script cannot leak a secret.

## Assertions

Declarative checks need no script. Each entry is a single-key mapping:

```yaml
assertions:
  - status: 200
  - header: { name: content-type, contains: application/json }
  - bodyContains: '"ok":true'
  - jsonPath: { path: user.roles.0, equals: admin }
  - responseTimeUnderMs: 500
```

`jsonPath` is a dot path over objects and numeric array indices (`id`, `user.name`,
`items.0.sku`); wildcards and filters are not supported.

A **check** is one assertion or one `lev.test(...)` outcome. A failed check is not a
request failure — the response is still returned — but `levi request` reports the checks
and exits **1** when any failed, which is what a CI job gates on.

## Load scenarios

A scenario lives beside the requests it drives — conventionally `load/<name>.yaml` — and
declares *what to run and how hard*, never *how to make the request* (that stays in the
request file):

```yaml
# load/checkout.yaml
id: checkout
name: Checkout flow
executor:
  constantVus:            # a fixed pool of virtual users
    vus: 50
    duration: 30s
steps:                    # run in order, once per iteration
  - request: create-order
    thinkTime: 1s
  - request: get-order
```

- **`executor`** selects the load model. Only `constantVus` exists today: a closed model
  (a fixed number of virtual users, each looping its steps) for a fixed `duration`.
  Open/ramping models are a later slice.
- **`steps`** is an ordered list. `request` is the `id` of a request in
  `requests/`; `thinkTime` (optional) is the pause after that step.
- A **duration** is `250ms`, `30s`, `2m`, `1h`, or a bare number read as seconds.

Running it prints a summary and (with `--out`) writes a JSON report:

```
levi run load/checkout.yaml --env dev --workers 8 --seed 1 --out report.json
```

A completed run exits `0`. Thresholds — and therefore exit code `1` for a failed one —
arrive in a later slice; a configuration error is `2` and an engine error is `3`.

## Conformance corpus

`corpus/` contains valid documents that every implementation must parse, and which must
round-trip (parse → render → parse) without change. The corpus is the gate that prevents
format drift across implementations. Run it with:

```
cargo test -p lev-format
```
