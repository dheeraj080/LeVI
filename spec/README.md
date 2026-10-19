# Levi collection format — spec

The Levi collection format is a **public, versioned** on-disk format: git-friendly
plain text, one request per file. This directory holds the specification and the
**conformance corpus**.

## Layout

```
collection.yaml               # id, name, vars, auth defaults, baseUrl
environments/<env>.yaml       # an overlay of vars
requests/<name>.yaml          # one request per file
load/<name>.yaml              # a load scenario: executor + steps + thresholds
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

## Bodies

`body:` takes exactly one of these keys:

| Key | Meaning |
|---|---|
| `json` | JSON source text, templated, sent as `application/json` |
| `form` | `application/x-www-form-urlencoded` fields |
| `raw` | literal text with an optional `contentType` |
| `file` | a path, read once **relative to the collection directory**, sent byte-for-byte |
| `bytes` | content written as base64, for a fixture too small to keep in its own file |

```yaml
body:
  file: fixtures/logo.png
```

A `file` path must stay inside the collection — no absolute paths and no `..` — because
a request file is text in a repository, often from a pull request, and opening it must
not be enough to read `../../.ssh/id_rsa`. The content type comes from the extension;
an explicit `Content-Type` header always wins. Files are read once, when the collection
is loaded, so a missing fixture fails `levi validate` and `levi request` rather than
surfacing mid-run.

```yaml
body:
  bytes:
    contentType: image/png
    base64: iVBORw0KGgo=
```

## Templates

- `{{ name }}` resolves a variable (precedence: global < collection < environment <
  runtime < iteration).
- `{{$uuid}}`, `{{$timestamp}}`, `{{$randomInt}}` are dynamic functions.

## Secrets

A value of the form `{ secret: <name> }` is a reference. The value is resolved from the
`LEVI_SECRET_<NAME>` environment variable, or from the OS keychain (entry `levi/<name>`)
when the environment does not name it — on first use, cached for the run, zeroized when
the run ends, and never written to a LeVI file or logged. `levi secret set <name>` stores
one in the keychain, reading the value from stdin.

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
thresholds:               # optional: the rules the run is judged by
  http_req_duration:
    - p(95)<500           # 95th percentile under 500 ms
  http_req_failed:
    - rate<0.01           # at most 1% of requests failed
abortOn:                  # optional: stop the run as soon as one of these fails
  - http_req_failed
```

- **`executor`** selects the load model. Only `constantVus` exists today: a closed model
  (a fixed number of virtual users, each looping its steps) for a fixed `duration`.
  Open/ramping models are a later slice.
- **`steps`** is an ordered list. `request` is the `id` of a request in
  `requests/`; `thinkTime` (optional) is the pause after that step.
- A **duration** is `250ms`, `30s`, `2m`, `1h`, or a bare number read as seconds.
- **`thresholds`** maps a metric to one or more expressions. The syntax is k6's, so the
  expressions people already write work unchanged:

  | Metric | Stats | Unit of the number |
  |---|---|---|
  | `http_req_duration` | `count`, `avg`, `min`, `max`, `p(N)` | milliseconds |
  | `http_req_failed` | `count`, `rate` | `rate` is a `0..1` fraction |
  | `http_reqs`, `iterations`, `checks`, `checks_failed`, `script_errors` | `count` | a count |

  The comparison is one of `>`, `>=`, `<`, `<=`, `==`, `!=`. A stat a metric cannot
  answer — `p(95)` of a counter, `rate` of a latency — is a parse error, not a silently
  meaningless rule. A stat with no samples (a percentile of an empty run) **fails**: a
  threshold that cannot be evaluated must not read as verified.
- **`abortOn`** lists metrics whose failure should stop the run immediately instead of
  being reported at the end. Each name must also have a `thresholds` entry; one that does
  not is a parse error, because an abort rule with no rule behind it could never fire.

Running it prints a summary and (with `--out`) writes a JSON report:

```
levi run load/checkout.yaml --env dev --workers 8 --seed 1 --out report.json
```

A completed run with no failing threshold exits `0`; a failed threshold exits `1`. A
configuration error is `2` and an engine error is `3`. A run stopped by Ctrl-C, or by
`abortOn`, is reported as `aborted` and exits `4` — unless a threshold failed, which
takes precedence and exits `1`, because that is the signal CI gates on.

## Conformance corpus

`corpus/` contains valid documents that every implementation must parse, and which must
round-trip (parse → render → parse) without change. The corpus is the gate that prevents
format drift across implementations. Run it with:

```
cargo test -p lev-format
```
