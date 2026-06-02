# Axiom Compiler — Rust Coding Conventions

> **The one rule above all others:** *Write the Rust a competent programmer who is **not** a Rust expert can read and understand.* Rust is a huge language with many ways to do everything. This document pins a **small, deliberate subset** so the Axiom compiler codebase stays consistent and approachable. When in doubt, choose the simpler, more obvious construct — even if a "cleverer" one exists.
>
> This is the singular-idiom philosophy (the thing Axiom enforces on its users) applied to **our own implementation language**.
>
> These conventions are derived from the patterns proven in the Oxy codebase, but **filtered for readability** — where Oxy uses an expert-level pattern, we either avoid it or quarantine it (see §3, §9).

---

## 0. How to use this document
- This is the source of truth for *how we write Rust* in the Axiom repo.
- It is referenced from `CLAUDE.md` so it applies to every change, by every contributor (human or AI).
- If you hit a case this doc doesn't cover, pick the option a non-expert would find clearest, and **add a rule here** in the same change.
- Disagreement with a rule is fine — but change the rule by discussion, don't quietly violate it.

---

## 1. Guiding Principles (in priority order)

1. **Readable by a non-expert.** Favor explicit, boring code. A reader should not need to know lifetimes, `Pin`, `async` internals, macro hygiene, or trait resolution subtleties to follow a function.
2. **One obvious way.** Don't introduce a second pattern for something that already has a pattern here. Consistency beats local cleverness.
3. **Explicit over implicit.** Prefer code that says what it does over code that's terse. `.to_string()` over `.into()` when the target isn't obvious; named locals over deeply nested expressions.
4. **Shallow over clever.** Avoid deep generic abstractions, trait gymnastics, and macro magic. A little repetition is better than an abstraction that takes 20 minutes to understand.
5. **Isolate the hard parts.** Where genuinely advanced Rust is unavoidable (codegen, FFI, `unsafe`), confine it to a few clearly-marked modules behind safe, simple APIs — so the other 90% of the codebase stays beginner-readable.

---

## 2. The Rust Subset — What We Use

### ✅ Use freely (the everyday toolkit)
- `struct`, `enum`, and `match` (exhaustive). This is the backbone — see §4.
- `Result<T, E>` and the `?` operator for error propagation.
- `Option<T>` with `match`, `if let`, `unwrap_or`, `map`, `ok_or_else`.
- `Vec<T>`, `HashMap<K, V>`, `BTreeMap<K, V>`, `String`, `&str` — standard collections, used directly.
- Plain functions and methods (`impl Block`).
- Simple iterator methods: `.iter()`, `.map()`, `.filter()`, `.collect()`, `.enumerate()` — **one or two adapters per chain** (see §7).
- `Box<T>` for recursive types (AST/IR nodes).
- Derives: `#[derive(Debug, Clone, PartialEq)]` and friends (see §4.3).
- `///` doc comments and `//!` module docs.

### ⚠️ Use sparingly, with a clear reason
- **Generics with trait bounds** (`fn f<T: Ord>(...)`). Use only when 2+ concrete types genuinely share code. Prefer a concrete type or an `enum` first. Never more than one or two type params.
- **Traits you define.** Fine for a small, named abstraction (e.g. one `Backend` trait with two impls). Avoid trait hierarchies, blanket impls, and "trait for the sake of it." Default to concrete types + `match`-based dispatch (Oxy's proven pattern).
- **Closures stored in structs / returned** (`Box<dyn Fn(...)>`). Allowed at backend boundaries, but a plain function pointer or an `enum` of cases is usually clearer.
- **Lifetime *parameters* in signatures** (`fn f<'a>(...)`, `struct S<'a>`). A *single* named lifetime is fine when it's natural — e.g. a parser or visitor that borrows the source/AST it walks. What we avoid is **multiple lifetimes, lifetime bounds (`'a: 'b`), and lifetimes added purely to dodge a cheap clone**. If a `.clone()` of a small value removes a lifetime and the code isn't in a hot loop, prefer the clone. If you're in a hot pass (see §5), borrowing is the right call.

### 🚫 Avoid (needs explicit sign-off + a comment explaining why)
- **`Rc<RefCell<T>>` / `Arc<Mutex<T>>` for shared mutable state.** This is the single biggest readability trap. Prefer ownership + passing values, or indices into a `Vec` (an "arena"), over interior mutability. *(Oxy uses `Rc<RefCell>` in its runtime `Value`; we deliberately do not adopt that pattern in normal code.)*
- **`unsafe`.** Allowed **only** in the codegen/FFI quarantine modules (§9). Never in the lexer, parser, typechecker, or ordinary logic.
- **Macros (`macro_rules!`, proc-macros).** Do not write custom macros for control flow or codegen. (Using derive macros from `std`/`thiserror` is fine — that's not writing a macro.)
- **`async`/`await` in the compiler.** The compiler is synchronous. (Axiom *the language* is colorless/green-threaded; the *compiler implementing it* is plain synchronous Rust.)
- **Advanced trait features:** associated types (except where a single backend trait truly needs one), GATs, trait objects with multiple bounds, `impl Trait` in complex positions, operator-overloading traits on our own types.
- **`unwrap()` / `expect()` / `panic!` on user-reachable paths.** See §3.4.
- **Deref coercion tricks, `Cow`, `Pin`, lifetime-heavy zero-copy parsing.** If you think you need these, ask first — there's usually a simpler design.

---

## 3. Error Handling

### 3.1 One error type per stage, built with `thiserror` [from Oxy — keep]
- Use the `thiserror` crate for all error enums. Derive `#[derive(Debug, Clone, thiserror::Error)]`.
- A single top-level pipeline error enum (e.g. `CompileError`) with variants per stage (`Lex`, `Parse`, `Type`, `Ownership`, ...), each carrying a message + source span.
```rust
#[derive(Debug, Clone, thiserror::Error)]
pub enum CompileError {
    #[error("lex error at {line}:{col}: {msg}")]
    Lex { line: usize, col: usize, msg: String },
    #[error("parse error at {line}:{col}: {msg}")]
    Parse { line: usize, col: usize, msg: String },
    // ...
}
```

### 3.2 Propagate with `?` [keep]
- Fallible functions return `Result<T, CompileError>`. Propagate with `?`. Do not hand-roll match-and-return when `?` works.

### 3.3 Constructing errors
- Provide small constructor helpers (`CompileError::parse(span, msg)`) rather than building the struct literal at every call site. (Oxy does this with `runtime_error()`, `check_arg_count()`, etc. — follow it.)
- One way to build a given error. Don't mix `.ok_or_else(...)` and direct construction for the same situation in different files — pick the helper.

### 3.4 No panics on user-reachable paths [keep, enforce]
- **Never** `unwrap()`/`expect()`/`panic!` on anything a user's `.ax` program can trigger. User errors are always `Result`.
- `unwrap()`/`expect()` are allowed **only** for *internal compiler invariants that are bugs if violated* — and then use `.expect("clear message describing the invariant")`, never bare `.unwrap()`.
- A bare `panic!` is allowed only for "this enum arm is structurally impossible" assertions, with a comment. Prefer `unreachable!("why")`.

---

## 4. Types & Data

### 4.1 Enums + `match` are the backbone [from Oxy — keep]
- Model the AST, IR, types, and values as `enum`s; process them with **exhaustive `match`** (no `_ =>` wildcard unless genuinely catch-all). This is what makes "add a variant → compiler shows you every site to update" work — the same property Axiom gives its users.
- Keep enum variants flat. Use `Box<T>` for recursion (`BinaryOp { left: Box<Expr>, right: Box<Expr> }`), not nested enums.

### 4.2 Structs for plain data
- Public data structs with named fields. No builder patterns except where there are many optional fields (and even then, prefer a plain struct with an `::new()` + defaults).
- Newtypes (`struct NodeId(usize)`) are encouraged for IDs/indices — they're cheap, clear, and prevent mixing up `usize`s.

### 4.3 Standard derive sets [keep]
- **Normal data (AST, IR, config):** `#[derive(Debug, Clone, PartialEq)]` (add `Eq, Hash` if used as a map key; `Copy` only for small all-`Copy` types like `Span`/IDs).
- **Avoid custom `Clone`/`PartialEq` impls.** (Oxy writes a manual `Clone` for `Value` for performance; that's an expert pattern — we don't do it unless profiling forces it, and then it gets a comment.)

### 4.4 Prefer owning data
- Structs should generally own their data (`String`, `Vec<T>`), not borrow it (`&'a str`, `&'a [T]`). Owning structs have no lifetime params and are dramatically easier to read and move around. Clone when needed; optimize only if profiling says so.

---

## 5. Ownership & Borrowing (in OUR Rust, not Axiom)
> We're building an ownership language, but the *compiler* should use ownership in the simplest way that's still sensible. **Borrowing is normal Rust — we use it freely. We only avoid *complex* lifetime machinery.**

The balance, in one line: **borrow by default for reading; clone when it removes a confusing lifetime and you're not in a hot path; reach for `Rc`/arena only when a profiler points at a specific pass.**

- **Borrow for parameters — this is the default, not an exception.**
  - `&str` for string params, `String` for owned returns [from Oxy — keep]. `fn keyword(name: &str) -> bool`.
  - `&[T]` for read-only slice params; `&T` to read; `&mut T` for genuine in-place mutation (keep its scope short).
  - These plain references need **no lifetime annotations** (Rust elides them) — use them everywhere, they're free and clear.
- **Clone when it buys clarity, not as a blanket rule.** A `.clone()` of a small value that removes a cryptic lifetime is good. Cloning a large AST subtree inside a hot pass is not — borrow there. Use judgment: *is this code run often, on big data?*
- **A single named lifetime is allowed** when borrowing is the natural design (a parser holding `&str` source, a visitor borrowing the AST). Don't contort the code to avoid it.
- **What we actually avoid:** multiple lifetime params, lifetime bounds (`'a: 'b`), `for<'a>` HRTBs, and self-referential structs. If you find yourself fighting the borrow checker with these, that's the signal to clone or restructure (arena + indices) instead.
- **Hot-pass exception:** in the few performance-sensitive tree walks (typecheck, IR-gen), prefer borrowing / `Rc` for shared-immutable nodes over cloning. Optimize these *after* profiling, not preemptively.

---

## 6. Traits & Generics
- **Concrete types first.** Reach for a trait or generic only when you have 2+ real implementations/types that share code.
- **Dispatch via `match` on an enum**, not via trait objects, wherever the set of cases is known and closed (it almost always is in a compiler — see Oxy's builtins dispatch). This is more readable for non-experts than `dyn Trait`.
- **One backend trait is fine** (e.g. `trait Backend { fn emit(...); }` with `CraneliftBackend` + `InterpBackend`) — that's a real, named, two-impl abstraction. Don't grow it into a trait family.
- **No blanket impls, no marker traits, no trait-bound chains** (`T: A + B + C`). One bound, maybe two.

---

## 7. Iterators vs Loops
> Oxy mixes both freely; we tighten this for readability.

- **Short chains are good:** up to ~2 adapters (`xs.iter().map(f).collect()`). Clear and idiomatic.
- **Use a plain `for` loop when:** there's mutable accumulation, early `break`/`continue`, side effects, or the chain would exceed ~2 adapters or wrap lines.
- **Never** build a 4+ adapter chain with closures spanning multiple lines — a `for` loop is more readable. If you can't read the chain aloud in one breath, make it a loop.
- Prefer `for x in &xs` over `xs.iter().for_each(|x| ...)` for side-effecting iteration.

---

## 8. Functions
- **Small and single-purpose.** If a function exceeds ~50 lines or has 3+ levels of nesting, consider splitting. (Match arms with long bodies → extract a helper.) **Mechanically enforced** (proxies): `clippy::too_many_lines` (≤60), `clippy::too_many_arguments` (≤5), `clippy::cognitive_complexity` — all fail the build via `-D warnings`. These catch fat methods, long arg lists, and tangled flow; they can't *prove* one-task, so semantic single-responsibility stays a review concern. Tune thresholds in `clippy.toml`, never silence per-function.
- **Name things.** Bind intermediate results to named `let`s instead of nesting calls. `let trimmed = line.trim(); let parsed = parse(trimmed)?;` over `parse(line.trim())?` when it aids reading.
- **No function overloading** (Rust doesn't have it anyway) and **no default-argument emulation** via `Option` soup — make a second named function or a small config struct.
- Free functions for stateless logic; methods (`impl`) for behavior tied to a type.

---

## 9. `unsafe` and Codegen (the quarantine) [from Oxy — keep + tighten]
- **`unsafe` lives only in designated modules** — codegen (`jit/`) and FFI. The lexer, parser, resolver, typechecker, ownership pass, and IR-gen contain **zero `unsafe`**.
- **Every `unsafe` block carries a `// Safety: ...` comment** explaining the invariant that makes it sound. No exceptions. (Oxy does this consistently — match it.)
- **Wrap unsafe behind safe APIs.** Callers outside the quarantine module never see raw pointers. Provide safe helpers (`push`/`pop`/`move_value`) and keep the unsafe inside them.
- **Encode unsafe invariants once.** If the same unsafe pattern appears twice (e.g. moving a value between slots), make a single safe helper so the invariant can't be forgotten at the next call site. (This is Oxy's hard-won `move_value` lesson — repeated unsafe is how double-frees happen.)
- A reader should be able to understand the entire compiler *except* the quarantine modules without ever encountering `unsafe`.

---

## 10. Modules & Files
- **One responsibility per file.** Target 150–400 lines; split past ~500. (Matches Oxy.) **Mechanically enforced:** `scripts/check.sh` fails the build on any `.rs` over 600 lines (the "~500" + headroom, mirroring §8's ~50→60). Split into a folder + `mod.rs`; a known pre-existing file may be grandfathered in the script *with a reason* and remains a tracked debt — never silence the gate.
- **Module layout:** use a folder + `mod.rs` when a module has submodules; a single named file otherwise. `lexer/mod.rs` + `lexer/token.rs`; standalone `errors.rs`.
- **Re-export the public API in `lib.rs`** with explicit `pub use` (no glob re-exports). Everything private by default; `pub` only what's needed across module boundaries.
- **Naming:** modules `snake_case`, types `PascalCase`, functions/vars `snake_case`, consts `SCREAMING_SNAKE_CASE`. (rustfmt + clippy won't fix names — reviewers must.)

---

## 11. Comments & Documentation
- **Module doc (`//!`) at the top of every file** — one or two sentences on what it's for. [from Oxy — keep]
- **`///` doc comments on public items** — what it does, not how. Keep them short.
- **Inline `//` comments only for the non-obvious** — invariants, "why", gotchas. Don't narrate obvious code.
- **Per-folder `README.md`** documenting the folder's purpose, a file→responsibility table, key entry points, and invariants. **Update it in the same change when you add/rename/move a file.** [from Oxy — keep; it's load-bearing]

---

## 12. Testing
- **Unit tests inline** in a `#[cfg(test)] mod tests { ... }` at the bottom of the file, or in a sibling `tests.rs` for large suites. [from Oxy — keep]
- **Test naming:** `test_<what>_<scenario>` (`test_parse_let_with_type`).
- **Extract test helpers** for boilerplate (e.g. `parse_expr("...")`).
- **Language-level tests** live as `.ax` files with `#[test]` / `#[compile_error]` functions, globbed by an integration test (Oxy's model — carry it over for Axiom).
- Prefer many small focused tests over one big one. A failing test name should tell you what broke.

---

## 13. Tooling & Config [match Oxy, enforce in CI]
- **`rustfmt.toml`:** `max_width = 100`, `use_field_init_shorthand = true`. Formatting is non-negotiable; `cargo fmt --all` before every commit.
- **Clippy:** `cargo clippy --all-targets -- -D warnings` must pass. **Warnings are errors.**
  - Crate-level `#![allow(...)]` is permitted **only** with a one-line comment justifying each allow (Oxy's `mutable_key_type`, `type_complexity` are examples). Don't sprinkle `#[allow]` on individual items to dodge a lint — fix the code.
- **Edition 2021** (or latest stable edition), pinned `rust-version` (MSRV).
- **Workspace** with `workspace.dependencies` for centralized version management.
- **Pre-commit gate:** `cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test`.

---

## 14. Quick Anti-Pattern Reference

| Don't | Do instead |
|---|---|
| `Rc<RefCell<T>>` for shared mutable state | Own the data; pass it; or use indices into a `Vec` (arena) |
| `unsafe` outside codegen/FFI | Keep it in the quarantine; wrap behind safe APIs |
| Custom `macro_rules!` | A plain function, or an `enum` + `match` |
| 4+ chained iterator adapters | A `for` loop |
| *Multiple* lifetimes / `'a: 'b` bounds / self-referential structs | One named lifetime at most, or `.clone()`, or arena + indices |
| Cloning a large AST subtree inside a hot pass | Borrow it (`&`), or `Rc` for shared-immutable nodes |
| `dyn Trait` for a closed set of cases | `enum` + exhaustive `match` |
| `.unwrap()` on user-reachable data | `?` and `Result`; `.expect("invariant")` only for true internal bugs |
| Trait bound chains `T: A + B + C` | Concrete type, or split the logic |
| `.into()` where the target type isn't obvious | `.to_string()` / explicit conversion |
| Clever one-liner spanning the screen | Named intermediate `let`s |
| Custom `Clone`/`PartialEq` impl | Derive it; optimize only if profiling demands |

---

*This document evolves. When a new pattern question comes up, decide it the simplest way, write it down here, and reference the decision. The goal never changes: **a programmer who isn't a Rust expert should be able to read this codebase and understand it.***
