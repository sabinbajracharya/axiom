# Type Checker Testing & Debugging Spec

> **Status:** authoritative for the type checker's test/debug tooling. Binding before code is written.
> **Decisions baked in:** bidirectional type checking (infer for `let`/`val`/`var`, explicit fn
> return/param types), nominal type universe, THIR output, hand-rolled snapshots (no `insta`),
> drift guard ensuring every HIR expression kind has a typing rule.
> **Companion docs:** [`hir-testing.md`](hir-testing.md) (the layer below),
> [`parser-testing.md`](parser-testing.md) (two layers below),
> [`RUST_CONVENTIONS.md`](../RUST_CONVENTIONS.md), [`ENFORCEMENT.md`](../ENFORCEMENT.md).

## 0. The concern this answers

The HIR proves "every name resolves to a definition or emits a diagnostic." The type checker
faces a different but equally fatal fear: **an expression kind the type checker doesn't handle
silently passes through untyped**, or **a type error is accepted that should be rejected
(or vice versa)**. Plain unit tests only cover cases someone thought to write. So the goal
of this spec is a tooling stack where **"every expression gets a type" and "every type error
is caught" are properties the machine enforces**, not promises a contributor makes.

Two ideas carry that weight: a *canonical THIR dump* that is simultaneously the debug tool
and the test oracle, and *coverage invariants* that prove — on every fixture — that the
type checker assigns a type to every expression node and that every Type::Error corresponds
to an emitted diagnostic (and vice versa).

---

## 1. The six layers (mirroring the HIR, parser, and lexer)

| Layer | What it is | The hole it closes |
|---|---|---|
| **1. Canonical dump** | One serializer `&Thir → String`, exposed as a CLI command *and* used by the test oracle | "I can't see what type the checker inferred" |
| **2. Golden snapshots** | `.ax` fixtures + checked-in `.thir` goldens, globbed by one test | "a change silently broke type inference or checking" |
| **3. Coverage invariants** | Drift guard (every HIR expr kind is typed) + type-error completeness (every `Ty::Error` has a matching diagnostic) | **"a case I never imagined slipped through"** ← the core fear |
| **4. Diagnostics** | Ill-typed input → specific error + span, snapshotted | "a type mismatch is silently accepted" |
| **5. Fuzz / property** | Lower+resolve+type-check random-but-well-formed trees; assert no panic, diagnostics are finite, no expression left untyped | "the unimagined case" |
| **6. Unit tests** | Pinpoint checks on fiddly atoms (unification, struct field order, match exhaustiveness, numeric coercion) | "the subtle type bug broad tests gloss over" |

Layers **3 and 5 are the load-bearing pair** — they make completeness mechanical.

---

## 2. The canonical THIR dump format (the contract)

One serializer produces this; the CLI prints it and the golden harness compares it. The
format is a **contract** — changing it regenerates every golden, so it is defined precisely
here.

### 2.1 Rules

- **One node per line.** Two-space indentation per depth level. No cross-row alignment.
  (Same rationale as all prior stages: diff-friendly over pretty.)
- **Every expression shows its type.** After name-resolution info, each expression node
  shows `: <type>` — the inferred or checked type. This is the entire point of the THIR dump.
- **Types use a compact, deterministic notation:**
  - Primitives: `Int`, `Float`, `Bool`, `String`, `Unit`
  - Named: `Point`, `Shape`, `IpAddr` (user-defined struct/enum names)
  - Tuple: `()` (unit), `(Int, Float)` (multi-field)
  - Function: `(Int, Float) -> Bool`
  - Error: `///error///` — signals a type error was diagnosed at this node
- **HirIds preserved from the HIR.** The THIR reuses HIR's `HirId`s — no new ID space.
  This is how downstream stages (IR lowering) cross-reference back to the source.
- **Resolved names shown as in the HIR dump.** `→<DefId>` for resolved, `→<unresolved>` for
  unresolved. The THIR does *not* re-resolve names — it consumes the HIR's resolution.
- **Deterministic:** same HIR input ⇒ byte-identical output. No `{:?}`, no hashes.
- **LF only**, pinned via `.gitattributes` (`*.thir text eol=lf`).

### 2.2 Line grammar

```
<depth_indent><Kind>(<HirId>) <fields> : <type>
```

- `<depth_indent>` — two spaces per nesting level.
- `<Kind>` — the THIR node kind (mirrors the HIR node kind, with type annotation).
- `<HirId>` — the stable ID carried over from the HIR.
- `<fields>` — kind-specific key=value pairs, space-separated (same as HIR dump).
- `: <type>` — the inferred or checked type for this node.

#### Examples

```
FnDef(0) name=main params=[] return_type=() : ()
  Block(1) stmts=[2] tail=Some(3) : ()
    ExprStmt(2) : ()
      Call(3) callee=print→<1000003> args=[Lit(4)] : Unit
        Lit(4) kind=String value="Hello, Axiom!" : String
StructDef(5) name=Point fields=[x: Float, y: Float]
ValStmt(6) : ()
  IdentPat(7) name=x : Int
  Bin(8) op=+ : Int
    Lit(9) kind=Int value=1 : Int
    Lit(10) kind=Float value=2.0 : ///error///
```

Note: the `StructDef` line has no type annotation — items (fn defs, struct defs, enum defs)
don't have expression types. Only statements and expressions carry type annotations.

### 2.3 Kind labels — single source of truth

Kind names come from a `ThirKind::label()` method (mirroring the HIR's labeling pattern).
A `test_no_hardcoded_kind_labels` guard scans the serializer's own source for quoted kind
labels and fails if it finds any.

---

## 3. The THIR data model

### 3.1 Design principles

The THIR is **the HIR + types**. It is not a new tree — it is the HIR's structure annotated
with type information:

- **Every expression carries a `Ty`.** No expression is left without a type. If type checking
  fails for an expression, it gets `Ty::Error` and a diagnostic is emitted — never `None` or
  silently skipped.
- **Statements carry types too** (the type of the value they produce, or `Unit` for
  declarations).
- **Items carry type information** in their signatures: `FnDef` has the fully-resolved
  return type and parameter types, `StructDef` and `EnumDef` are themselves types.
- **`HirId`s are reused.** The THIR does not introduce new IDs. `HirId` from the HIR is the
  stable reference that downstream stages (IR lowering) use. Type information is stored in
  a side table (`TypeMap: HashMap<HirId, Ty>`) keyed by `HirId`.
- **The type checker consumes HIR + its diagnostics.** It does *not* re-parse or re-resolve.
  Unresolved names from the HIR produce `Ty::Error` in the THIR — the checker does not
  try to recover name resolution failures.

### 3.2 The type universe (Ty)

```rust
/// The type universe for v0 (M2). Nominal, no generics, no traits.
/// Every expression in the THIR carries one of these.
#[derive(Debug, Clone, PartialEq)]
pub enum Ty {
    /// The 64-bit signed integer type.
    Int,
    /// The 64-bit IEEE-754 floating-point type.
    Float,
    /// The boolean type.
    Bool,
    /// The string type (heap-allocated UTF-8).
    String,
    /// The unit type `()`, also the type of statements.
    Unit,
    /// A user-defined struct type, identified by name.
    /// The DefId points to the StructDef in the HIR.
    Struct(StructTy),
    /// A user-defined enum type, identified by name.
    /// The DefId points to the EnumDef in the HIR.
    Enum(EnumTy),
    /// A function type: `(param_types) -> return_type`.
    Fn(FnTy),
    /// A tuple type (deferred to v1+ for multi-return; present now for completeness
    /// and for enum variant payloads).
    Tuple(Vec<Ty>),
    /// A type error — this expression failed type checking.
    /// Always paired with a diagnostic. Never propagated silently.
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructTy {
    pub name: String,
    pub def_id: DefId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumTy {
    pub name: String,
    pub def_id: DefId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FnTy {
    pub params: Vec<Ty>,
    pub return_type: Box<Ty>,
}
```

### 3.3 The THIR structure

The THIR does **not** duplicate the HIR's tree. Instead, it wraps the HIR and adds a
type map:

```rust
/// The output of type checking: the original HIR + type annotations + new diagnostics.
pub struct Thir {
    /// The HIR we type-checked (borrowed, not cloned).
    pub hir: Hir,
    /// Maps every HirId (expressions, statements, patterns) to its inferred/checked type.
    pub types: TypeMap,
    /// Type-check diagnostics (type mismatches, missing fields, non-exhaustive match, etc.).
    pub diagnostics: Vec<TypeDiagnostic>,
}

/// A HashMap from HirId to Ty. The THIR's core payload.
pub type TypeMap = std::collections::HashMap<HirId, Ty>;
```

This design means:
- The HIR tree is traversed as-is (no tree duplication).
- Type information is looked up by `HirId` during serialization or IR lowering.
- Downstream, the IR lowering walks the HIR tree and queries `TypeMap::get(id)` for each node.

### 3.4 Type diagnostics

One `thiserror` enum for type-check diagnostics, **separate from** `HirDiagnostic`:

```rust
#[derive(Debug, Clone, thiserror::Error)]
pub enum TypeDiagnostic {
    #[error("type mismatch: expected `{expected}`, found `{found}`")]
    TypeMismatch { expected: Ty, found: Ty, span: Span },

    #[error("undefined type: `{name}`")]
    UndefinedType { name: String, span: Span },

    #[error("unknown field `{field}` on type `{ty}`")]
    UnknownField { field: String, ty: Ty, span: Span },

    #[error("unknown variant `{variant}` on enum `{name}`")]
    UnknownVariant { variant: String, name: String, span: Span },

    #[error("call arity mismatch: `{name}` expects {expected} argument(s), found {found}")]
    CallArityMismatch {
        name: String,
        expected: usize,
        found: usize,
        span: Span,
    },

    #[error("struct `{name}` expects {expected} field(s), found {found}")]
    StructFieldCountMismatch {
        name: String,
        expected: usize,
        found: usize,
        span: Span,
    },

    #[error("struct `{name}` missing field `{field}`")]
    StructMissingField { name: String, field: String, span: Span },

    #[error("struct `{name}` has unknown field `{field}`")]
    StructUnknownField { name: String, field: String, span: Span },

    #[error("non-exhaustive match: patterns do not cover all possible values")]
    NonExhaustiveMatch { missing: Vec<String>, span: Span },

    #[error("match arms have inconsistent types: expected `{expected}`, arm has `{found}`")]
    MatchArmTypeMismatch { expected: Ty, found: Ty, arm_index: usize, span: Span },

    #[error("if branches have inconsistent types: expected `{expected}`, else has `{found}`")]
    IfBranchMismatch { expected: Ty, found: Ty, span: Span },

    #[error("loop body must produce Unit")]
    LoopBodyNotUnit { found: Ty, span: Span },

    #[error("condition must be Bool, found `{found}`")]
    ConditionNotBool { found: Ty, span: Span },

    #[error("cannot call `{name}`: not a function")]
    NotCallable { name: String, found: Ty, span: Span },

    #[error("binary operator `{op}` cannot be applied to `{left}` and `{right}`")]
    BinOpMismatch { op: String, left: Ty, right: Ty, span: Span },

    #[error("unary operator `{op}` cannot be applied to `{operand}`")]
    UnaryOpMismatch { op: String, operand: Ty, span: Span },

    #[error("cannot assign to immutable binding `{name}`")]
    AssignToImmutable { name: String, span: Span },

    #[error("return type mismatch: expected `{expected}`, body produces `{found}`")]
    ReturnTypeMismatch { expected: Ty, found: Ty, span: Span },

    #[error("`{feature}` is not yet supported in type checking")]
    NotYetSupported { feature: String, span: Span },
}
```

Rendering (`render(source)`) mirrors the established pattern: `line:col + message`.

---

## 4. The type-checking algorithm

### 4.1 Bidirectional typing (the design choice)

Axiom's v0 posture is **explicit function signatures + local inference in function bodies**.
This maps to bidirectional type checking with two modes:

- **`infer(expr)` → `Ty`:** Compute the type of `expr` from its subexpressions and the
  environment. Used for expressions whose type is not constrained by context (e.g., the
  initializer of `val x = expr`).
- **`check(expr, expected)` → `Ty`:** Verify that `expr` produces a type compatible with
  `expected`, emitting a diagnostic if not. Used when a type annotation constrains the
  expression (e.g., `val x: Int = expr`, function return types, `if`/`else` branch
  unification).

Both modes return a `Ty` (never `None`). On error, they return `Ty::Error` and emit a
diagnostic. The `Ty::Error` propagates silently (does not cascade error messages) but is
tracked by the coverage invariant.

### 4.2 Typing rules (v0 subset)

The following table defines the typing rule for each expression kind. Every rule must be
implemented; the drift guard verifies this exhaustively.

| Expression | `infer` rule | `check` rule |
|---|---|---|
| `Lit(Int(_))` | `Int` | check against expected; mismatch → error |
| `Lit(Float(_))` | `Float` | same |
| `Lit(Bool(_))` | `Bool` | same |
| `Lit(String(_))` | `String` | same |
| `Lit(Unit)` | `Unit` | same |
| `Path(NameRef::Resolved)` | Look up the binding's type in the environment | check against expected |
| `Path(NameRef::Unresolved)` | `Error` (diagnostic already emitted at HIR level) | `Error` |
| `Bin(op, left, right)` | Infer both sides; apply operator rules (arithmetic → numeric, comparison → Bool, logical → Bool) | check against expected after inference |
| `Unary(op, operand)` | Infer operand; apply operator rules (Neg → numeric, Not → Bool) | check against expected |
| `Call(callee, args)` | Look up callee's type (fn type or Error); check arity + arg types; result is fn return type | same |
| `MethodCall(receiver, method, args)` | Infer receiver; look up method (v1: trait dispatch; v0: Error + `NotYetSupported`) | same |
| `Field(receiver, field)` | Infer receiver (must be struct); look up field type | check against expected |
| `Index(base, index)` | `NotYetSupported` (v0 has no indexable types) | same |
| `Block(stmts, tail)` | Type is `tail`'s type, or `Unit` if no tail | check against expected |
| `If(cond, then, else)` | Infer then + else; they must agree; result is then's type. If no else, result is `Unit` | check both branches + cond |
| `Match(scrutinee, arms)` | Infer scrutinee; check exhaustiveness; all arms must produce same type | check each arm against expected |
| `Loop(body)` | Always `Unit` | must be `Unit` |
| `StructLit(type_name, fields)` | Look up struct def; check field names, types, completeness | check against expected struct type |
| `Assign(target, value)` | `Unit`; check target is mutable (`var`); check value type matches target type | must be `Unit` |

### 4.3 Statement typing

| Statement | Type | Notes |
|---|---|---|
| `ValStmt(pattern, ty, value)` | `Unit` | Infer `value`; if `ty` annotation present, `check(value, ty)`. Bind pattern variables in scope. |
| `VarStmt(pattern, ty, value)` | `Unit` | Same as ValStmt but the binding is mutable (`var`). |
| `ExprStmt(expr)` | `Unit` | Infer `expr`; discard its type (but still type-check it). |
| `ReturnStmt(value)` | `Never` (v1 concept; v0: check against enclosing fn return type) | No binding; the function's return type constrains this. |

### 4.4 Function-level typing

1. **Collect fn signatures.** First pass: register every `fn`'s name, param types, and
   return type in the type environment. This allows mutual recursion and forward references.
2. **Type-check fn bodies.** Second pass: for each `fn`, create a scope with its params,
   then `check(body, return_type)`.

This two-pass approach mirrors the HIR's own `collect → resolve` pattern and avoids
forward-reference issues.

### 4.5 Struct and Enum typing

- **Structs:** Register the struct's name and field types in the type environment during
  the collection pass. When a `StructLit` refers to one, look it up, check all fields are
  present (no missing, no unknown), and check each field's type.
- **Enums:** Register the enum's name and variant payloads. When a `Call` constructs a
  variant (e.g., `Circle(3.0)`), look up the enum, find the variant, and check the argument
  count and types.
- **Match exhaustiveness:** For `match` on an enum scrutinee, verify that the patterns cover
  all variants. Wildcard `_` covers everything. `OrPat` covers union.

### 4.6 Error recovery

Type errors do **not** halt the checker. When a type mismatch is found:

1. Emit a `TypeDiagnostic`.
2. Assign `Ty::Error` to the offending node.
3. Continue checking sibling and parent nodes.

`Ty::Error` is "sticky" in subexpressions: if a subexpression is `Error`, the parent
expression is also `Error` (without emitting an additional diagnostic — one error per
root cause). This prevents cascading error messages.

The coverage invariant ensures every `Ty::Error` has a corresponding diagnostic, and
every diagnostic has a `Ty::Error` somewhere (bidirectional check).

---

## 5. The drift guard (the load-bearing "nothing missed" proof)

The type checker must assign a type to **every expression and statement node in the HIR**.
This is the type-checker's analogue of the lowerer's drift guard:

```rust
#[test]
fn test_typecker_handles_every_hir_expr_kind() {
    // Every Expr variant and Stmt variant in the HIR must have a
    // typing rule in the type checker. Adding a new HIR node kind
    // without a corresponding typing rule fails this test.
    let kinds: Vec<String> = all_hir_expr_kinds(); // Enumerate from HIR's Expr enum
    for kind in kinds {
        assert!(
            typecker_handles(&kind),
            "type checker does not handle HIR expression kind: {:?}",
            kind
        );
    }
}
```

The `typecker_handles` function is an exhaustive `match` over `Expr` and `Stmt` variants —
adding a new variant to the HIR without a corresponding typing rule makes the test fail at
compile time (exhaustive match) or runtime (the guard catches it).

Additionally:

```rust
#[test]
fn test_every_error_type_has_diagnostic() {
    // For every HirId where TypeMap has Ty::Error, there must be
    // a corresponding TypeDiagnostic. For every TypeDiagnostic,
    // there must be at least one HirId with Ty::Error in the TypeMap.
    // This is the type-checker's analogue of the HIR's check_all.
}
```

---

## 6. The public API

```rust
/// Type-check an HIR, producing a THIR (HIR + type map + diagnostics).
/// The HIR is consumed (moved) — the THIR owns it.
/// Never panics on user-reachable input. Returns a Thir even if
/// type errors exist; diagnostics are in `thir.diagnostics`.
pub fn check(hir: Hir) -> Thir;

/// Canonical serialization of the THIR (the test oracle and debug dump).
/// Shows every expression with its inferred/checked type.
pub fn serialize(thir: &Thir) -> String;

/// Coverage checks: verifies that every HirId has a type in the TypeMap,
/// and that every Ty::Error has a corresponding TypeDiagnostic.
/// Returns Ok(()) if coverage is clean, or a list of coverage errors.
pub fn check_all(thir: &Thir) -> Result<(), Vec<TypeckCoverageError>>;

/// A coverage error — a type check gap discovered by `check_all`.
pub enum TypeckCoverageError {
    /// An expression/statement HirId has no entry in the TypeMap.
    UntypedExpression { id: HirId, kind: String },
    /// A Ty::Error with no corresponding TypeDiagnostic.
    ErrorWithoutDiagnostic { id: HirId, ty: Ty },
}
```

The entry point takes an `Hir` (owned, since the THIR wraps it), returns a `Thir`
(always present — type errors are in diagnostics, never a panic), and never fails.

---

## 7. Test file layout

```
crates/typecheck/
  tests/
    golden.rs          # .ax → .thir snapshot tests (Layer 2)
    diagnostics.rs     # error .ax → .stderr diagnostic snapshots (Layer 4)
    invariants.rs      # drift guard + type-error completeness (Layer 3)
    fuzz.rs            # no-panic + termination + property fuzz (Layer 5)
    fixtures/
      hello.ax          # simple, inferable types
      arithmetic.ax     # numeric ops, type inference
      control_flow.ax   # if/else, match, loop
      structs.ax        # struct definitions, field access, struct literals
      enums.ax          # enum definitions, variant construction
      match_patterns.ax # match exhaustiveness, pattern types
      functions.ax      # fn defs, calls, return types
      assignments.ax    # val/var, mutability checking
      methods.ax        # method calls (v0: NotYetSupported diagnostics)
      bindings.ax       # val/var with type annotations
      errors/
        type_mismatch.ax          # assigning Int where Float expected
        undefined_type.ax          # referencing a nonexistent type
        unknown_field.ax           # accessing a field that doesn't exist
        unknown_variant.ax         # constructing a variant that doesn't exist
        call_arity_mismatch.ax    # wrong number of arguments
        struct_field_mismatch.ax  # wrong struct fields
        non_exhaustive_match.ax   # match that doesn't cover all variants
        match_arm_type_mismatch.ax # match arms producing different types
        if_branch_mismatch.ax      # if/else branches producing different types
        not_callable.ax            # calling a non-function
        assign_to_immutable.ax     # assigning to a val binding
        return_type_mismatch.ax    # function body type ≠ declared return type
```

Golden fixtures use `UPDATE_SNAPSHOTS=1 cargo test` to regenerate, matching the
established pattern. The typeck crate tests fixture `.ax` files (shared where possible
with the HIR test corpus), plus an internal `fixtures/` directory for typeck-specific
error cases.

---

## 8. The THIR dump with type annotations (examples)

### A simple function

Axiom source:
```
fn add(a: Int, b: Int) -> Int { a + b }
```

THIR dump:
```
FnDef(0) name=add params=[a: Int, b: Int] return_type=Int : (Int, Int) -> Int
  Block(1) stmts=[] tail=Some(2) : Int
    Bin(2) op=+ : Int
      Path(3) name=a→<1> : Int
      Path(4) name=b→<2> : Int
```

### Struct + match

Axiom source:
```
struct Point { x: Float, y: Float }
enum Shape { Circle(Float), Rect(Float, Float) }

fn area(s: Shape) -> Float {
  match s {
    Circle(r) -> 3.14159 * r * r
    Rect(w, h) -> w * h
  }
}
```

THIR dump (abbreviated):
```
StructDef(5) name=Point fields=[x: Float, y: Float]
EnumDef(8) name=Shape variants=[Circle(Float), Rect(Float, Float), ]
FnDef(14) name=area params=[s: Shape] return_type=Float : (Shape) -> Float
  Block(15) stmts=[] tail=Some(16) : Float
    Match(16) scrutinee= : Float
      Path(17) name=s→<14> : Shape
      Arm pattern=Circle(IdentPat(18) name=r) body=
        Bin(19) op=* : Float
          Bin(20) op=* : Float
            Lit(21) kind=Float value=3.14159 : Float
            Path(22) name=r→<18> : Float
          Path(23) name=r→<18> : Float
      Arm pattern=Rect(IdentPat(24) name=w, IdentPat(25) name=h) body=
        Bin(26) op=* : Float
          Path(27) name=w→<24> : Float
          Path(28) name=h→<25> : Float
```

### A type error

Axiom source:
```
fn main() { val x: Int = 3.14 }
```

THIR dump (relevant excerpt):
```
ValStmt(2) : Unit
  IdentPat(3) name=x : Int
  Lit(4) kind=Float value=3.14 : ///error///
```

With a diagnostic:
```
1:16: type mismatch: expected `Int`, found `Float`
```

---

## 9. Match exhaustiveness (the headline v0 feature)

### 9.1 Rules

- When the scrutinee is a known enum type, the checker verifies that all variants are
  covered by the match arms. A `_` wildcard covers all remaining variants.
- `OrPat` covers the union of its alternatives' variants.
- Literal patterns (int, string, bool) need not be exhaustive in v0 — only enum types
  require exhaustiveness checking. (Full exhaustiveness for literal patterns is v1+.)
- If the scrutinee's type is `Ty::Error`, exhaustiveness is skipped (one error per root
  cause, no cascading).

### 9.2 Reporting

When a match is non-exhaustive, the diagnostic lists the uncovered variants:
```
3:3: non-exhaustive match: patterns do not cover all possible values
missing: `Empty`
```

---

## 10. Commands

```bash
cargo test -p typecheck                            # full suite
cargo test -p typecheck --test fuzz                # fuzz/property tests only
UPDATE_SNAPSHOTS=1 cargo test -p typecheck          # regenerate .thir / .stderr
cargo run -p typecheck --example typeck -- file.ax # debug THIR dump
cargo run -p cli -- check file.ax                # CST + HIR + THIR dumps + diagnostics
```

---

## 11. When you change this crate

- Add a `Ty` variant: add a variant to `Ty`, add a `label()` arm, update `serialize`,
  add golden fixtures that exercise it. The drift guard will pass when the checker handles
  the new type.
- Add a new HIR expression kind (upstream): the drift guard `test_typecker_handles_every_hir_expr_kind`
  will fail until the type checker has a typing rule for it.
- Add a new type diagnostic kind: add a variant to `TypeDiagnostic`, add a fixture in
  `errors/*.ax` + checked-in `.stderr`, regenerate with `UPDATE_SNAPSHOTS=1`.
- Add a new type-checking rule: add a unit test first (pinpoint), then implement. Add a
  golden fixture that exercises it.

---

## 12. Architecture (binding)

On top of `RUST_CONVENTIONS.md`:

### 12.1 Functional by default, one stateful core
- **Pure functions everywhere except the type checker.** The type checker is a stateful
  walk over the HIR that builds a `TypeMap` and `Vec<TypeDiagnostic>`. It is a struct
  (`TypeChecker`) holding the type environment (scopes) and the ongoing results.
- The `serialize` function and `check_all` are pure transforms, touching no shared state.
- **The line:** *pure transforms by default; localized mutation only inside the type checker,
  never in the serializer or the test helpers.*

### 12.2 Two-pass type checking (mirrors HIR's two-pass resolution)
1. **Collect pass:** Walk all item definitions, register fn signatures, struct definitions,
   and enum definitions in the type environment. This allows forward references.
2. **Check pass:** Walk fn bodies, type-checking each expression against the environment.
   This is where type errors are emitted and `Ty::Error` nodes are created.

### 12.3 Type environment (scoped)

```rust
/// The type environment: a stack of scopes mapping names to types.
/// Entries are pushed for fn params, val/var bindings, match-arm bindings.
struct TypeEnv {
    scopes: Vec<Scope>,
}

struct Scope {
    bindings: HashMap<String, (Ty, DefId, Mutability)>,
}

enum Mutability {
    Immutable, // val
    Mutable,   // var
}
```

Lookups walk the scope stack from innermost to outermost. Shadowing follows the same
rules as the HIR (inner scopes can shadow, same scope cannot redeclare).

### 12.4 File-size and complexity caps
- `typeck/` subfolder when files exceed 600 lines (following the lexer/parser pattern).
- Complexity lints enforced mechanically: `too_many_lines` ≤ 60, `too_many_arguments` ≤ 5,
  `cognitive_complexity` capped.
- `unwrap`/`expect`/`panic` denied by workspace lints on user-reachable paths.

---

## 13. Directory layout

```
crates/typecheck/
├── src/
│   ├── lib.rs            # pub use of the public API (check, serialize, check_all)
│   ├── types.rs           # Ty, StructTy, EnumTy, FnTy, Mutability — the type universe
│   ├── thir.rs            # Thir, TypeMap — the THIR wrapper around HIR
│   ├── typeck.rs          # TypeChecker struct + two passes (collect + check)
│   ├── exhaustiveness.rs  # match exhaustiveness checking (separate, testable in isolation)
│   ├── error.rs           # TypeDiagnostic (thiserror enum) + render
│   ├── serialize.rs       # canonical THIR serializer (pure function)
│   └── coverage.rs        # check_all + TypeckCoverageError
└── tests/
    ├── golden.rs          # globs fixtures/*.ax, compares fixtures/*.thir
    ├── invariants.rs      # drift guard + type-error completeness
    ├── diagnostics.rs     # malformed inputs → snapshotted error + span
    ├── fuzz.rs            # randomized no-panic + termination + property
    └── fixtures/
        ├── *.ax           # source samples (shared types with HIR corpus where possible)
        ├── *.thir         # golden THIR dumps
        └── errors/*.ax    # ill-typed samples + *.stderr goldens
```

Per-folder `README.md` carries the file→responsibility table, updated in the same change
as any file move.

---

## 14. Build order (TDD)

1. `types.rs` — the `Ty` enum + `StructTy`, `EnumTy`, `FnTy`, label methods, `Display` impl.
   Unit tests for `Display` rendering (compact type notation).
2. `error.rs` — `TypeDiagnostic` enum + `render(source)`. Unit tests for each variant's
   message format.
3. `thir.rs` — `Thir`, `TypeMap` structs. Minimal, no logic yet — just the container.
4. `coverage.rs` — `check_all` + `TypeckCoverageError`. At this stage, it's almost trivial
   (empty TypeMap check), but it exists from the start and grows with the checker.
5. `typeck.rs` — the `TypeChecker` struct, `TypeEnv`, two passes. Written test-first:
   each typing rule gets a failing test, then implementation. Start with literals and
   paths, then binary/unary ops, then calls, then struct/enum, then match, then loops.
6. `exhaustiveness.rs` — match exhaustiveness, separate and testable in isolation. Start
   with simple enum coverage, then `OrPat`, then wildcard.
7. `serialize.rs` — the canonical THIR dump. Pure, unit-tested on hand-built `Thir`s.
8. `golden.rs`, `diagnostics.rs` — golden snapshot and error fixture tests over the
   combined pipeline (parse → lower → check → serialize).
9. `invariants.rs` — drift guard (`every_hir_expr_kind_has_typing_rule`) + type-error
   completeness (`every_error_type_has_diagnostic`).
10. `fuzz.rs` — std-only PRNG, random HIR trees, assert no panic + every node typed.

Steps 1–4 are **green before** the type checker does anything real — so the moment
`check::check()` produces its first `TypeMap`, `check_all` is already watching it.

---

## 15. What's mechanically enforced vs judgment

| Guarantee | Mechanism | Strength |
|---|---|---|
| Type checker never panics | `panic`/`unwrap`/`expect` denied (lints) + fuzz | **Hard** |
| Every expression gets a type | `check_all` verifies TypeMap covers all HirIds | **Hard** |
| Every Ty::Error has a diagnostic | `check_all` cross-references | **Hard** |
| No silent type regression | golden `.thir` snapshots | **Hard** |
| Type labels are complete | `test_no_hardcoded_kind_labels` source-scan test | **Hard** (narrow) |
| Every HIR expr kind is typed | drift guard (exhaustive match) | **Hard** |
| Match exhaustiveness | unit tests + golden fixtures | **Hard for tested cases** |
| Error message quality | diagnostic snapshots pin them; *quality* is review | **Mixed** |
| Small, single-purpose functions | complexity lints | **Hard-ish** |
| Bidirectional typing correctness | pinpoint unit tests per expression kind | **Hard for tested kinds** |

The soft residue is kept small and named — the same philosophy as all prior testing specs:
mechanize what we can, be honest about the rest.