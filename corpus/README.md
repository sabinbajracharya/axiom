# corpus — the `.ax` feature-test corpus

Real Axiom programs that exercise the compiler end-to-end. The `axiom-cli`
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

(This mirrors `crates/axiom-parser/tests/fixtures/` + `fixtures/errors/`, lifted
to the whole-pipeline level. Those stay as the parser's own unit fixtures; this
corpus is the cross-crate, end-to-end set.)

## Seed corpus

| File | Exercises |
|---|---|
| `valid/hello.ax` | `fn main`, a `print` call, string literal |
| `valid/arithmetic.ax` | functions + params, `val`/`var`, arithmetic + comparison ops, calls |
| `valid/structs_enums_match.ax` | structs, enums, exhaustive `match`, struct literals, variant construction |
| `errors/missing_expr.ax` | a binding with no initializer — must be rejected |
