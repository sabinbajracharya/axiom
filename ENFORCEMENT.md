# Axiom — Convention Enforcement

> **Why this exists.** A rule that lives only in a doc is a suggestion, and suggestions get bypassed under pressure ("I forgot"). The lesson from Oxy: **mechanize every rule you can, so violations fail the build or get fed back to the author automatically.** This file maps each convention to *how* it is enforced, and is honest about what stays a matter of judgment.

## The enforcement stack

| Layer | Mechanism | Status |
|------|-----------|--------|
| 1 | **Compile-time bans** — `unsafe_code = "forbid"` (workspace lints) + the codegen crate-split | ✅ scaffolded |
| 2 | **`clippy.toml` disallow-lists** — banned types/methods/macros by name | ✅ scaffolded |
| 3 | **`-D warnings`** — clippy warnings are errors; panic-prone lints denied | ✅ scaffolded |
| hook | **Claude Code PostToolUse hook** — runs layers 1–3 after every `.rs` edit and feeds failures back to the model | ✅ scaffolded |
| 4 | **Custom checks** (dylint / scripts) — Safety comments, no `macro_rules!`, file size, README freshness | ⏳ later |
| CI | **Pre-commit / CI gate** — same checks block merge | ⏳ later |

## What each rule maps to

| Convention (RUST_CONVENTIONS.md) | Enforced by | Hard? |
|---|---|---|
| `rustfmt`, max_width 100 | `rustfmt.toml` + `cargo fmt --check` (hook) | **Hard** |
| No `unsafe` outside codegen | `unsafe_code = "forbid"` in `[workspace.lints.rust]`; codegen crate is the sole opt-out | **Hard** |
| No `unwrap`/`expect`/`panic!`/`todo!`/`unimplemented!` on real paths | `[workspace.lints.clippy]` denies | **Hard** |
| No `RefCell`/`Cell`/`Mutex`/`RwLock` (interior-mutability/lock traps) | `clippy.toml` `disallowed-types` | **Hard** |
| Warnings = errors | `cargo clippy -- -D warnings` (hook) | **Hard** |
| Banning more types/methods later | extend `clippy.toml` | **Hard** |
| Every `unsafe` has `// Safety:` | custom check (layer 4) | ⏳ not yet |
| No `macro_rules!` | custom check (layer 4) | ⏳ not yet |
| File size, README freshness | custom check (layer 4) | ⏳ not yet |
| Multiple lifetimes / `'a: 'b` | dylint (layer 4, hard to write) | ⏳ not yet |
| "Readable by a non-expert" | review only | **Soft** (judgment — irreducible) |
| "Clone vs borrow in a hot path" | review only | **Soft** (judgment) |

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

**⚠️ Toolchain requirement (important).** The hook runs `cargo` **on the host**. If `cargo` is not on the host PATH (this machine currently has none — Oxy builds via Docker), `scripts/check.sh` **skips with a warning instead of blocking**, so the hook is effectively a no-op until the toolchain is reachable. To make enforcement actually bite locally, do one of:
- install `rustup` + `clippy` on the host (simplest), **or**
- give the script a Docker wrapper — replace the two `cargo …` calls in `scripts/check.sh` with `docker compose run --rm dev bash -c "cargo …"` once Axiom has a `docker-compose.yml` (mirroring Oxy).

CI will always have `cargo`, so the gate bites there regardless. Decide the local story when Axiom's build environment (host vs Docker) is set up.

**Notes / tuning:**
- The hook runs per `.rs` edit. If that feels slow once the codebase is large, switch the event from `PostToolUse` to `Stop` (runs once when the turn ends) by editing `.claude/settings.json`.
- `scripts/check.sh` also runs standalone (`bash scripts/check.sh`) and is the basis for the future CI gate.
- Test modules may relax panic-lints with a module-level `#![allow(clippy::unwrap_used)]` — tests legitimately unwrap.

## Adding a new enforced rule
1. Add the rule to `RUST_CONVENTIONS.md` (the *why*).
2. Mechanize it: a clippy lint level (`Cargo.toml`), a `clippy.toml` entry, or a custom check (layer 4).
3. Add a row to the table above.
4. If it can't be mechanized, mark it **Soft** and keep the soft set as small as possible.
