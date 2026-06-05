# `string::format` & Retiring the `print`/`println` Type-Checker Stand-in

> **Status:** ✅ Complete. Completes the I/O story begun in
> [`extern-buffers-and-path-unification.md`](extern-buffers-and-path-unification.md):
> `print`/`println` become genuinely the `stdlib/io.ax` functions everywhere — the
> type checker no longer carries a hand-written generic stand-in for them. The
> enabling piece is `string::format`, the one formatting primitive the spec
> mandates (§11, §15 item 7).
>
> **Companion docs:** [`io-design.md`](io-design.md) (the two-layer I/O architecture),
> [`DESIGN_SPEC.md`](../DESIGN_SPEC.md) §11 (stdlib surface; formatting), §15 item 7
> (interpolation-vs-format open question).

---

## 1. The problem

`crates/axiom-typeck/src/typeck/helpers.rs::builtin_fn` hands `print`/`println` a
**generic** signature — `fn<T>(T) -> Unit` — so any `print(x)` type-checks regardless of
`x`'s type. But the real `stdlib/io.ax` is **String-only**:

```axiom
pub fn print(s: String)   { write(1, s.as_bytes()) }
pub fn println(s: String) { write(1, s.as_bytes()); write(1, "\n".as_bytes()) }
```

Two layers hid the divergence:

1. **The stand-in is looser than the truth.** `builtin_fn` accepts any type; `io::print`
   accepts only `String`. So the test corpus is full of `print(anInt)` / `print(aFloat)`
   that *only* type-checks because of the stand-in.
2. **The single-file check path never sees `io::print`'s body.** `compile_source(source,
   global_exports)` resolves the *name* `print` (via the `io`-module export injection in
   HIR name resolution) but the `io.ax` `FnDef` is **not** in the compiled unit — so the
   type checker has no real signature and falls through to `builtin_fn`.

The spec is decisive (§11, §794): `print`/`println` are **String-only**; the idiomatic way
to print a number is `println(string::format("{}", n))`. The generic `print(Int)` is not
spec-faithful — it is an artifact of the stand-in.

So **retiring the stand-in requires two things**, not one:
- the type checker must obtain `print`/`println`'s *real* `String` signature in every path
  (a **prelude signature** injection), and
- the ~15 call sites that currently do `print(intOrFloat)` need a spec-faithful way to
  render a non-string — i.e. **`string::format`** (deferred until now, §11/§15 item 7).

The prelude is necessary but **not sufficient**; `string::format` is the long pole.

## 2. `string::format` — the one variadic primitive

Axiom has no varargs syntax, and the spec forbids adding it (singular idiom). `format` is
therefore a **compiler intrinsic**, recognised by name — the *only* magic call in the
language, and the one §11 explicitly sanctions ("**one** formatting mechanism"). This
mirrors Oxy's proven shape (typechecker special-case + a runtime template engine).

**Surface.** `string::format("{} = {}", k, v) -> String`. HIR call-lowering already reduces
a `::`-qualified callee to its **last path segment** (`lower_call_expr` →
`path_last_segment_from_node`), so `string::format(...)` lowers to a bare call to `format`
— no path-call machinery is needed. `format` is added to HIR `builtin_def_id` so the name
resolves (like `todo`) and reaches the type checker instead of erroring as unresolved.

**Type checking.** `format` is special-cased in `infer_call`: every argument is inferred
(any type is accepted — the template decides rendering), arity is *not* checked, and the
call's type is `String`. It is **not** in `builtin_fn` and carries no `FnTy` (it is genuinely
variadic, which `FnTy` cannot express). A user-defined `format` shadows the intrinsic
(checked via `env.lookup` first).

**IR + VM.** The bare `format` call lowers to an `IrInstr::Call { function: "format", .. }`.
The VM dispatches it through `is_builtin`/`call_builtin` (args already arrive as a
`Vec<Value>`, so variadic is natural). `builtin_format` parses the template — `{}` (Display),
`{:?}` (Debug-ish), `{{`/`}}` escapes — and renders each argument through the existing
`Value: Display` impl (`Int`/`Float`/`Bool`/`String`/… already covered).

## 3. Prelude signature injection (retiring the stand-in)

The type checker seeds its global environment with the **real** `print`/`println`
signatures, sourced from the bundled `stdlib/io.ax`, *before* checking user code:

- At `collect_pass`, parse + structurally lower the bundled prelude source and run the
  existing signature-collection (`collect_fn_sigs`, factored into a reusable
  `register_fn_sig`) over its `FnDef`s, defining each name in `env` **only if absent**.
- These signatures are added to the *environment only* — **not** to `hir.items` — so THIR
  dumps stay focused on user code (no stdlib noise in goldens).
- Call resolution is by **name** (`resolve_callee` → `env.lookup("print")`), so the
  prelude `FnDef`'s synthetic `HirId` need not match the export `DefId`; the binding just
  needs the right `FnTy`.
- In the `with_stdlib` (source-concatenation) path the real `io.ax` `FnDef`s are already in
  `hir.items`, so `collect_fn_sigs` registers them and the prelude inject is a harmless
  no-op (define-if-absent).

With real signatures present in every path, `builtin_fn`'s `print`/`println` become dead
code and are deleted. `todo` stays (it is a legitimate compiler stub, not a stdlib stand-in).

## 4. Call-site fallout (the ~15 sites)

Every type-checked call that passes a non-`String` to `print` is rewritten to the
spec-faithful form, preserving newline semantics (`print` stays `print`):

```axiom
print(x)            →  print(string::format("{}", x))
```

Sites that already pass a `String` (`print(name)` where `name: String`) are **unchanged**.
Affected: `corpus/valid/{methods,functions,assignments,bindings,match,match_patterns}.ax`,
the inline sources in `crates/axiom-typeck/tests/golden.rs`, and the executed
`crates/axiom-{vm,ir}/tests/fixtures/*.ax`. Lexer/parser/HIR fixtures that print a `String`
need no change; those are not type-checked.

The VM golden harness (`crates/axiom-vm/tests/golden.rs`) currently **ignores** type
diagnostics. It is changed to **assert clean** — surfacing exactly this class of bug going
forward. All affected `.thir` and `.trace` goldens are regenerated (`UPDATE_SNAPSHOTS=1`).

## 5. Work plan (each step ≈ one commit; TDD; gate must pass)

1. ✅ **docs + spec** — this file; `DESIGN_SPEC.md` §11/§15 record `format`-as-intrinsic and
   `print` String-only.
2. ✅ **`string::format` intrinsic, end-to-end** — HIR `builtin_def_id += "format"`; typeck
   `infer_call` variadic special-case → `String`; IR lowers the `format` call; VM
   `builtin_format` + template engine; tests. `builtin_fn` unchanged (no breakage yet).
3. ✅ **Prelude injection + call-site rewrite** — seed `print`/`println` real sigs from
   `io.ax` (`collect.rs::inject_prelude_sigs`); rewrote all non-`String` `print` sites;
   regenerated `.thir`/`.trace` goldens; VM harness asserts clean.
4. ✅ **Delete `builtin_fn` `print`/`println`** (dead after step 3; only `todo` remains);
   updated `io-design.md` and the companion plan docs.

### Bugs surfaced en route (pre-existing, masked by the lenient VM harness)

Tightening the VM golden harness to assert clean (step 3) exposed two latent bugs that the
ignored-diagnostics path had hidden, both now fixed in the same step:

- **`let` used for locals.** Several VM fixtures wrote `let x = …` for a *local* binding, but
  `let` is a parameter borrow-convention (§4.2) — locals are `val`/`var`. The misuse left the
  name unresolved. Fixtures corrected to `val`.
- **Qualified enum patterns didn't bind.** `match s { Shape::Circle(r) => … }` failed to bind
  `r` because `define_pattern_tuple_struct` compared the variant against the *full* path text
  (`Shape::Circle`) rather than the last segment (`Circle`). Now matched on the last segment,
  so qualified and bare variant patterns behave identically.

## 7. Status: complete

All four steps landed; `fmt` + `clippy -D warnings` + the full suite pass. The deferred items
in §6 (user-type `Display` dispatch, interpolation, format specs, native `format`) remain
tracked there.

## 6. Out of scope (deferred, with reason)

| Item | Why deferred |
|---|---|
| `Display`/`Debug` *traits* for user types (§11) | `format` renders built-ins via the VM `Value` match today; user-type `fmt` dispatch waits on the trait-object story. |
| String **interpolation** (§15 item 7) | Singular idiom forbids both interpolation and `format`; `format` is chosen. Interpolation stays rejected. |
| Width/precision/alignment format specs (`{:>8.2}`) | Only `{}` / `{:?}` are needed now; extend the template engine when a real need appears. |
| Native-backend `format` | Same VM-callback→real-FFI path as the rest of `core::platform` (io-design.md). |
