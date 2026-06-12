# CLAUDE.md — Axiom Language Project

> **Working names (revisit at 1.0):** language **Axiom**, file extension **`.ax`**, build tool/package manager **`forge`**.
> **Status:** Design phase. Greenfield. No compiler code yet. The immediate next step is **Spike 0** (see below).

## What Axiom is

Axiom is a statically typed, compiled, general-purpose language that delivers **deterministic memory safety with no garbage collector and no lifetime annotations**. Reads like Swift/Kotlin, types like Rust (ADTs + exhaustive `match`), errors like Zig (error sets + `try`/`catch`/`errdefer`), concurrency like Go (colorless green threads) — with one compiler-enforced rule the others abandon: **one obvious way to do each thing.**

The memory model (the heart of the language) is **Mutable Value Semantics** — borrowing as a *calling convention* (`let`/`inout`/`sink`), not a reference type — plus **Perceus** compile-time reference counting. This is *not* "Rust without the borrow checker" (that's incoherent — see the spec); it's the Hylo/Koka resolution: keep determinism, drop the checker, replace references-as-types with conventions + refcounting.

**The compiler is written in Rust.** Native backend: Cranelift (AOT object → native executable; not yet built). Second backend: a register-IR interpreter — the `vm` crate — which is the **portability + parity-oracle** engine (it runs the IR everywhere and keeps the future native backend honest), **not** a `.wasm` producer; a `.wasm` emit backend is a separate v2.x+ concern. Dual-backend, mirroring Oxy.

## The two authoritative documents — READ THESE

1. **[`DESIGN_SPEC.md`](DESIGN_SPEC.md)** — the complete language design. Every load-bearing decision is tagged **[Decided]** or **[Deferred]**. §4 (Memory Model) is the heart. §14 is the staged roadmap; §15 is the honest open-questions table. **When making any language-design choice, this is the source of truth. Don't contradict it without discussion; if a decision changes, update the spec in the same change.**

2. **[`RUST_CONVENTIONS.md`](RUST_CONVENTIONS.md)** — how we write Rust in this repo. **The top rule: write Rust a competent programmer who is *not* a Rust expert can read.** When writing or reviewing any Rust code, follow it. The §14 anti-pattern table is the quick cheat sheet.

3. **[`ENFORCEMENT.md`](ENFORCEMENT.md)** — how the conventions are **mechanically enforced** (not just documented). Most rules fail the build via `[workspace.lints]` (`unsafe_code = "forbid"`, complexity caps), `clippy.toml` ban-lists/thresholds, and `clippy -D warnings`; a Claude Code `PostToolUse` hook (`.claude/settings.json` → `scripts/check.sh`) runs the checks after every `.rs` edit and feeds failures back. **Do not silence a lint to make code pass** — fix the code, or change the rule openly in `RUST_CONVENTIONS.md` + `ENFORCEMENT.md`. The hook needs `cargo` on PATH to bite (see the toolchain note in ENFORCEMENT.md).

4. **[`docs/lexer-testing.md`](docs/lexer-testing.md)** — the test/debug tooling spec for the lexer (the first thing built). Hand-rolled token snapshots + lossless lexing + tiling/reconstruction invariants + fuzz. **When building the lexer, follow it** — especially the architecture section (pure transforms + one stateful scanner; single source of truth; data-driven extendability).

## Load-bearing rules (summary — full detail in the docs above)

### Language design (from DESIGN_SPEC.md)
- **Singular idiom, compiler-enforced.** One loop keyword, one branching tool (`match`), one mandatory formatter. Reject overlapping syntax.
- **Path A is chosen** (systems-capable: no GC, zero-cost, exclusivity discipline). Path B (simpler, GC escape hatch) is the documented fallback if Spike 0 fails.
- **No** `async`/`await` (colorless concurrency), **no** algebraic effects (contradicts colorless), **no** generational references (one memory spine, not three), **no** inheritance, **no** exceptions, **no** lifetimes in the *language*.
- **Nothing permanent is built before Spike 0 passes** (§4.10). Spike 0 must resolve the near-foundational open questions — closure-capture-of-borrows (§8.2) and subscript×exclusivity (§4.4) — *before* the real foundation is poured. These are not allowed to remain loose ends.

### Rust we write (from RUST_CONVENTIONS.md)
- **Simple, non-expert-readable Rust.** Enums + exhaustive `match` are the backbone. `Result` + `?` for errors (one `thiserror` enum per pipeline). No `unwrap`/`panic!` on user-reachable paths.
- **Avoid the readability traps:** `Rc<RefCell<T>>` for shared mutable state, custom macros, `async`, multiple lifetimes / lifetime bounds, `dyn Trait` for closed case sets (use `enum` + `match`).
- **Borrow freely for reading** (`&str`, `&[T]` — no annotations needed); clone when it removes a confusing lifetime and you're not in a hot path; `Rc`/arena only where a profiler points.
- **`unsafe` is quarantined** to codegen/FFI modules only, every block with a `// Safety:` comment, wrapped behind safe APIs. The rest of the compiler is `unsafe`-free.
- **Per-folder `README.md`** kept current — update it in the same change when you add/rename/move a file.
- **Test-first (TDD), always.** Every layer is built test-first against its testing spec (`docs/lexer-testing.md`, `docs/parser-testing.md`, …): write the failing tests/invariants first, then implement until green — never weaken a test to make code pass. Mechanize the "can't silently drift" guard for each layer (the lexer's `symbol_consistency`, the parser's coverage invariants are the templates). The pre-commit gate (`fmt && clippy -D warnings && test`) must pass before every commit.
- **Never defer work specified in a design doc.** If a plan or spec says something should be implemented, implement it — do not mark it "deferred", "later", "deferred per plan", or "for a future phase". Every task in the current scope must be completed. No exceptions.

## Build & Test (once code exists)

```bash
cargo fmt --all                                  # format (max_width 100)
cargo clippy --all-targets -- -D warnings        # lint — warnings are errors
cargo test                                       # all tests
```
**Pre-commit gate:** `cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test` — all must pass.

## Relationship to Oxy (`/Users/sabin/work/Oxide`)

Oxy is a **reference/parts donor, not a base to copy wholesale.** Axiom is a separate language and a separate repo.
- **Harvest the patterns** (study + re-implement around Axiom's semantics): pipeline skeleton, dual-backend + shared FFI + divergence guards, the `symbols.rs` single-source-of-truth pattern, the `.ax` feature-test harness, IR snapshot tests, LSP scaffold, `tug`→`forge`.
- **Do NOT copy** Oxy's `Value`, type checker, or ir_gen semantics — the ownership model changes everything downstream.
- **Do NOT copy** Oxy's expert-level Rust patterns (`Rc<RefCell>` in the runtime, custom `Clone`) — see RUST_CONVENTIONS.md for why we diverge toward simplicity.

## Roadmap (see DESIGN_SPEC.md §14 for detail)

- **Spike 0** — throwaway memory-model prototype; exit gate decides Path A vs B. *Do this before anything permanent.*
- **v0** — end-to-end pipeline (lex→parse→typecheck→IR→Cranelift), naive memory, no exclusivity. Proves the pipeline.
- **v1** — the real memory model (ownership pass + Perceus), structs/enums/traits/generics, `match`, error handling. *Language identity lands.*
- **v2** — concurrency (green threads, structured `scope`, channels), `forge`, LSP.
- **v2.x+** — optional cycle collector (if leaks prove real), LLVM-tier backend (to approach Rust/C perf), self-hosting.

## Conventions
- Conventional commits: `feat:`, `fix:`, `refactor:`, `test:`, `docs:`. No co-author trailers.
- Edition 2021 (or latest stable), workspace with `workspace.dependencies`.
- Test naming: `test_<what>_<scenario>`.

# context-mode — MANDATORY routing rules

You have context-mode MCP tools available. These rules are NOT optional — they protect your context window from flooding. A single unrouted command can dump 56 KB into context and waste the entire session.

## BLOCKED commands — do NOT attempt these

### curl / wget — BLOCKED
Any Bash command containing `curl` or `wget` is intercepted and replaced with an error message. Do NOT retry.
Instead use:
- `ctx_fetch_and_index(url, source)` to fetch and index web pages
- `ctx_execute(language: "javascript", code: "const r = await fetch(...)")` to run HTTP calls in sandbox

### Inline HTTP — BLOCKED
Any Bash command containing `fetch('http`, `requests.get(`, `requests.post(`, `http.get(`, or `http.request(` is intercepted and replaced with an error message. Do NOT retry with Bash.
Instead use:
- `ctx_execute(language, code)` to run HTTP calls in sandbox — only stdout enters context

### WebFetch — BLOCKED
WebFetch calls are denied entirely. The URL is extracted and you are told to use `ctx_fetch_and_index` instead.
Instead use:
- `ctx_fetch_and_index(url, source)` then `ctx_search(queries)` to query the indexed content

## REDIRECTED tools — use sandbox equivalents

### Bash (>20 lines output)
Bash is ONLY for: `git`, `mkdir`, `rm`, `mv`, `cd`, `ls`, `npm install`, `pip install`, and other short-output commands.
For everything else, use:
- `ctx_batch_execute(commands, queries)` — run multiple commands + search in ONE call
- `ctx_execute(language: "shell", code: "...")` — run in sandbox, only stdout enters context

### Read (for analysis)
If you are reading a file to **Edit** it → Read is correct (Edit needs content in context).
If you are reading to **analyze, explore, or summarize** → use `ctx_execute_file(path, language, code)` instead. Only your printed summary enters context. The raw file content stays in the sandbox.

### Grep (large results)
Grep results can flood context. Use `ctx_execute(language: "shell", code: "grep ...")` to run searches in sandbox. Only your printed summary enters context.

## Tool selection hierarchy

1. **GATHER**: `ctx_batch_execute(commands, queries)` — Primary tool. Runs all commands, auto-indexes output, returns search results. ONE call replaces 30+ individual calls.
2. **FOLLOW-UP**: `ctx_search(queries: ["q1", "q2", ...])` — Query indexed content. Pass ALL questions as array in ONE call.
3. **PROCESSING**: `ctx_execute(language, code)` | `ctx_execute_file(path, language, code)` — Sandbox execution. Only stdout enters context.
4. **WEB**: `ctx_fetch_and_index(url, source)` then `ctx_search(queries)` — Fetch, chunk, index, query. Raw HTML never enters context.
5. **INDEX**: `ctx_index(content, source)` — Store content in FTS5 knowledge base for later search.

## Subagent routing

When spawning subagents (Agent/Task tool), the routing block is automatically injected into their prompt. Bash-type subagents are upgraded to general-purpose so they have access to MCP tools. You do NOT need to manually instruct subagents about context-mode.

## Output constraints

- Keep responses under 500 words.
- Write artifacts (code, configs, PRDs) to FILES — never return them as inline text. Return only: file path + 1-line description.
- When indexing content, use descriptive source labels so others can `ctx_search(source: "label")` later.

## ctx commands

| Command | Action |
|---------|--------|
| `ctx stats` | Call the `ctx_stats` MCP tool and display the full output verbatim |
| `ctx doctor` | Call the `ctx_doctor` MCP tool, run the returned shell command, display as checklist |
| `ctx upgrade` | Call the `ctx_upgrade` MCP tool, run the returned shell command, display as checklist |
