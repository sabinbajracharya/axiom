# corpus — the `.ax` feature-test corpus

Real Axiom programs that exercise the compiler end-to-end. The `cli`
harness (`harness::discover`) walks this directory recursively, so **dropping a
`*.ax` file here is all it takes** to add it to the test suite.

The corpus is organized by **expected outcome** — a milestone-stable axis, so a
program never moves as the pipeline grows:

| Directory | Contract | Checked by |
|---|---|---|
| `valid/**` | Well-formed programs. Must pass every stage built so far. | parse clean (M0) → typecheck (M2) → run w/ expected stdout (M4) → native parity (M5) |
| `errors/**` | Programs that must be rejected, with a diagnostic. | must produce ≥1 diagnostic (M0); the diagnostic's content is pinned by per-stage `*.stderr` goldens as those stages land |

So what "passing" means grows with the pipeline (see
[`docs/v0-roadmap.md`](../docs/v0-roadmap.md)), but a file's directory — and thus
its contract — stays put. A program in `valid/` must parse with **zero
diagnostics** today even though its later meaning (types, runtime behavior) isn't
checked yet; a program in `errors/` must produce a diagnostic today.

(This mirrors `crates/parser/tests/fixtures/` + `fixtures/errors/`, lifted
to the whole-pipeline level. Those stay as the parser's own unit fixtures; this
corpus is the cross-crate, end-to-end set.)

## Seed corpus

| File | Exercises |
|---|---|
| `valid/hello.ax` | `fn main`, a `print` call, string literal |
| `valid/arithmetic.ax` | functions + params, `val`/`var`, arithmetic + comparison ops, calls |
| `valid/structs_enums_match.ax` | structs, enums, exhaustive `match`, struct literals, variant construction |
| `valid/functions.ax` | function definitions and calls |
| `valid/structs.ax` | struct definitions, field access, struct literals |
| `valid/enums.ax` | enum definitions, variant construction, match |
| `valid/control_flow.ax` | `if`/`else` chains, blocks as expressions |
| `valid/loops.ax` | `loop` (conditional, iterator, infinite), `break`, `continue` |
| `valid/match.ax` | exhaustive `match` over enums |
| `valid/match_patterns.ax` | pattern destructuring in match arms |
| `valid/bindings.ax` | `val`/`var` bindings, shadowing |
| `valid/assignments.ax` | mutable variable assignment |
| `valid/methods.ax` | method calls on struct receivers |
| `valid/generics.ax` | generic functions, structs, enums |
| `valid/traits.ax` | trait definitions, impl blocks, trait method dispatch |
| `errors/missing_expr.ax` | a binding with no initializer expression |
| `errors/unclosed_call.ax` | an argument list that is never closed |
| `errors/garbage_item.ax` | stray tokens where a top-level item is expected |
