# Design Audit: M1/M2 vs DESIGN_SPEC.md

> Audited 2026-06-04. Cross-references DESIGN_SPEC.md, hir-testing.md,
> typeck-testing.md, and the axiom-hir/axiom-typeck implementations.

## How to use

Each item below has a checkbox. After fixing, commit and check the box.

---

## MUST-FIX (design contradictions or missing specs)

### 1. §7.1 `break value` contradicts loop typing rules
- [x] **Status:** DONE — implemented in v0

§7.1 says "`break value` makes loop an expression." The type checker now infers
loop types from break values via a `loop_break_types` stack. `break`/`continue`
are properly lowered from AST → HIR → type checker. Loop type rules:
no break-with-value → `Unit`; all breaks yield the same type → that type;
mismatched break types → `BreakTypeMismatch` diagnostic. Spec updated with
`[Decided — v0]` tag.

---

### 2. §7.2 Guards × exhaustiveness — underspecified
- [x] **Status:** DONE

§7.2 lists guards (`if cond` on a match arm) as a pattern feature alongside
exhaustiveness enforcement. But it doesn't say how guards interact with
exhaustiveness. A guarded arm `A(x) if x > 0` doesn't cover all `A` values —
guard conditions can't be checked statically. The exhaustiveness checker
(`exhaustiveness.rs`) ignores guards entirely.

**Resolution:** Added "Guards × exhaustiveness" bullet to §7.2: guarded arms
do not contribute to exhaustiveness. Updated `exhaustiveness.rs` to skip arms
with guards in the coverage loop. Added 4 unit tests + 1 diagnostic snapshot
fixture for the guard case.

---

### 3. §3.2 Built-in collections not reflected in type system
- [ ] **Status:** OPEN

§3.2 specifies `List<T>`, `Map<K,V>`, `Set<T>`, `Option<T>`, `Result<T,E>` as
built-in types. But the type system (`Ty` enum) has no `List`, `Map`, `Set`,
`Option`, or `Result` constructors. There's no way to express `List<Int>` in the
current type universe.

**Resolution needed:** Tag these as `[Deferred — v1]` in §3.2, or add a
`Ty::Builtin(name, args)` form to the M3 scope.

---

## SHOULD-FIX (inconsistencies that will cause confusion)

### 4. Unit type representation inconsistency
- [ ] **Status:** OPEN

| Context | Display |
|---|---|
| `Ty::Unit` in diagnostics (types.rs:61) | `"Unit"` |
| `HirTy::Unit` in canonical dump (serialize.rs:565) | `"()"` |
| `LitKind::Unit` in canonical dump (serialize.rs:591) | `"Unit"` |
| `Ty::Tuple(vec![])` in diagnostics (types.rs:67) | `"()"` |

§3.2 says "Unit: `()`". The spec should pick one canonical name for diagnostics
and use `()` only in source syntax (like Rust).

**Resolution needed:** Decide: diagnostics say `Unit` or `()`? Update one side.

---

### 5. `LoopBodyNotUnit` diagnostic defined but never emitted
- [ ] **Status:** OPEN

`TypeDiagnostic::LoopBodyNotUnit` exists in error.rs. `infer_loop()` never emits
it — it forces `Ty::Unit` without checking the body's type. This means
`loop { 42 }` silently returns Unit with no diagnostic.

**Resolution needed:** Either emit it (if loop bodies must be Unit in v0) or
remove it (if `break value` is coming in v1).

---

### 6. §8.1 "Return types required except when `()`" — not explicit
- [ ] **Status:** OPEN

§8.1 says return types are required except for Unit. The parser allows
`fn foo() { ... }` without a return type annotation. The HIR lowerer treats
missing return type as `None` (defaults to Unit). This works but §8.1 should
say `[Decided — parser defaults to Unit]` rather than leaving it implicit.

**Resolution needed:** Add clarification to §8.1.

---

## NICE-TO-HAVE (fine to defer)

### 7. `Byte` type missing from type checker
- [ ] **Status:** DEFERRED

§2.4 specifies `Byte` (8-bit unsigned) as a primitive. `Ty` has no `Byte`
variant. Tag as v1 or add to M3 scope.

---

### 8. §3.5–3.6 Traits and generics need version tags
- [ ] **Status:** DEFERRED

§3.5 (traits) and §3.6 (generics) are `[Decided]` but clearly v1. Neither
section says `[Decided — v1]` explicitly. hir-testing.md correctly lists
`TraitDef` as v1+ but the spec itself doesn't.

**Resolution needed:** Add `[v1]` tags to §3.5 and §3.6 headers.

---

### 9. Name resolution scope rules underspecified in DESIGN_SPEC
- [x] **Status:** DONE

Scoping rules were only in hir-testing.md (§4.2), not in the design spec itself.

**Resolution:** Added §5.4 Name Resolution [Decided] to DESIGN_SPEC.md. Covers
two-pass resolution (collect definitions, then resolve bodies), lexical scoping
rules (same-scope shadowing disallowed, nested allowed, function params, match-arm
bindings, val/var scope, module-level forward references), resolution guarantee
(every NameRef resolved or diagnosed), and qualified path syntax (`.`, `::`, `use`).
hir-testing.md §4 now serves as the implementation-level detail while §5.4 is the
language-level spec.

---

## Not design issues (confirmed correct)

- Method calls, index, iterator loops: correctly `NotYetSupported` in v0
- Span tracking: correctly TODO(v1)
- Closures: correctly gated behind Spike 0 (passed)
- Ownership/Perceus pipeline: correctly deferred to v1
- No generics/traits in type checker: correct for v0
