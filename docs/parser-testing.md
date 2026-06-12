# Parser Testing & Architecture Spec

> **Status:** authoritative for the parser's tree model, test/debug tooling, and architecture. Binding before the parser is written.
> **Decisions baked in:** **lossless CST** (rust-analyzer–shaped green/red tree, hand-rolled — no `rowan`), hand-rolled snapshots (no `insta`), **total + recovering** parsing (never panics, emits error nodes, reports diagnostics in a result list).
> **Companion docs:** [`lexer-testing.md`](lexer-testing.md) (the layer below — the same philosophy one level down), [`RUST_CONVENTIONS.md`](../RUST_CONVENTIONS.md), [`ENFORCEMENT.md`](../ENFORCEMENT.md).

## 0. The concern this answers

The lexer proved "nothing is missed" with two mechanical guarantees: a *canonical dump* that is both debug tool and test oracle, and *coverage invariants* (`tiles` + `reconstruct`) that hold on every input including random ones. The parser keeps **exactly those two ideas, one level up**:

- The canonical dump becomes a **tree dump** (`SyntaxNode → String`).
- `reconstruct == source` survives intact, because the tree is a **lossless CST**: every token — trivia included — is a leaf, so concatenating the leaves rebuilds the source byte-for-byte. This is the single most powerful check in the suite, and it is the reason we chose a CST over a plain AST.

The new fear the parser introduces, and how we kill it: **a parser can loop forever or panic on malformed input.** We forbid both mechanically — the parser is *total* (every input yields a tree + a diagnostics list, never a panic or hang), proven by a fuzzer that asserts no-panic + termination + round-trip on tens of thousands of random inputs.

---

## 1. Why a lossless CST (the decision)

A plain AST throws away trivia and exact spans; you cannot rebuild the source from it, so you lose the round-trip invariant *and* you cannot build a faithful formatter later. A **lossless CST** keeps every token as a tree leaf. We pay for it with one extra layer of machinery (an untyped tree + a typed view on top); we get back:

1. **The round-trip coverage invariant** (`reconstruct(tree) == source`) — the parser's load-bearing "nothing dropped" proof.
2. **The formatter and a precise LSP fall out for free** — both need trivia and exact spans, which the CST already holds.
3. **Error resilience** — error nodes live in the same tree as everything else, so a malformed region is *localized*, not fatal.

The tree type is wired into every downstream stage (type checker, lowering, every test), so this is a decide-once choice. It is decided: **lossless CST.**

---

## 2. The tree model (rust-analyzer–shaped, hand-rolled)

Three layers, smallest to largest:

### 2.1 `SyntaxKind` — one flat enum, the single source of truth
A single enum naming **both** token kinds and node kinds (`WHITESPACE`, `IDENT`, `KW_LET`, `FN_DEF`, `BIN_EXPR`, `ERROR`, …). Every leaf carries a token `SyntaxKind`; every interior node carries a node `SyntaxKind`. As in the lexer's `symbols.rs`, the kind → display-label match is **exhaustive**: adding a variant without a label fails to compile. The lexer's `TokenKind` maps into the token half of `SyntaxKind` through **one** conversion function — the only bridge between the crates, so the two enums cannot silently drift.

### 2.2 Green tree — immutable, position-independent
`GreenNode { kind, text_len, children }` and `GreenToken { kind, text }`, shared via `Rc`. Green nodes know their *length*, never their *offset* — they are position-independent so identical subtrees can be shared. Built bottom-up by the builder; never mutated after construction.

### 2.3 Red tree — lazy, absolute positions
`SyntaxNode` / `SyntaxToken` wrap a green node plus its **absolute offset** and a parent pointer, computed lazily on traversal. This is the layer the snapshot serializer, the invariants, and the AST views walk. Offsets are *derived* from green lengths — byte offset stays the single positional truth, exactly as in the lexer.

### 2.4 AST view — typed, zero-copy
Thin typed wrappers (`ast::FnDef`, `ast::LetStmt`, `ast::BinExpr`) over `SyntaxNode`, each a `cast(SyntaxNode) -> Option<Self>` + accessor methods that find children by kind. No data of their own — they are a *lens* over the red tree. The compiler consumes this view and never sees trivia; the formatter consumes the raw red tree and sees everything.

---

## 3. Losslessness & trivia attachment (the invariant's foundation)

The builder consumes the **full** lexer token stream, trivia included. The parser makes decisions over the **significant-token view** (trivia filtered — reusing the lexer's `is_trivia`, the one sanctioned filter from `lexer-testing.md` §3), but every trivia token is still placed into the tree as a leaf.

**Attachment rule (deterministic, documented, load-bearing):** trivia attaches as **leading trivia of the significant token that follows it**; the owning node is whichever node is open at the moment that token is consumed. Trailing trivia after the final significant token attaches to the root. This rule is simple, total, and — crucially — makes `reconstruct` hold by construction: every byte of source lands in exactly one leaf, in source order.

> We deliberately do **not** copy rust-analyzer's leading/trailing heuristic (`n_attached_trivia`) in v1. It produces prettier comment attachment but is not needed for correctness or round-trip; revisit when the formatter's comment placement demands it.

`EOF` is **not** a tree leaf (it is zero-width and has no source bytes). Reconstruction concatenates leaf token texts only.

---

## 4. The coverage invariants (DRY — defined once, used everywhere)

Pure helpers in the parser crate, called by golden tests, unit tests, *and* the fuzzer — never re-implemented:

### 4.1 `reconstruct(tree) -> String`
Concatenate every leaf token's text in left-to-right order. **Invariant: `reconstruct(tree) == source`, byte for byte, on every input.** Inherited from the lexer one level up; the CST is what keeps it alive.

### 4.2 `spans_well_formed(tree) -> Result<(), SpanError>`
Walks the red tree asserting structural sanity:
- every child's span is **contained** in its parent's span (`parent.lo <= child.lo && child.hi <= parent.hi`);
- siblings are **contiguous and ordered** (`child[i].hi == child[i+1].lo`) — no gaps, no overlap;
- a node's span equals the union of its children's spans;
- the root span is `0..source.len()`.

`reconstruct` proves no byte was dropped; `spans_well_formed` proves the *tree shape* is sound. Together they are the parser's analogue of the lexer's `tiles`.

### 4.3 `every_token_present(tree, tokens) -> Result<(), CoverageError>`
The lexer's non-trivia tokens, in order, must equal the tree's non-trivia leaves, in order. Proves the parser **consumed every significant token** — none silently dropped during recovery, none duplicated.

### 4.4 `check_all(tree, source, tokens)`
Runs all three. This is what the fuzzer and every golden test call.

---

## 5. Total + recovering parsing (no panic, no hang)

- **Total.** `parse(source) -> ParseResult { tree, errors }`. Every input produces a tree. Problems are `errors`, never a panic, `unwrap`, or failed `Result` — consistent with the workspace `panic`/`unwrap`/`expect` lint ban.
- **Recovering.** On an unexpected token the parser emits an `ERROR` node wrapping the offending tokens and **resynchronizes** at the next reliable boundary (statement/item start, closing brace, `;`, newline). Parsing continues, so one syntax error does not cascade into a hundred.
- **Termination (the anti-hang guarantee).** Every parser loop must consume at least one token per iteration or break. This is enforced structurally by a `bump`-or-`error`-and-advance discipline and asserted by the fuzzer (a wall-clock/step budget per parse). A loop that neither advances nor terminates is the one parser bug class we treat as critical.

---

## 6. The canonical tree dump (the contract)

One serializer, `SyntaxNode → String`; the CLI prints it and the golden harness compares it. Same contract discipline as the lexer dump (LF-only, deterministic, no `{:?}`, labels from the single source of truth).

### 6.1 Format
Indented S-expression-ish tree, one node/token per line, two spaces per depth level:

```
NODE_KIND @ lo..hi
  NODE_KIND @ lo..hi
    TOKEN_KIND @ lo..hi "repr"
```

- Interior nodes: `KIND @ lo..hi` (byte range; `line:col` is available via the same `LineMap` but kept out of the tree dump to stay diff-stable — offsets suffice and don't re-pad on edits).
- Leaf tokens: `KIND @ lo..hi "repr"`, repr quoted/escaped exactly as the lexer dump (§2 there). The dump shows **raw text only — no decoded `value=`**: literal-value decoding (`0x10 → 16`) is already pinned by the lexer's snapshot layer one level down, and re-deriving it here would duplicate lexer logic into the parser. The parser dump's job is *tree shape*, not re-verifying lexing.
- Trivia leaves are shown (losslessness is visible in the dump), prefixed nothing special — they are ordinary leaves.
- Error nodes render as `Error @ lo..hi` with their captured tokens as children.

### 6.2 Why offsets not line:col in the tree dump
The lexer dump shows `line:col` because a human reading a flat token list wants it. A tree dump is read structurally; byte offsets are enough to locate, and they never re-pad sibling rows when an edit shifts a column. The `line:col` mapping still exists (diagnostics use it) — it is just not in this particular oracle.

---

## 7. The six layers (mapped from the lexer)

| Layer | Parser form | The hole it closes |
|---|---|---|
| **1. Canonical dump** | `SyntaxNode → String` tree serializer; CLI `parse` + test oracle | "I can't see what the parser built" |
| **2. Golden snapshots** | `*.ax` fixtures → checked-in `*.ast` tree goldens | "a change silently reshaped the tree" |
| **3. Coverage invariants** | `reconstruct` + `spans_well_formed` + `every_token_present` on every fixture & fuzz input | **"a case I never imagined slipped through"** |
| **4. Diagnostics** | malformed `*.ax` → `*.stderr` (message + span + recovery shape) | "bad input mis-parsed, surfaces as a weird crash later" |
| **5. Fuzz** | std-only PRNG: random source *and* random token streams → no-panic + terminates + round-trips | "the unimagined case / the infinite loop" |
| **6. Unit tests** | pinpoint checks on precedence, associativity, recovery points | "the subtle precedence/recovery bug broad tests gloss over" |

Layers **3 and 5 stay the load-bearing pair.**

---

## 8. Architecture (binding)

On top of `RUST_CONVENTIONS.md`:

### 8.1 Functional by default, two stateful cores
- **Pure, single-task functions everywhere** except two deliberate stateful cores: the **builder** (accumulates the green tree) and the **parser** (a cursor over significant tokens with a marker/event stack). Both are small structs with short single-purpose methods. The serializer, the invariants, the `SyntaxKind` classifiers, and the AST accessors are all pure.
- The line, stated once: *pure transforms by default; localized mutation only inside the builder and the parser, never in the serializer, the AST views, or the test helpers.*

### 8.2 Events decouple grammar from tree-building
The parser does not build the tree directly. It emits a flat **event** stream (`Start(kind)`, `Token`, `Finish`, plus a `forward-parent` mechanism for precedence wrapping). A separate, pure-ish `build_tree(events, tokens)` step materializes the green tree and re-inserts trivia (§3). This is the rust-analyzer separation: the grammar reads top-down and never touches `Rc`/offsets, and tree-building (with its trivia/attachment subtlety) lives in exactly one place.

### 8.3 Single source of truth (no hardcoded strings)
`SyntaxKind` and its display labels live in one module. The lexer→syntax token bridge is one function. Adding a node kind is: one `SyntaxKind` variant + one label arm. The serializer, invariants, CLI, and fuzzer are data-driven over the enum — **zero** changes. If a new kind forces a serializer edit, the architecture leaked; fix the architecture.

### 8.4 Complexity caps (mechanically enforced)
Inherited from the workspace lints (`too_many_lines` ≤ 60, `too_many_arguments` ≤ 5, `cognitive_complexity`). Grammar functions are per-production (`fn parse_fn`, `parse_struct`, `parse_expr_bp`), each small. A fat `parse_expr` is split by precedence, not silenced.

---

## 9. Directory layout

```
crates/parser/
├── src/
│   ├── lib.rs            # pub use of the public API (parse, ParseResult, SyntaxNode, serialize, ast)
│   ├── syntax_kind.rs    # SINGLE source of truth: SyntaxKind (token+node), labels, lexer bridge
│   ├── green.rs          # immutable green tree (GreenNode, GreenToken) + builder
│   ├── syntax.rs         # red tree (SyntaxNode, SyntaxToken, SyntaxElement) — lazy offsets/parents
│   ├── event.rs          # parser Event + build_tree (events + tokens → green tree, trivia re-inserted)
│   ├── parser.rs         # the parser cursor (significant-token view, markers, recovery) — stateful core
│   ├── grammar/          # one file per production family
│   │   ├── mod.rs        #   entry: parse a source file
│   │   ├── item.rs       #   fn / struct / enum / trait / impl / mod / use / const
│   │   ├── stmt.rs       #   val / var / let-binding / expr-stmt / return / loop control
│   │   ├── expr.rs       #   Pratt expression parser (§2.7 precedence table)
│   │   ├── pattern.rs    #   match-arm + destructure patterns
│   │   └── ty.rs         #   type annotations (incl. error-union `!`, generics, `dyn`)
│   ├── ast.rs            # typed views over the red tree (cast + accessors)
│   ├── snapshot.rs       # canonical tree serializer (pure)
│   ├── invariants.rs     # reconstruct + spans_well_formed + every_token_present + check_all
│   └── error.rs          # ParseError (thiserror) + diagnostic rendering
└── tests/
    ├── golden.rs         # globs fixtures/*.ax, compares fixtures/*.ast
    ├── invariants.rs     # check_all over every fixture
    ├── diagnostics.rs    # malformed inputs → snapshotted error + recovery shape
    ├── fuzz.rs           # randomized no-panic + terminates + round-trip (std-only PRNG)
    └── fixtures/
        ├── *.ax          # source samples
        ├── *.ast         # golden tree dumps
        └── errors/*.ax   # malformed samples + *.stderr goldens
```

Per the per-folder-docs rule, `crates/parser/README.md` carries the file→responsibility table, updated in the same change as any file move.

---

## 10. What's mechanically enforced vs judgment

| Guarantee | Mechanism | Strength |
|---|---|---|
| Parser never panics | `panic`/`unwrap`/`expect` denied (lints) + fuzz | **Hard** |
| Parser always terminates | per-loop advance discipline + fuzz step budget | **Hard** |
| Source reconstructs exactly from the tree | `reconstruct(tree) == source` (lossless CST) | **Hard** |
| Tree shape sound (nesting/contiguity) | `spans_well_formed` over every fixture + fuzz | **Hard** |
| Every significant token consumed once | `every_token_present` | **Hard** |
| No silent tree regression | golden `*.ast` snapshots | **Hard** |
| `SyntaxKind` ↔ label complete | exhaustive match (compile error) + consistency `#[test]` | **Hard** |
| Lexer/parser kind enums don't drift | one bridge fn + consistency `#[test]` | **Hard** |
| No raw strings in serializer | source-scan `#[test]` | **Hard** (narrow) |
| Small, single-purpose functions | complexity lints | **Hard-ish** |
| Recovery is *good* (resyncs at the right place) | diagnostic snapshots pin it; *quality* is review | **Mixed** |
| Precedence matches §2.7 | targeted unit tests per level | **Hard for tested levels** |

---

## 11. Build order (TDD)

1. `syntax_kind.rs` (data + single source of truth + lexer bridge) → consistency test green.
2. `green.rs` + `syntax.rs` + builder — tree types, unit-tested by **hand-building** a tiny tree and asserting `reconstruct`/offsets, before any parsing.
3. `invariants.rs` + `snapshot.rs` — pure, unit-tested on hand-built trees.
4. `event.rs` (`build_tree`) — unit-tested on hand-written event vectors.
5. `parser.rs` + `grammar/` — written test-first; each construct adds a fixture + golden. Start with expressions (Pratt, the precedence table is the spec), then statements, then items.
6. `diagnostics.rs`, then `fuzz.rs` — once the happy path round-trips, harden recovery and prove totality/termination.

Layers 2–4 are green **before** the grammar parses anything real — so the moment the parser emits its first node, `reconstruct`, `spans_well_formed`, and `every_token_present` are already watching it.

---

## 12. How to run / regenerate

```bash
cargo test -p parser                          # full suite
cargo test -p parser golden                   # snapshots only
UPDATE_SNAPSHOTS=1 cargo test -p parser        # regenerate *.ast / *.stderr goldens (eyeball the diff!)
cargo run -p parser --example parse -- file.ax # the debug tree dump
```

Regeneration discipline is identical to the lexer's: `UPDATE_SNAPSHOTS=1` is a deliberate act, never a reflex to green a red test.
</content>
</invoke>
