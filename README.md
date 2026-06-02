# Axiom

> **Working names (revisited at 1.0):** language **Axiom** · file extension **`.ax`** · build tool / package manager **`forge`**.

Axiom is a statically typed, compiled, general-purpose language aiming for
**deterministic memory safety with no garbage collector and no lifetime
annotations**. It reads like Swift/Kotlin, types like Rust (ADTs + exhaustive
`match`), handles errors like Zig (error sets + `try`/`catch`/`errdefer`), and
does concurrency like Go (colorless green threads) — held together by one
compiler-enforced rule the others abandon: **one obvious way to do each thing.**

The heart of the language is its memory model: **Mutable Value Semantics**
(borrowing as a *calling convention* — `let`/`inout`/`sink` — not a reference
type) plus **Perceus** compile-time reference counting. This is the Hylo/Koka
resolution, *not* "Rust without the borrow checker": keep determinism, drop the
checker, replace references-as-types with conventions + refcounting.

The compiler is written in **Rust**. Native backend: **Cranelift**; a second
register-IR interpreter backend targets WASM (dual-backend).

---

## Status

**Phase: early compiler front-end.** The design is settled and the first two
pipeline stages are built test-first, lossless, and total (never panic, never
drop source). The memory model — the language's load-bearing bet — has passed
its de-risking spike.

| Stage | Component | Status |
|---|---|---|
| Design | [`DESIGN_SPEC.md`](DESIGN_SPEC.md) — full language design, every decision tagged `[Decided]`/`[Deferred]` | ✅ Settled (living doc) |
| Memory-model spike | [`docs/spike-0-findings.md`](docs/spike-0-findings.md) — Path A de-risk | ✅ **Preliminary GREEN** (23/23 scenarios matched intent; named follow-ups remain) |
| Lex | [`crates/axiom-lexer`](crates/axiom-lexer) — source → lossless, tiling token stream | ✅ Done (snapshot + invariant + fuzz tested) |
| Parse | [`crates/axiom-parser`](crates/axiom-parser) — tokens → lossless CST (rust-analyzer-shaped green/red tree) | ✅ Done; total recovery, recovery-set-aware |
| Typed AST / name resolution | — | ⬜ Not started |
| Type checking | — | ⬜ Not started |
| Ownership pass + Perceus | — | ⬜ Not started (the v1 identity) |
| IR + Cranelift codegen | — | ⬜ Not started |
| `forge`, LSP | — | ⬜ Not started |

**Path A is chosen** (systems-capable: no GC, zero-cost, exclusivity discipline);
Path B (simpler, with a GC escape hatch) remains the documented fallback if the
exclusivity rule proves too costly in practice.

### What's next

Per [`DESIGN_SPEC.md` §14](DESIGN_SPEC.md), the **v0** milestone is an
end-to-end pipeline `lex → parse → typecheck → IR → Cranelift` with *naive*
memory (no exclusivity) — to prove the pipeline runs end to end. Lex and parse
are done; the immediate frontier is the **typed AST + name resolution**, then a
minimal type checker. The real memory model (ownership pass + Perceus), generics,
and full error handling land in **v1**, where the language identity arrives.

---

## Repository layout

```
.
├── DESIGN_SPEC.md        # The language design — source of truth for any design choice
├── RUST_CONVENTIONS.md   # How we write Rust here: simple, non-expert-readable
├── ENFORCEMENT.md        # How the conventions are mechanically enforced (lints + hooks)
├── CLAUDE.md             # Orientation for AI/code agents working in the repo
├── clippy.toml           # Complexity caps + ban-lists (Layer 2 enforcement)
├── Cargo.toml            # Workspace + centralized [workspace.lints] policy
├── crates/
│   ├── axiom-lexer/      # Stage 1: lossless, total tokenizer
│   └── axiom-parser/     # Stage 2: lossless CST + error recovery
├── docs/
│   ├── lexer-testing.md  # Test/debug tooling spec for the lexer
│   ├── parser-testing.md # Test/debug tooling spec for the parser
│   └── spike-0-findings.md  # Memory-model spike result + Path A/B decision
└── scripts/              # check.sh and friends (the PostToolUse enforcement hook)
```

Each crate carries its own `README.md` with a per-file responsibility table —
start there when diving into a stage.

---

## Build & test

Requires a stable Rust toolchain (edition 2021).

```bash
cargo build                                      # build the workspace
cargo test                                       # all tests (incl. fuzz suites)
cargo fmt --all                                  # format (max_width 100)
cargo clippy --all-targets -- -D warnings        # lint — warnings are errors
```

**Pre-commit gate** (all must pass):

```bash
cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test
```

Try a stage directly:

```bash
cargo run -p axiom-lexer --example lex -- path/to/file.ax     # dump tokens
```

---

## Design principles (the load-bearing rules)

- **Singular idiom, compiler-enforced.** One loop keyword, one branching tool
  (`match`), one mandatory formatter. Overlapping syntax is rejected by design.
- **Deterministic safety without a GC or lifetimes** — Mutable Value Semantics +
  Perceus, not a borrow checker.
- **No** `async`/`await` (colorless concurrency), **no** algebraic effects, **no**
  inheritance, **no** exceptions, **no** lifetimes in the language surface.
- **Front-end is lossless and total.** Lexer and parser reconstruct their input
  byte-for-byte and never fail — malformed input yields error tokens/nodes plus a
  diagnostics list, never a panic. This is what makes fuzzing assert real
  invariants on *every* input.
- **Simple, non-expert-readable Rust.** Enums + exhaustive `match` over clever
  abstractions; `Result` + `?` for errors; `unsafe` quarantined to the (future)
  codegen/FFI crate only. Mechanically enforced — see
  [`ENFORCEMENT.md`](ENFORCEMENT.md).

---

## Contributing

Read [`CLAUDE.md`](CLAUDE.md) for orientation, [`RUST_CONVENTIONS.md`](RUST_CONVENTIONS.md)
before writing Rust, and [`DESIGN_SPEC.md`](DESIGN_SPEC.md) before making any
language-design choice (and update it in the same change if a decision moves).

- Conventional commits: `feat:`, `fix:`, `refactor:`, `test:`, `docs:`.
- Keep per-folder `README.md`s current in the same change that adds/moves a file.
- The enforcement hook needs `cargo` on `PATH` to bite (see `ENFORCEMENT.md`).

> **Status caveat:** Axiom is in active design + early implementation. Names,
> syntax, and APIs are unstable and will change without notice before 1.0.

## License

MIT (see `[workspace.package]` in `Cargo.toml`).
