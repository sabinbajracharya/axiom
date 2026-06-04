# HIR Testing & Debugging Spec

> **Status:** authoritative for the HIR's test/debug tooling. Binding before code is written.
> **Decisions baked in:** desugared ID-keyed HIR tree, hand-rolled snapshots (no `insta`),
> two-pass name resolution (collect defs → resolve bodies), drift guard mirroring the parser's
> `test_ast_every_node_kind_covered`.
> **Companion docs:** [`lexer-testing.md`](lexer-testing.md) (two layers below),
> [`parser-testing.md`](parser-testing.md) (one layer below),
> [`RUST_CONVENTIONS.md`](../RUST_CONVENTIONS.md), [`ENFORCEMENT.md`](../ENFORCEMENT.md).

## 0. The concern this answers

The parser proves "every byte is present" with tiling and reconstruction. The HIR faces a
different fear: **a node kind the parser can produce is silently dropped during lowering,
or a name that should resolve doesn't.** Plain unit tests only cover cases someone thought
to write. So the goal of this spec is a tooling stack where **"nothing is missed" and "every
name resolves" are properties the machine enforces**, not promises a contributor makes.

Two ideas carry that weight: a *canonical HIR dump* that is simultaneously the debug tool
and the test oracle, and *coverage invariants* that prove — on every fixture — that the
lowerer handles every AST node kind and the resolver assigns a definition to every
identifier it should.

---

## 1. The six layers (mirroring the lexer and parser)

| Layer | What it is | The hole it closes |
|---|---|---|
| **1. Canonical dump** | One serializer `&Hir → String`, exposed as a CLI command *and* used by the test oracle | "I can't see what the HIR produced" |
| **2. Golden snapshots** | `.ax` fixtures + checked-in `.hir` goldens, globbed by one test | "a change silently broke lowering or resolution" |
| **3. Coverage invariants** | Drift guard (every AST node kind is lowered) + resolution completeness (every `Ident`/`NameRef` in the v0 subset resolves to a def ID or produces a diagnostic) | **"a case I never imagined slipped through"** ← the core fear |
| **4. Diagnostics** | Malformed input → specific error + span, snapshotted | "an unresolved name is silently accepted" |
| **5. Fuzz / property** | Lower+resolve random-but-well-formed trees; assert no panic, diagnostics are finite, HirIds are unique | "the unimagined case" |
| **6. Unit tests** | Pinpoint checks on fiddly atoms (scope shadowing, match-arm bindings, struct field resolution) | "the subtle name-resolution bug broad tests gloss over" |

Layers **3 and 5 are the load-bearing pair** — they make completeness mechanical.

---

## 2. The canonical HIR dump format (the contract)

One serializer produces this; the CLI prints it and the golden harness compares it. The
format is a **contract** — changing it regenerates every golden, so it is defined precisely
here.

### 2.1 Rules

- **One node per line.** Two-space indentation per depth level. No cross-row alignment.
  (Same rationale as the lexer/parser: diff-friendly over pretty.)
- **HirIds are stable within a dump.** Assigned in a pre-order traversal of the source.
  The HIR dump orders nodes by their position in the source, not alphabetically.
- **Resolved names show their definition ID.** After name resolution, every `PathExpr`
  or `NameRef` that resolves prints the `DefId` it resolved to, not the raw text.
  Unresolved names print `<unresolved>`.
- **Deterministic:** same source ⇒ byte-identical output. No `{:?}`, no hashes.
- **LF only**, pinned via `.gitattributes` (`*.hir text eol=lf`).

### 2.2 Line grammar

```
<depth_indent><Kind>(<HirId>) <fields>
```

- `<depth_indent>` — two spaces per nesting level.
- `<Kind>` — the HIR node kind from the single source of truth enum (never hardcoded).
- `<HirId>` — the node's stable ID, a `usize` assigned in source order.
- `<fields>` — kind-specific key=value pairs, space-separated.

#### Examples

```
FnDef(0) name=main params=[] return_type=() body=Block(1)
  Block(1) stmts=[2] tail=Some(3)
    ExprStmt(2) expr=Call(3)
      Call(3) callee=print(args) args=[Lit(4)]
        Lit(4) kind=String value="Hello, Axiom!"
StructDef(5) name=Point fields=[x: Float, y: Float]
EnumDef(8) name=Shape variants=[Circle(Float), Rect(Float, Float), Empty]
```

### 2.3 Kind labels — single source of truth

Kind names come exclusively from a `HirKind::label()` method (mirroring the parser's
`SyntaxKind::label()`). A `test_no_hardcoded_kind_labels` guard scans the serializer's
own source for quoted kind labels and fails if it finds any.

---

## 3. The HIR data model

### 3.1 Design principles

The HIR is **not a lossless CST**. It is a desugared, ID-keyed tree where:

- **Trivia is gone.** Whitespace, comments, and formatting are the parser/printer's job.
- **Every node has a stable `HirId`.** This is the linker to later stages (type annotations
  will be on `HirId`s in M2).
- **Names are resolved or diagnosed.** Every identifier either resolves to a `DefId`
  (pointing at the definition's `HirId`) or produces an `UnresolvedName` diagnostic.
- **The tree owns its data.** `String` fields, not `&'a str` — no lifetime parameters on
  the HIR itself. (Per RUST_CONVENTIONS.md §4.4 — prefer owning data.)

### 3.2 Core types

```rust
/// A stable identifier for an HIR node, assigned in source order during lowering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HirId(pub usize);

/// A definition ID — the `HirId` of the item/binding/param where a name is defined.
/// Used by name resolution to link uses to definitions.
pub type DefId = HirId;

/// A calling convention on a parameter or argument.
/// Present from the start even though enforcement is deferred to v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallingConvention {
    Let,    // immutable borrow (default)
    Inout,  // mutable borrow
    Sink,   // consume (move/transfer ownership)
}

/// A name that resolved successfully, pointing at its definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedName {
    /// The definition this name resolves to.
    pub def_id: DefId,
    /// The text of the name as it appeared in the source (for diagnostics).
    pub text: String,
}

/// A name that did not resolve — the diagnostic is already emitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedName {
    pub text: String,
}

/// The result of name resolution: either resolved or unresolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameRef {
    Resolved(ResolvedName),
    Unresolved(UnresolvedName),
}
```

### 3.3 Items

```rust
pub enum Item {
    FnDef(FnDef),
    StructDef(StructDef),
    EnumDef(EnumDef),
    // v1+ items: TraitDef, ImplBlock, ModDef, UseDecl, ConstDef, ErrorSetDef
    // Drift-guarded: the lowerer must handle every AST item kind, but these
    // produce a "not yet supported" diagnostic during resolution.
}

pub struct FnDef {
    pub id: HirId,
    pub name: String,
    pub visibility: Visibility,
    pub params: Vec<Param>,
    pub return_type: Option<HirTy>,
    pub body: Block,
}

pub struct Param {
    pub id: HirId,
    pub convention: CallingConvention,
    pub name: String,
    pub ty: Option<HirTy>,
}

pub struct StructDef {
    pub id: HirId,
    pub name: String,
    pub visibility: Visibility,
    pub fields: Vec<FieldDef>,
}

pub struct FieldDef {
    pub id: HirId,
    pub name: String,
    pub ty: HirTy,
    pub visibility: Visibility,
}

pub struct EnumDef {
    pub id: HirId,
    pub name: String,
    pub visibility: Visibility,
    pub variants: Vec<VariantDef>,
}

pub struct VariantDef {
    pub id: HirId,
    pub name: String,
    pub payload: Vec<HirTy>,
}
```

### 3.4 Statements

```rust
pub enum Stmt {
    ValStmt(ValStmt),
    VarStmt(VarStmt),
    ExprStmt(ExprStmt),
    ReturnStmt(ReturnStmt),
}

pub struct ValStmt {
    pub id: HirId,
    pub pattern: Pattern,
    pub ty: Option<HirTy>,
    pub value: Expr,
}

pub struct VarStmt {
    pub id: HirId,
    pub pattern: Pattern,
    pub ty: Option<HirTy>,
    pub value: Expr,
}

pub struct ExprStmt {
    pub id: HirId,
    pub expr: Expr,
}

pub struct ReturnStmt {
    pub id: HirId,
    pub value: Option<Expr>,
}
```

### 3.5 Expressions

```rust
pub enum Expr {
    Lit(LitExpr),
    Path(PathExpr),
    Bin(BinExpr),
    Unary(UnaryExpr),
    Call(CallExpr),
    MethodCall(MethodCallExpr),
    Field(FieldExpr),
    Index(IndexExpr),
    Block(Block),
    If(IfExpr),
    Match(MatchExpr),
    Loop(LoopExpr),
    StructLit(StructLitExpr),
    Assign(AssignExpr),
    // v1+: Closure, Try, Cast, Catch, Scope, Spawn, Range, ListLit
    // Handled by the lowerer but produce a "not yet supported" diagnostic.
}
```

(Full struct definitions for each expression kind are in the crate source —
the pattern should be clear: each has an `id: HirId` plus relevant fields.
Note: `PathExpr` carries a `name_ref: NameRef`, `CallExpr` has `callee: NameRef`,
and `StructLitExpr` has `type_name: NameRef`.)

### 3.6 Patterns

```rust
pub enum Pattern {
    Wildcard(HirId),
    Ident(IdentPat),
    Literal(LitPat),
    TupleStruct(TupleStructPat),
    Struct(StructPat),
    Or(OrPat),
    Range(RangePat),
}
```

### 3.7 Types

```rust
pub enum HirTy {
    Named(NameRef),
    Unit,
    Tuple(Vec<HirTy>),
    Fn(FnTy),
    Error,
}
```

---

## 4. The lowering pipeline (CST/AST → HIR)

> **Spec reference:** The language-level name resolution rules live in
> DESIGN_SPEC.md §5.4. This section documents the implementation-level details
> (data structures, coverage invariants, diagnostic variants).

### 4.1 Two-pass resolution

**Pass 1 — Collect definitions:** Walk all top-level items and collect their names
into a symbol table. This is where duplicate-definition detection happens — two `fn`s
with the same name in the same scope produce a diagnostic.

**Pass 2 — Resolve bodies:** Walk expressions and statements, resolving every
`Ident`/`NameRef` against the symbol table and lexical scopes (block scoping,
`match`-arm bindings, `val`/`var`/`fn` params). Same-scope redefinition is an error.

### 4.2 Scoping rules (per DESIGN_SPEC.md §8)

- **Same-scope shadowing is disallowed.** `val x = 1; val x = 2` in the same block is an error.
- **Nested-scope shadowing is allowed.** A `val x` in an inner block can shadow an outer `x`.
- **Function parameters** form the innermost scope of a function body.
- **Match-arm bindings** are scoped to their arm's expression.
- **`val`** bindings are immutable; **`var`** bindings are mutable. Both are in scope
  for the rest of their enclosing block.

### 4.3 Resolution result

```rust
pub struct Hir {
    pub items: Vec<Item>,
    pub diagnostics: Vec<HirDiagnostic>,
}
```

Every `NameRef` in the HIR is either resolved (carries a `DefId` pointing at the
definition) or unresolved (produces a diagnostic). There is **no silently unresolved
name** — if resolution fails, an `UnresolvedName` diagnostic is emitted. The
`check_all` coverage function verifies this invariant at the HIR level.

---

## 5. Diagnostics

One `thiserror` enum for the HIR stage:

```rust
#[derive(Debug, Clone, thiserror::Error)]
pub enum HirDiagnostic {
    #[error("unresolved name: `{name}`")]
    UnresolvedName { name: String, span: Span },
    #[error("duplicate definition: `{name}`")]
    DuplicateDefinition { name: String, span: Span },
    #[error("{kind} `{name}` expects {expected} argument(s), found {found}")]
    ArityMismatch { kind: String, name: String, expected: usize, found: usize, span: Span },
    #[error("{feature} is not yet supported in this version")]
    NotYetSupported { feature: String, span: Span },
}
```

Rendering (`render(source)`) mirrors `ParseError::render`: line:col + message.

---

## 6. The drift guard (the load-bearing "nothing missed" proof)

The HIR lowerer must handle **every `SyntaxKind` node variant that `is_item()`,
`is_expr_kind()`, `is_pat()`, or `is_type_kind()` returns true for.** This is
mirrored directly from the parser's `test_ast_every_node_kind_covered`:

```rust
#[test]
fn test_lowerer_handles_every_ast_node_kind() {
    // Every non-Error SyntaxKind that the parser can produce for items, stmts,
    // exprs, patterns, or types must be handled by the lowerer. Adding a new
    // node kind to SyntaxKind without a corresponding HIR lowering path fails
    // this test.
    let kinds: Vec<SyntaxKind> = SyntaxKind::ALL
        .iter()
        .copied()
        .filter(|k| !k.is_trivia() && *k != SyntaxKind::Error)
        .filter(|k| {
            is_item_kind(*k) || is_stmt_kind(*k) || is_expr_kind(*k)
                || is_pat(*k) || is_type_kind(*k)
        })
        .collect();

    for kind in kinds {
        assert!(
            lowerer_handles(kind),
            "lowerer does not handle AST node kind: {:?}",
            kind
        );
    }
}
```

The `lowerer_handles` function is an exhaustive `match` that returns `true` for every
known kind and panics on an unrecognized one — adding a new kind to the parser without
updating the lowerer makes the test fail at compile time or runtime.

---

## 7. Test file layout

```
crates/axiom-hir/
  tests/
    golden.rs          # .ax → .hir snapshot tests (Layer 2)
    diagnostics.rs     # error .ax → .stderr diagnostic snapshots (Layer 4)
    invariants.rs      # drift guard + resolution completeness (Layer 3)
    fuzz.rs            # no-panic + termination + property fuzz (Layer 5)
    fixtures/
      hello.ax         # (symlink or copy from corpus/)
      arithmetic.ax
      structs_enums_match.ax
      functions.ax
      # … more as the corpus grows
      errors/
        unresolved_name.ax
        unresolved_call.ax
        duplicate_def.ax
        duplicate_binding.ax
        # … more error cases
```

Golden fixtures use `UPDATE_SNAPSHOTS=1 cargo test` to regenerate, matching the
established pattern. The HIR crate tests the same `.ax` files the parser and CLI use
(the corpus under `corpus/`), plus an internal `fixtures/` directory for HIR-specific
error cases.

---

## 8. The public API (what downstream crates consume)

```rust
/// Lower a parsed CST/AST + resolve names → HIR.
/// Returns the resolved tree and any diagnostics.
pub fn lower(root: &ast::SourceFile, source: &str) -> Hir;

/// Canonical serialization of the HIR (the test oracle and debug dump).
pub fn serialize(hir: &Hir) -> String;

/// Coverage checks: verifies that every `NameRef::Unresolved` in the HIR
/// has a corresponding `HirDiagnostic::UnresolvedName`. Returns `Ok(())`
/// if coverage is clean, or a list of coverage errors otherwise.
pub fn check_all(hir: &Hir) -> Result<(), Vec<CoverageError>>;

/// A coverage error — an unresolved name without a corresponding diagnostic.
pub enum CoverageError {
    UnresolvedWithoutDiagnostic { name: String, id: HirId },
}
```

The entry point takes an `ast::SourceFile` (the typed CST view) and the source string
(for span mapping), returns an `Hir` (always present — problems are in diagnostics,
never a panic), and never fails.

---

## 9. Commands

```bash
cargo test -p axiom-hir                            # full suite
cargo test -p axiom-hir --test fuzz                # fuzz/property tests only
UPDATE_SNAPSHOTS=1 cargo test -p axiom-hir         # regenerate .hir / .stderr
cargo run -p axiom-hir --example hir -- file.ax    # debug HIR dump
cargo run -p axiom-cli -- check file.ax             # CST + HIR dumps + diagnostics
```

---

## 10. When you change this crate

- Add an HIR node kind: add a variant to the relevant `enum`, add a `label()` arm,
  update `serialize`, add golden fixtures. The drift guard `test_lowerer_handles_every_ast_node_kind`
  will fail until the lowerer handles the new AST kind.
- Add a name-resolution rule: add a unit test in the relevant module, add a golden
  fixture that exercises it, update `serialize` if the output format changes.
- Add a new diagnostic kind: add a variant to `HirDiagnostic`, add a fixture
  `errors/*.ax` + checked-in `.stderr`, regenerate with `UPDATE_SNAPSHOTS=1`.
- Add a new AST node kind upstream (in `axiom-parser`): the drift guard in
  `axiom-hir` will fail until the lowerer handles it.