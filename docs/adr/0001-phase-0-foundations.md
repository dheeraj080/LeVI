# ADR 0001 — Phase 0 foundations

- **Status:** accepted
- **Date:** 2026-10-13
- **Phase:** 0

## Context

Starting the Levi engine as a standalone Rust workspace before any UI or protocol
exists. Four decisions were blocking, because they determine the shape of the
foundation and are expensive to reverse later.

## Decisions

### 1. On-disk format: YAML via `serde_yaml_ng`, behind a `DocumentFormat` boundary

The collection format is git-friendly plain text, one request per file. We parse it
with `serde_yaml_ng` (an actively maintained fork with a drop-in API) but keep the
concrete parser behind a `DocumentFormat` trait in `lev-format`, so the syntax is
swappable without touching `lev-core`. Request bodies are stored as YAML block
scalars (`|`), which round-trip and diff line-by-line. The on-disk syntax is pinned by
a conformance corpus in `spec/`.

*Implementation note:* `serde_yaml_ng` does **not** accept the single-key-map form for
externally-tagged enums (it requires YAML `!tags`). Bodies therefore use an
`#[serde(untagged)]` single-key enum (`{ json: ... }`), and auth an internally-tagged
enum (`{ type: bearer, ... }`). Both are natural, git-friendly YAML.

### 2. License: `MIT OR Apache-2.0` dual

Matches the Rust norm and maximizes adoption — the product thesis is adoption-driven
(tiny binary, CI funnel, a public format). The `spec/` corpus is licensed permissively
so the format can become a standard. A future hosted/enterprise layer can be licensed
separately; the core engine and format stay permissive.

### 3. Domain model: `lev-core` owns it, with no `serde`

`lev-core` holds the domain types and has **no I/O and no `serde`** — core types are
never serialized. `lev-format` owns the on-disk DTOs and all (de)serialization;
`lev-protocol` (Phase 1) will own the wire DTOs. Both convert to and from core. This is
the same discipline the protocol requires, applied uniformly, at the cost of a thin
mirrored DTO per boundary.

### 4. UI↔engine boundary: one `lev-protocol` boundary, transport varies

Every client path speaks the same `lev-protocol` DTOs. Only the transport changes:
a typed in-process channel for interactive requests, NDJSON over stdio for spawned load
runs, and a socket/WebSocket for agents. "Zero IPC" for interactive really means "zero
serialization"; the child process exists to isolate load-run OOM/crashes, not because
interactive needs a different code path.

## Consequences

- `lev-core` gains a hard, CI-enforceable rule: no `serde` dependency.
- Secrets: `lev-core` defines a `SecretStore` trait (no I/O); the OS-keychain
  implementation lands in `lev-engine` (Phase 1). Values are resolved once per run,
  zeroized, and persisted as names only.
- The engine is built as a CLI-first workspace so the unit/integration loop never links
  a GUI.
