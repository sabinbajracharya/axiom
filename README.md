# Axiom

```
       █████╗ ██╗  ██╗██╗ ██████╗ ███╗   ███╗
      ██╔══██╗╚██╗██╔╝██║██╔═══██╗████╗ ████║
      ███████║ ╚███╔╝ ██║██║   ██║██╔████╔██║
      ██╔══██║ ██╔██╗ ██║██║   ██║██║╚██╔╝██║
      ██║  ██║██╔╝ ██╗██║╚██████╔╝██║ ╚═╝ ██║
      ╚═╝  ╚═╝╚═╝  ╚═╝╚═╝ ╚═════╝ ╚═╝     ╚═╝
   deterministic memory safety · no GC · no lifetimes
```

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

## A taste of Axiom

> ⚠️ Illustrative — the syntax below follows [`DESIGN_SPEC.md`](DESIGN_SPEC.md).
> The front-end, type checker, IR lowerer, and register-IR interpreter (VM) are
> built; structs, enums, match, and basic control flow run end-to-end. What's
> missing is Cranelift codegen, the memory model (ownership + Perceus), and most
> of the language surface (generics, error handling, concurrency).

**Structs, traits, and methods** — receivers declare a borrowing convention
(`let`/`inout`/`sink`), the same machinery as parameters; no `&self`/`&mut self`:

```rust
struct Point { x: Float, y: Float }

impl Point {
    fn origin() -> Point { Point { x: 0.0, y: 0.0 } }    // associated fn
    fn dist(let self, other: Point) -> Float { ... }     // borrows self (read)
    fn translate(inout self, dx: Float, dy: Float) {     // mutates self
        self.x += dx
        self.y += dy
    }
}
```

**Sum types + exhaustive `match`** (the *only* branching tool over ADTs; missing
a variant is a compile error):

```rust
enum Shape {
    Circle(Float),
    Rect(Float, Float),
}

fn area(let s: Shape) -> Float {
    match s {
        Circle(r)  => 3.14159 * r * r,
        Rect(w, h) => w * h,
    }
}
```

**Borrowing as a calling convention** — visible at the call site, no lifetimes:

```rust
fn rename(inout u: User, n: String) { u.name = n }
fn archive(sink u: User) { db.store(u) }   // consumes u

var u = User { ... }
rename(inout u, "Sam")   // mutation stated at the call site
archive(sink u)          // consumption stated at the call site
// u is now invalid — referencing it is a compile-time error
```

**Errors are values** — error sets + `try`/`catch`/`errdefer`, no exceptions
(`FsError!Config` is sugar for `Result<Config, FsError>`):

```rust
error FsError { NotFound, AccessDenied }

fn read_config(path: String) -> FsError!Config {
    val file = try open(path)        // propagate on Err
    errdefer log_failure(path)       // runs only on the error-return path
    val text = try file.read_all()
    return parse(text)               // success auto-wrapped in Ok
}

val cfg = read_config("/etc/app") catch |e| match e {
    FsError.NotFound     => default_config(),
    FsError.AccessDenied => panic("permission denied"),
}
```

**One unified `loop`, `if`/`match`/blocks are expressions**:

```rust
loop x in items { print(x) }          // iterator form
loop if ready() { tick() }            // pre-condition form (replaces while)

val grade = if score >= 90 { "A" } else if score >= 80 { "B" } else { "C" }
```

**Structured (colorless) concurrency** — green threads in a lexical `scope`
nursery; no `async`/`await`, no function coloring:

```rust
fn fetch_all(let urls: List<String>) -> List<Response>!NetError {
    scope |s| {
        val handles = urls.map(|u| s.spawn(|| http_get(u)))
        handles.map(|h| try h.join())
    }   // scope can't exit until every spawned task finishes or is cancelled
}
```

---

## Status

**Phase: front-end complete, IR + register-IR interpreter running.** The design
is settled and six pipeline stages are built test-first, lossless, and total
(never panic, never drop source). The VM executes basic programs end-to-end
(structs, enums, match, control flow, function calls). The memory model — the
language's load-bearing bet — has passed its de-risking spike.

| Stage | Component | Status |
|---|---|---|
| Design | [`DESIGN_SPEC.md`](DESIGN_SPEC.md) — full language design, every decision tagged `[Decided]`/`[Deferred]` | ✅ Settled (living doc) |
| Memory-model spike | [`docs/spike-0-findings.md`](docs/spike-0-findings.md) — Path A de-risk | ✅ **Preliminary GREEN** (23/23 scenarios matched intent; named follow-ups remain) |
| Lex | [`crates/axiom-lexer`](crates/axiom-lexer) — source → lossless, tiling token stream | ✅ Done (snapshot + invariant + fuzz tested) |
| Parse | [`crates/axiom-parser`](crates/axiom-parser) — tokens → lossless CST (rust-analyzer-shaped green/red tree) | ✅ Done; total recovery, recovery-set-aware |
| Name resolution (HIR) | [`crates/axiom-hir`](crates/axiom-hir) — CST → desugared, ID-keyed HIR with name resolution | ✅ Done (M1); golden + diagnostic snapshot tested |
| Type checking (THIR) | [`crates/axiom-typeck`](crates/axiom-typeck) — HIR → THIR via bidirectional type checker | ✅ Done (M2); golden + diagnostic + invariant tested |
| IR generation | [`crates/axiom-ir`](crates/axiom-ir) — THIR → register IR (basic blocks, SSA-lite registers) | ✅ Done (M3); golden traces + invariants |
| Register-IR interpreter | [`crates/axiom-vm`](crates/axiom-vm) — executes IR: structs, enums, match, control flow, calls | ✅ Done (M3); 56 tests + 9 golden traces |
| Cranelift codegen | — | ⬜ Not started |
| Ownership pass + Perceus | — | ⬜ Not started (the v1 identity) |
| `forge`, LSP | — | ⬜ Not started |

**Path A is chosen** (systems-capable: no GC, zero-cost, exclusivity discipline);
Path B (simpler, with a GC escape hatch) remains the documented fallback if the
exclusivity rule proves too costly in practice.

### What's next

Per [`DESIGN_SPEC.md` §14](DESIGN_SPEC.md), the **v0** milestone is an
end-to-end pipeline `lex → parse → resolve → typecheck → IR → Cranelift` with
*naive* memory (no exclusivity) — to prove the pipeline runs end to end. The
front-end and IR are complete, and the register-IR interpreter runs basic
programs. The next frontier is a minimal **Cranelift backend** to produce native
executables. The real memory model (ownership pass + Perceus), generics, and
full error handling land in **v1**, where the language identity arrives.

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
│   ├── axiom-parser/     # Stage 2: lossless CST + error recovery
│   ├── axiom-hir/        # Stage 3: CST → desugared HIR + name resolution
│   ├── axiom-typeck/     # Stage 4: HIR → THIR (bidirectional type checker)
│   ├── axiom-ir/         # Stage 5: THIR → register IR (basic blocks, SSA-lite regs)
│   ├── axiom-vm/         # Stage 6: register-IR interpreter (structs, enums, match)
│   └── axiom-cli/        # Compiler driver (`axiom check` today; `run`/`build` later)
├── docs/
│   ├── lexer-testing.md    # Test/debug tooling spec for the lexer
│   ├── parser-testing.md   # Test/debug tooling spec for the parser
│   ├── hir-testing.md      # Test/debug tooling spec for the HIR lowerer
│   ├── typeck-testing.md   # Test/debug tooling spec for the type checker
│   ├── ir-design.md        # IR design: register model, basic blocks, lowerer
│   ├── vm-design.md        # VM design: execution model, value representation
│   ├── spike-0-findings.md # Memory-model spike result + Path A/B decision
│   └── v0-roadmap.md       # v0 milestone plan (M1–M5)
└── scripts/              # check.sh and friends (the PostToolUse enforcement hook)
```

Each crate carries its own `README.md` with a per-file responsibility table —
start there when diving into a stage.

### Test harness

**455 tests** across 7 crates. Each pipeline stage has its own testing spec
(`docs/*-testing.md`) with a 6-layer test stack:

1. **Unit tests** — Rust-side logic in `#[cfg(test)]` modules
2. **Golden snapshots** — `.ax` → `.hir`/`.cst`/`.thir`/`.stderr` files, checked in, regenerated with `UPDATE_SNAPSHOTS=1`
3. **Diagnostic snapshots** — error `.ax` files paired with `.stderr` expected-output
4. **Drift guards** — coverage invariants that fail the build if any AST/HIR node or diagnostic variant is untested
5. **Round-trip / tiling invariants** — lossless reconstruction assertions (lex, parse)
6. **Fuzz targets** — arbitrary input, assert invariants hold

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

Try it:

```bash
cargo run -p axiom-cli -- check path/to/file.ax     # lex → parse → resolve → typecheck
cargo run -p axiom-lexer --example lex -- path/to/file.ax     # dump tokens (lexer only)
```

---

## Design principles (the load-bearing rules)

- **Singular idiom, compiler-enforced.** One loop keyword, one branching tool
  (`match`), one mandatory formatter. Overlapping syntax is rejected by design.
- **Deterministic safety without a GC or lifetimes** — Mutable Value Semantics +
  Perceus, not a borrow checker.
- **No** `async`/`await` (colorless concurrency), **no** algebraic effects, **no**
  inheritance, **no** exceptions, **no** lifetimes in the language surface.
- **Front-end is lossless and total.** Lexer, parser, and HIR lowerer reconstruct
  their input byte-for-byte and never fail — malformed input yields error
  tokens/nodes plus a diagnostics list, never a panic. This is what makes fuzzing
  assert real invariants on *every* input.
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
