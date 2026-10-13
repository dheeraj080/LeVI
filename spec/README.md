# Levi collection format — spec

The Levi collection format is a **public, versioned** on-disk format: git-friendly
plain text, one request per file. This directory holds the specification and the
**conformance corpus**.

## Layout

```
collection.yaml               # id, name, vars, auth defaults, baseUrl
environments/<env>.yaml       # an overlay of vars
requests/<name>.yaml          # one request per file
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
```

## Templates

- `{{ name }}` resolves a variable (precedence: global < collection < environment <
  runtime < iteration).
- `{{$uuid}}`, `{{$timestamp}}`, `{{$randomInt}}` are dynamic functions.

## Secrets

A value of the form `{ secret: <name> }` is a reference. The value is resolved from the
OS keychain at send time, zeroized for the run, and never written to disk or logged.

## Conformance corpus

`corpus/` contains valid documents that every implementation must parse, and which must
round-trip (parse → render → parse) without change. The corpus is the gate that prevents
format drift across implementations. Run it with:

```
cargo test -p lev-format
```
