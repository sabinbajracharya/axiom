# examples/features — the `.ax` feature-test corpus

Real Axiom programs that exercise the compiler end-to-end. The `axiom-cli`
harness (`harness::discover`) walks this directory recursively, so **dropping a
`*.ax` file here is all it takes** to add it to the test suite.

What "passing" means grows with the pipeline (see
[`docs/v0-roadmap.md`](../../docs/v0-roadmap.md)):

- **M0 (now):** every file must lex + parse into a clean `SourceFile` — zero
  diagnostics. (`crates/axiom-cli/tests/features.rs`.)
- **M2:** type-checks.
- **M4:** runs under the interpreter with snapshotted stdout.
- **M5:** the native binary's output matches the interpreter's (parity).

So a program added here must parse with **zero diagnostics** today, even though
its later meaning (types, runtime behavior) isn't checked yet.

## Seed corpus

| File | Exercises |
|---|---|
| `hello.ax` | `fn main`, a `print` call, string literal |
| `arithmetic.ax` | functions + params, `val`/`var`, arithmetic + comparison ops, calls |
| `structs_enums_match.ax` | structs, enums, exhaustive `match`, struct literals, variant construction |
