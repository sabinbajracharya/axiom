# Deferred fixtures

These `.ax` programs exercise name resolution that M1 can't fully validate yet —
struct constructors, enum variant calls, field access, method dispatch, and
closures all produce `→<unresolved>` because the M1 resolver only handles
value-level names (functions, variables, builtins).

When M2 (type checker) lands, these should be promoted to `fixtures/` and
given `.hir` golden snapshots. Until then, they are tracked here as the
target M2 must aim for.