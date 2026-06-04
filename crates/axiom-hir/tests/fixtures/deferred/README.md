# Deferred fixtures

These `.ax` programs exercise language features beyond v0's scope. When the
features land, promote the fixture to `fixtures/` (axiom-hir) and add a typeck
golden test.

- **closures.ax** — nested `fn` with capture (closure capture, §8.2 in
  DESIGN_SPEC.md). Blocked on closure capture semantics (Spike 0 / v1+).
- **method_chains.ax** — higher-order `Fn` type syntax, lambda expressions,
  recursive struct types, `Nil` sentinel. Blocked on generics or associated
  features (v1+).

Previously deferred and now promoted (M2):
- structs_enums_match.ax → fixtures/ (enum variant construction + exhaustive match)
- struct_field_access.ax → fixtures/ (struct field access + mutation with `inout`)