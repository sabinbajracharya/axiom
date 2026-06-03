# Axiom v0 — End-to-End Pipeline Plan (milestone-by-milestone)

> The detailed execution plan for `DESIGN_SPEC.md` §14's **v0** stage. Sits alongside
> `DESIGN_SPEC.md` / `RUST_CONVENTIONS.md` / `ENFORCEMENT.md` as the roadmap the work follows.

## Context

**Where we are.** Spike 0 is GREEN — Path A (no GC, no lifetimes, exclusivity discipline)
is confirmed tolerable (`docs/spike-0-findings.md`, 23/23 scenarios matched intent). The
**lexer** and **parser** are production-complete: lossless, total, fuzzed, snapshot-tested,
with 94 typed AST views over a lossless CST (`crates/axiom-lexer`, `crates/axiom-parser`).
Everything downstream of the parser is 0% — the typed AST views have no consumer yet.

**What's next, per `DESIGN_SPEC.md` §14.** The next milestone is **v0 — the end-to-end
skeleton with NO memory model**: `lex → parse → typecheck → IR → backend`, over a
value-semantics subset with *naive* memory (copy/refcount everything; no exclusivity, no
Perceus). The point of v0 is to **prove the pipeline runs real programs natively** — the
language *identity* (ownership + Perceus) is deliberately deferred to v1, which folds the
spiked passes into the IR layer this plan establishes.

**Confirmed decisions for this plan:**
- **Subset:** Int/Bool/Float/Unit/String, `fn` defs & calls, `val`/`var` + `let` bindings,
  arithmetic/comparison/logical ops, `if`/`else`, `loop`, **structs, enums, and exhaustive
  `match`**, and a `print` builtin. Generics, traits, closures, concurrency, error sets →
  deferred to v1+.
- **Backend strategy:** **one register IR**, then the **IR interpreter first** (reference
  oracle + fastest path to a running program), then **Cranelift second**, parity-checked
  against the interpreter — mirroring Oxy's proven discipline.
- **Cranelift mode:** **AOT object → native executable** (`cranelift-object` → `.o` →
  system-linker → standalone binary). `axiom build hello.ax` → `./hello`.
- **`.wasm` artifact output:** deferred to v2.x+ (a *separate* emit backend —
  `wasm-encoder`/LLVM, never Cranelift). The interpreter covers any playground need.
  *Side task:* clarify the CLAUDE.md line "register-IR interpreter for WASM" — the
  interpreter is the **portability + parity-oracle** engine, not a `.wasm` producer.

### Backend architecture (the shape we're building toward)

```
CST (done) ─► HIR (M1) ─► typed HIR / THIR (M2) ─► register IR (M3) ──┬─► IR interpreter (M4)  ◄─ first runnable
                                                  [v1: ownership +     │      oracle + portable
                                                   Perceus run HERE]   └─► Cranelift AOT (M5) ─► native executable
                                                                              parity-checked vs interpreter
        (.wasm emit = separate later backend, v2.x+ — NOT Cranelift)
```

**Why an IR at all (not "a backend choice"):** the register IR is the single lowering target
*both* backends consume, **and** it is where v1's ownership pass + Perceus refcount insertion
will run. Getting it right now is the highest-leverage work in v0.

**Shared runtime:** both backends execute identical IR and call **one** runtime/FFI surface
(`axiom-runtime`) for builtin semantics (print, value ops, aggregate alloc). The interpreter
calls these Rust fns directly; Cranelift-generated code calls them as linked symbols. This is
what makes parity meaningful — neither backend re-implements language semantics.

---

## Conventions every milestone must honor

These come from `RUST_CONVENTIONS.md` / `ENFORCEMENT.md` and the existing lexer/parser crates
— follow the established templates, don't invent new shapes:
- **New crate checklist:** add to `members` in root `Cargo.toml`; add `[lints] workspace = true`
  (inherits `unsafe_code = "forbid"`). **Exception:** the codegen crate (M5) is the *only*
  crate that opts out — its own `[lints]` block keeps every clippy deny but sets
  `unsafe_code = "allow"`. That crate split is what makes the unsafe ban a hard boundary.
- **Test-first (TDD), always.** Each stage gets a testing-spec doc *first*
  (`docs/hir-testing.md`, `docs/typeck-testing.md`, `docs/ir-testing.md`,
  `docs/backend-parity-testing.md`), modeled on `docs/lexer-testing.md` /
  `docs/parser-testing.md`: canonical snapshot dump + golden fixtures + **coverage invariants**
  + diagnostics fixtures + fuzz/property + unit tests. Write failing tests, then implement.
- **Per-component verify + debug harness (a hard deliverable for EVERY new crate, not just
  "tests exist").** Each stage ships the same six-layer kit the lexer/parser already have —
  this is how we test input→output and debug each component by hand:
  1. **Canonical `serialize` dump** — one pure `Stage → String` function (e.g.
     `axiom_lexer::serialize`, `axiom_parser::serialize`). It is the *single oracle* used by
     **both** golden tests **and** humans. Deterministic, diff-friendly, LF-only.
  2. **`examples/<stage>.rs` debug CLI** — `cargo run -p <crate> --example <stage> -- file.ax`
     dumps that stage's output for any `.ax` (the existing `examples/lex.rs`,
     `examples/parse.rs` are the template). Every stage gets one: `hir`, `typeck`, `ir`.
  3. **Golden input→output fixtures** — `tests/fixtures/*.ax` → checked-in `*.<ext>` goldens,
     regenerated with `UPDATE_SNAPSHOTS=1 cargo test`. This *is* the "test input, see output"
     loop.
  4. **Coverage / drift invariants** — `check_all`-style proofs + the per-layer "can't silently
     drift" guard (see next bullet).
  5. **Diagnostics fixtures** — malformed `*.ax` → `*.stderr` (message + span), via each
     stage's `Error::render(source)`.
  6. **Fuzz / property + unit tests** — no-panic/termination property tests + pinpoint units.
- **The "can't silently drift" guard per layer** (the lexer's `symbol_consistency`, the
  parser's `every_token_present` / `test_ast_every_node_kind_covered` are the templates):
  M1 — every AST node kind is lowered to HIR; M2 — every HIR expr is typed; M3 — exhaustive
  `match` over every HIR/THIR construct *and* the interpreter's exhaustive `match` over every
  `IrOp`/`Terminator` (adding an IR op fails the build until the interpreter handles it).
- **Enum + exhaustive `match` backbone.** One `thiserror` error enum per pipeline stage.
  No `unwrap`/`expect`/`panic` on user-reachable paths. File-size cap ≤600 lines (split into
  a `foo/` folder + `mod.rs`, as `lexer/` and `ast/` already do).
- **Per-folder `README.md`** created/updated in the same change that adds files.
- **Pre-commit gate green before every commit:** `cargo fmt --all && cargo clippy
  --all-targets -- -D warnings && cargo test`. The PostToolUse hook runs fmt+clippy after
  every `.rs` edit and feeds failures back.
- **Conventional commits**, no co-author trailers (`feat:`/`fix:`/`refactor:`/`test:`/`docs:`).

---

## M0 — Driver skeleton + feature-test harness *(deliverable: `axiom check hello.ax`)*

**Goal:** stand up the plumbing everything else plugs into, before any new pipeline stage.

- New crate **`crates/axiom-cli`** producing the `axiom` binary. Subcommand scaffold:
  `axiom check <file>` runs lex→parse and renders diagnostics (reuse `axiom_parser::parse`
  + the existing `ParseError::render(source)`); `run`/`build` stubbed to "not yet" exit codes.
- New top-level **`examples/features/**/*.ax`** corpus dir + a shared harness crate/module that
  walks it (the `.ax` feature-test harness Oxy uses; harvest the *pattern*, re-implement).
  Seed with `hello.ax` and a couple of parse-only programs.
- Decide naming now: keep `axiom` as the compiler-driver binary; `forge` (package manager)
  stays a v2 concern — note it, don't build it.

**Exit / tests:** `axiom check` prints a parse tree or well-formed diagnostics for every corpus
file; harness discovers and iterates fixtures; workspace builds clean with new crate's lints on.

---

## M1 — HIR + name resolution *(deliverable: name-resolved tree + resolution diagnostics)*

**Goal:** turn the lossless CST/AST views into a desugared, ID-keyed **HIR** where every
identifier resolves to a binding or item def.

- New crate **`crates/axiom-hir`**. Lower `axiom_parser::ast::*` views → HIR nodes
  (enum + `match` per AST family: items, stmts, exprs, patterns, types). Strip trivia; assign
  stable `HirId`s.
- **Two-pass resolution:** (1) collect item defs (fns, structs, enums, variants, fields) into a
  symbol table; (2) resolve bodies against lexical scopes (block scoping, shadowing per
  `shadowing.ax`, params, `match`-arm bindings). Minimal `mod`/`use` handling for the subset.
- Diagnostics (one `thiserror` enum): unresolved name, duplicate definition, arity placeholder.
- **Drift guard:** test asserting every AST node kind the parser can produce is handled by the
  lowerer (mirror `test_ast_every_node_kind_covered`).

**Verify + debug harness:** `axiom_hir::serialize` canonical HIR dump (resolved names →
def IDs); **`examples/hir.rs`** debug CLI (`cargo run -p axiom-hir --example hir -- file.ax`).
**Exit / tests:** `docs/hir-testing.md` written first; HIR snapshot goldens over the corpus;
resolution-error fixtures (`*.ax` → `*.stderr`); coverage guard green.

---

## M2 — Type checker (naive, no ownership) *(deliverable: typed HIR / THIR)*

**Goal:** assign a type to every expression and reject ill-typed programs — *without* any
ownership/exclusivity reasoning (that's v1).

- New crate **`crates/axiom-typeck`**. Type universe: `Int`, `Bool`, `Float`, `Unit`,
  `String`, user `struct`/`enum` (nominal), function types. Bidirectional checking with local
  inference: infer `let`/`val`/`var` from initializer; require explicit fn return/param types
  (matches the spec's v0 posture — annotations over full inference).
- Check: call arity+types, arithmetic/comparison/logical operand types, `if`/`loop`/block
  result-type unification, **struct literal fields**, **enum variant construction**, field
  access, and **`match` exhaustiveness + per-arm type agreement** (the headline v0 type-system
  work — drives layout decisions downstream).
- Output **THIR**: HIR annotated with resolved types per node. One `thiserror` error enum;
  invest in clear messages (carry spans through from HIR).
- **Drift guard:** every HIR expression kind has a typing rule (exhaustive `match`).

**Verify + debug harness:** `axiom_typeck::serialize` canonical THIR dump (type per node);
**`examples/typeck.rs`** debug CLI (`cargo run -p axiom-typeck --example typeck -- file.ax`).
**Exit / tests:** `docs/typeck-testing.md` first; typed-snapshot goldens; type-error fixtures
(mismatch, non-exhaustive match, unknown field/variant, arity); exhaustiveness unit tests.

---

## M3 — Register IR + lowering *(deliverable: well-formed IR for the whole subset)*

**Goal:** define the register IR and lower THIR into it. **Highest-leverage milestone** — this
is the layer v1's ownership + Perceus passes will later run on.

- New crate **`crates/axiom-ir`**. **CFG-based register IR, exactly like Oxide's** (model on
  `Oxide/.../vm/jit/ir.rs` + `IR_DESIGN.md`, re-implemented around Axiom semantics): the IR
  *is* a control-flow graph — `IrFunction { blocks, entry, locals, params, return_type }`,
  `BasicBlock { id, ops: Vec<IrOp>, terminator, predecessors }`, where the `terminator`
  (`Jump`/`Branch`/`Return`/`Halt`/`Panic`) is what wires blocks into the graph and
  `predecessors` records the reverse edges. Infinite virtual registers (`Reg = usize`, fresh
  per definition). `IrOp` enum: const loads,
  local load/store, arithmetic/compare/logical/bitwise, struct alloc + field get/set, enum
  construct + tag/payload access, `Copy`, `Phi`, `CallBuiltin{...}` FFI escape hatch.
  `Terminator`: `Return`, `Jump`, `Branch`, `Halt`, `Panic`.
- Lower THIR → IR: `if`/`loop` → branch/jump CFG; **`match` → decision-tree branches** over
  enum tags + literal tests; calls; struct/enum build + field/variant access; pattern
  destructure → loads. **Naive memory:** aggregates are values with copy semantics; heap values
  use straightforward refcount/clone — *no* elision, *no* reuse, *no* exclusivity (v1).
- Define the shared runtime surface here: declare the `axiom_*` builtin/FFI signatures
  (`print`, value ops, aggregate alloc) that both backends will satisfy — implemented in a new
  **`crates/axiom-runtime`** crate (plain safe Rust; linked into both backends).
- **IR invariants** (load-bearing, mirror the lexer/parser coverage layers): every block ends
  in a terminator; every vreg defined before use; CFG predecessors consistent; the
  exhaustive-`IrOp` guard.

**Verify + debug harness:** `axiom_ir::serialize` canonical IR dump (CFG-readable: blocks,
ops, terminators, predecessors); **`examples/ir.rs`** debug CLI (`cargo run -p axiom-ir
--example ir -- file.ax`) for inspecting lowered CFGs by hand.
**Exit / tests:** `docs/ir-testing.md` first; IR snapshot goldens over the corpus; IR
well-formedness invariant checks; lowering unit tests (match decision trees, loop CFGs).

---

## M4 — IR interpreter backend *(deliverable: `axiom run hello.ax` prints output — FIRST RUNNABLE)*

**Goal:** execute the IR. **This is the headline v0 milestone — the pipeline runs end to end.**

- New crate **`crates/axiom-interp`**. Runtime value rep as a tagged enum
  (`Int`/`Bool`/`Float`/`Unit`/`Str`/`Struct{fields}`/`Enum{tag,payload}`). Walk each
  `IrFunction`'s blocks, execute `IrOp`s, follow terminators; a register file per frame; a call
  stack for `fn` calls. Delegate **all** language semantics to `axiom-runtime` (the interpreter
  re-implements *nothing* — same FFI bodies the native backend will call).
- **Divergence guard:** the interpreter's dispatch is an **exhaustive `match` over `IrOp`/
  `Terminator`** — adding an IR op makes this crate fail to compile until handled (Oxy's
  type-checked guard; the v1 ownership ops will inherit this protection).
- Wire into CLI: `axiom run <file>` does lex→parse→hir→typeck→ir→interpret and prints output.

**Verify + debug harness:** `axiom run --trace <file>` (or `examples/interp.rs`) dumps the
block/op execution trace + final register state for debugging; stdout of `axiom run` is the
input→output oracle for corpus snapshots.
**Exit / tests:** the `examples/features/**` corpus runs end-to-end with stdout snapshots;
`hello.ax`, `fib.ax`, `fizzbuzz.ax`, and a struct+enum+`match` program all produce correct
output; runtime-trap fixtures (e.g. arithmetic panic) behave deterministically.

---

## M5 — Cranelift AOT native backend + parity *(deliverable: `axiom build hello.ax` → `./hello`)*

**Goal:** compile the same IR to a standalone native executable, and prove it agrees with the
interpreter on every program.

- New crate **`crates/axiom-codegen`** — **the single `unsafe`-permitted crate.** Its own
  `[lints]` block: every clippy deny retained, `unsafe_code = "allow"`. All `unsafe` blocks
  carry `// Safety:` comments and sit behind safe APIs. Deps: `cranelift-codegen`,
  `cranelift-frontend`, `cranelift-module`, `cranelift-object`, `cranelift-native` (added to
  `workspace.dependencies`).
- IR → CLIF: translate `IrFunction`/`BasicBlock`/`IrOp` to Cranelift IR (vregs → CLIF
  values/variables, blocks → CLIF blocks, terminators → jumps/brif/return). Declare the
  `axiom_*` runtime symbols as imports. Use **`ObjectModule`** → emit `.o` → invoke the system
  linker (`cc`) to link the object + `axiom-runtime` (as a staticlib) → native executable.
- Wire CLI: `axiom build <file>` → `./<name>`; the produced binary runs standalone.
- **Parity harness** (`docs/backend-parity-testing.md` first): run the whole corpus through the
  interpreter AND the compiled native binary; assert identical stdout/exit per program. An
  `INTERP_UNSUPPORTED` / `NATIVE_UNSUPPORTED` marker mechanism classifies *expected* gaps as
  deferred (not regressions), like Oxy's `jit_interp_parity`.

**Exit / tests:** `cargo test` parity suite green across the corpus; `axiom build hello.ax &&
./hello` prints the expected output on a clean machine path; codegen crate is the only crate
containing `unsafe`, all blocks justified.

---

## M6 — v0 hardening & wrap *(deliverable: tagged v0, docs complete)*

**Goal:** make v0 a clean, defensible baseline before v1's memory model lands on top of the IR.

- Diagnostics quality pass across HIR/typeck (spans, fix-suggesting messages where cheap).
- Broaden `examples/features/**` to a representative corpus (functions, recursion, structs,
  enums, nested `match`, loops) — these become v1's regression bedrock.
- Per-folder `README.md` for every new crate, current and accurate.
- Confirm the full pre-commit gate green; tag/document **v0**.
- Update `DESIGN_SPEC.md` §14 status notes and **clarify the CLAUDE.md backend line** (the
  interpreter is the portability + parity-oracle engine; `.wasm` emission is a distinct v2.x+
  backend, not Cranelift; native = Cranelift AOT object).

---

## Verification (end-to-end, per milestone)

- **Per stage:** `cargo test` runs that stage's golden snapshots + coverage invariants + fuzz
  + diagnostics fixtures (the established lexer/parser pattern). Regenerate snapshots with
  `UPDATE_SNAPSHOTS=1 cargo test` and eyeball the diff.
- **M4 smoke (first runnable):** `cargo run -p axiom-cli -- run examples/features/hello.ax`
  prints the expected output; same for `fib.ax`, `fizzbuzz.ax`, and a struct+enum+`match`
  program.
- **M5 native + parity:** `cargo run -p axiom-cli -- build examples/features/hello.ax &&
  ./hello`; `cargo test --test parity` (interpreter vs native binary) green over the whole
  corpus.
- **Always:** `cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test`
  before every commit; PostToolUse hook stays green after each `.rs` edit.

## Out of scope for v0 (deferred, by design)

Ownership pass / exclusivity checker / Perceus / reuse analysis (**v1** — runs on the M3 IR);
generics, traits, closures (**v1**); concurrency `scope`/`spawn`, `forge`, LSP (**v2**); `.wasm`
emit backend + LLVM-tier backend + cycle collector (**v2.x+**).
