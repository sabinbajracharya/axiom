# Axiom — Convention Enforcement

> **Why this exists.** A rule that lives only in a doc is a suggestion, and suggestions get bypassed under pressure ("I forgot"). The lesson from Oxy: **mechanize every rule you can, so violations fail the build or get fed back to the author automatically.** This file maps each convention to *how* it is enforced, and is honest about what stays a matter of judgment.

## The enforcement stack

| Layer | Mechanism | Status |
|------|-----------|--------|
| 1 | **Compile-time bans** — `unsafe_code = "forbid"` (workspace lints) + the codegen crate-split | ✅ scaffolded |
| 2 | **`clippy.toml` disallow-lists** — banned types/methods/macros by name | ✅ scaffolded |
| 3 | **`-D warnings`** — clippy warnings are errors; panic-prone lints denied | ✅ scaffolded |
| hook | **Claude Code PostToolUse hook** — runs layers 1–3 after every `.rs` edit and feeds failures back to the model | ✅ scaffolded |
| 4 | **Custom checks** (dylint / scripts) — file size ✅ (`scripts/check.sh`); Safety comments, no `macro_rules!`, README freshness | ⏳ partial |
| CI | **Pre-commit / CI gate** — same checks block merge | ⏳ later |

## What each rule maps to

| Convention (RUST_CONVENTIONS.md) | Enforced by | Hard? |
|---|---|---|
| `rustfmt`, max_width 100 | `rustfmt.toml` + `cargo fmt --check` (hook) | **Hard** |
| No `unsafe` outside codegen | `unsafe_code = "forbid"` in `[workspace.lints.rust]`; codegen crate is the sole opt-out | **Hard** |
| No `unwrap`/`expect`/`panic!`/`todo!`/`unimplemented!` on real paths | `[workspace.lints.clippy]` denies | **Hard** |
| No `RefCell`/`Cell`/`Mutex`/`RwLock` (interior-mutability/lock traps) | `clippy.toml` `disallowed-types` | **Hard** |
| No fat methods (§8) | `clippy::too_many_lines` + `too-many-lines-threshold` | **Hard** |
| No long parameter lists (§8) | `clippy::too_many_arguments` + `too-many-arguments-threshold` | **Hard** |
| No tangled control flow (proxy for "one task", §8) | `clippy::cognitive_complexity` (nursery; generous threshold) | **Hard-ish** (proxy) |
| Single source of truth for symbols (token kinds ↔ display names) | consistency `#[test]` (Oxy `symbol_consistency.rs` pattern) | **Hard** (test) — lands with lexer crate |
| No raw string literals in the serializer | focused `#[test]` scanning the module's own source | **Hard** (test, narrow) — lands with lexer crate |
| Warnings = errors | `cargo clippy -- -D warnings` (hook) | **Hard** |
| Banning more types/methods later | extend `clippy.toml` | **Hard** |
| Every `unsafe` has `// Safety:` | custom check (layer 4) | ⏳ not yet |
| No `macro_rules!` | custom check (layer 4) | ⏳ not yet |
| File size (≤600 lines; §10) | `scripts/check.sh` line-count gate (layer 4) — fails the build; pre-existing files grandfathered with a reason | **Hard** |
| README freshness | custom check (layer 4) | ⏳ not yet |
| Multiple lifetimes / `'a: 'b` | dylint (layer 4, hard to write) | ⏳ not yet |
| "Readable by a non-expert" | review only | **Soft** (judgment — irreducible) |
| "Clone vs borrow in a hot path" | review only | **Soft** (judgment) |
| No hardcoded strings (project-wide) | review + the serializer-scan test covers the place it matters most | **Mostly Soft** (no clean lint exists) |
| DRY / no copy-paste logic | review + data-driven architecture removes the need | **Soft** (stable clippy can't detect it) |
| "One method, one task" (semantic) | the three complexity lints are *proxies*; true single-responsibility is judgment | **Soft at the margin** |

~75–80% of the conventions are mechanically hard today; the rest is judgment we accept and keep small.

## The crate-split rule (makes the `unsafe` ban real)

`unsafe_code = "forbid"` in `[workspace.lints.rust]` is inherited by every crate that declares:

```toml
[lints]
workspace = true
```

`forbid` **cannot** be re-enabled by an inner `#[allow(unsafe_code)]` — that is its defining property, and it is exactly the "no silent bypass" guarantee we want. Therefore:

- **Every crate** (lexer, parser, typechecker, ownership pass, IR-gen, CLI, LSP) uses `[lints] workspace = true` → zero `unsafe` possible.
- **The codegen/FFI crate is the single exception.** It does *not* set `workspace = true`; it copies the clippy denies but sets `unsafe_code = "allow"`. All `unsafe` in the entire project is physically confined to this one crate, behind safe APIs.

A reader can trust that any file outside the codegen crate contains no `unsafe`, because the compiler refuses to build it otherwise.

## How the hook works (answers "the model will forget")

`.claude/settings.json` registers a `PostToolUse` hook on `Edit`/`Write`/`MultiEdit`. After any edit, `scripts/check.sh`:
1. Reads the tool payload; if no `.rs` file changed, exits immediately (editing docs never compiles).
2. Otherwise runs `cargo fmt --check` + `cargo clippy -- -D warnings`.
3. On failure, prints the reason to **stderr and exits 2** — Claude Code feeds that back to the model as a problem it must resolve before moving on.

This is harness-level, not model-level: it does not depend on the model *remembering* to run checks. The checks run *at* the model. Whichever model is driving (Opus, Sonnet, …), a lint failure becomes an unavoidable, surfaced problem.

**Toolchain: native, no Docker (decided).** Early Axiom is a pure-Rust project (lexer→parser→typechecker→IR→Cranelift) with **zero system dependencies**, so it builds with a plain `rustup` install — no Docker needed. Docker is deferred until there's a concrete reason (the wasm playground build, or byte-for-byte CI), and even then it would be for CI/packaging while local dev + this hook stay native.

- **Status: installed and verified.** Rust (stable, via `rustup`) with `clippy` + `rustfmt` is installed on the host. The hook has been tested end-to-end: a crate using `unsafe` and `.unwrap()` fails (`-F unsafe-code` + `-D clippy::unwrap-used`) and `scripts/check.sh` exits 2. Enforcement is **live**.
- `scripts/check.sh` sources `~/.cargo/env` itself, so it finds `cargo` even in the non-interactive shell a hook runs in.
- If `cargo` is ever absent (e.g. fresh machine before `rustup`), the script **skips with a warning instead of blocking** — install with `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y`.
- **Reproducibility without Docker:** pin the toolchain with a `rust-toolchain.toml` (`channel = "1.96.0"`, `components = ["clippy", "rustfmt"]`) so every machine and CI use the same version. *(Not yet added — create when convenient.)*

CI will always have `cargo`, so the gate bites there regardless.

**Notes / tuning:**
- The hook runs per `.rs` edit. If that feels slow once the codebase is large, switch the event from `PostToolUse` to `Stop` (runs once when the turn ends) by editing `.claude/settings.json`.
- `scripts/check.sh` also runs standalone (`bash scripts/check.sh`) and is the basis for the future CI gate.
- Test modules may relax panic-lints with a module-level `#![allow(clippy::unwrap_used)]` — tests legitimately unwrap.

## Adding a new enforced rule
1. Add the rule to `RUST_CONVENTIONS.md` (the *why*).
2. Mechanize it: a clippy lint level (`Cargo.toml`), a `clippy.toml` entry, or a custom check (layer 4).
3. Add a row to the table above.
4. If it can't be mechanized, mark it **Soft** and keep the soft set as small as possible.
