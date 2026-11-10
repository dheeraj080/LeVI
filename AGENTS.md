# Agent Rules for Levi

## Environment & Execution
- **Do not run local Rust/cargo builds, tests, or checks.** The environment may not have Rust toolchain installed. All validation should rely on GitHub Actions workflows.
- **Use Windows PowerShell syntax.** Do not use Unix commands like `head`, `tail`, `cat | head`, `grep`, `gh`, etc. Use the equivalents below:
  - `head -n N` / `tail -n N` → `Get-Content file | Select-Object -First N` / `Select-Object -Last N`
  - `cat file` → `Get-Content file` or just use the `read` tool
  - `grep pattern` → `Select-String -Pattern pattern` or use the `grep` tool
  - Piping: avoid `command | head` in a single shell call; break into separate operations or use PowerShell cmdlets
- **Prefer file tools over shell.** Use `read`, `write`, `edit`, `glob`, `grep` instead of shell commands when possible. Only use `shell` for operations that truly require it.

## Project Structure
- **Workspace layout:** Cargo workspace with `crates/*` and `apps/desktop/src-tauri`. MSRV is 1.90 (see `Cargo.toml`).
- **Tauri app:** Located in `apps/desktop/`. Frontend is in `apps/desktop/ui/` (built by Vite), Rust backend in `apps/desktop/src-tauri/`. Icons are in `apps/desktop/src-tauri/icons/`.
- **CI/CD:** Two workflows in `.github/workflows/` - `ci.yml` (lint/test/msrv/coverage) and `tauri-build.yml` (Windows Tauri build). Both must pass.

## Code Quality
- **Formatting:** Follow Rust's `rustfmt` standard. CI runs `cargo fmt --all -- --check`.
- **Clippy:** CI runs `cargo clippy --workspace --exclude levi-desktop --all-targets -- -D warnings`. Fix warnings instead of allowing them.
- **Dependencies:** `cargo-deny` enforces licenses/advisories via `deny.toml`. Add only approved licenses.
- **Tests:** CI runs tests with `cargo nextest` and doc tests, excluding `levi-desktop`. Tests are platform-dependent on Linux via matrix.

## Tauri-Specific Rules
- **Bundling:** Tauri bundling must be enabled (`bundle.active: true`) with proper targets (`nsis`, `msi`) for Windows. Always include icon paths in `bundle.icon`.
- **Frontend build:** Frontend builds with `npm run build` from `apps/desktop/` using Vite (root is `ui/`).
- **Workflow alignment:** Keep `mozilla-actions/sccache-action` versions aligned between CI and Tauri workflows.

## Workflow Fixing
- **Debugging failures:** When fixing GitHub Actions failures, look at workflows first. Common issues: missing icons/bundling config, dependency versions, missing system deps, formatting/clippy violations, or cargo-deny issues.
- **Do not guess.** If logs aren't available, infer from workflow config and recent changes, but prioritize fixing the obvious config issues (like bundling/icons/sccache) first.
- **Commit minimal changes.** Only modify files necessary to fix the failing workflow(s).
