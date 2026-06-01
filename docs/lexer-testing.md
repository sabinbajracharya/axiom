# Lexer Testing & Debugging Spec

> **Status:** authoritative for the lexer's test/debug tooling. Binding before the lexer is written.
> **Decisions baked in:** hand-rolled snapshots (no `insta`), **lossless** lexing (trivia preserved).
> **Companion docs:** [`RUST_CONVENTIONS.md`](../RUST_CONVENTIONS.md) (how we write the Rust), [`ENFORCEMENT.md`](../ENFORCEMENT.md) (what's mechanically checked).

## 0. The concern this answers

A wrong token never crashes — it produces a wrong parse three layers later, silently. Plain `#[test]`s only cover the cases someone thought to write. So the goal of this spec is a tooling stack where **"nothing is missed" is a property the machine enforces, not a promise a contributor makes.** Two ideas carry that weight: a *canonical token dump* that is simultaneously the debug tool and the test oracle, and *coverage invariants* that prove — on every input, including random ones — that the lexer consumed the whole source exactly once.

---

## 1. The six layers (and what each one buys)

| Layer | What it is | The hole it closes |
|---|---|---|
| **1. Canonical dump** | One serializer `&[Token] → String`, exposed as a CLI command *and* used by the test oracle | "I can't see what the lexer produced" |
| **2. Golden snapshots** | `.ax` fixtures + checked-in `.tokens` goldens, globbed by one test | "a change silently broke something" |
| **3. Coverage invariants** | Tiling + reconstruction asserted on every fixture and every fuzz input | **"a case I never imagined slipped through"** ← the core fear |
| **4. Diagnostics** | Malformed input → specific error + correct span, snapshotted | "garbage input mis-handled, surfaces as a weird crash later" |
| **5. Fuzz / no-panic** | Randomized input can never panic and always tiles | "the unimagined case" — the machine imagines them |
| **6. Unit tests** | Pinpoint checks on fiddly atoms (`>>` munch, number literals, escapes) | "the subtle one-token bug broad tests gloss over" |

Layers **3 and 5 are the load-bearing pair** — they make completeness mechanical. The rest give debuggability (1), regression-proofing (2), and aimed coverage (4, 6).

---

## 2. The canonical token format (the contract)

One serializer produces this; the CLI prints it and the golden harness compares it. The format is a **contract** — changing it regenerates every golden, so it is defined precisely here.

### 2.1 Rules

- **One token per line.** No cross-row column alignment. (Aligned tables are diff-hostile: one wide token re-pads every row. We trade prettiness for clean diffs.)
- **Source order**, exactly as the lexer emits. Byte offsets are monotonically non-decreasing.
- **Lossless:** every token — including trivia (whitespace, comments) and `Eof` — appears. Nothing is filtered from the dump.
- **Deterministic:** same source ⇒ byte-identical output. No `{:?}`, no hashes, no clock/paths.
- **LF only**, pinned via `.gitattributes` (`*.tokens text eol=lf`) — same lesson as Oxy's IR snapshots.

### 2.2 Line grammar

```
[<idx>] <Kind> @ <l>:<c>-<l>:<c> (<start>..<end>) <repr>
```

- `<idx>` — 0-based position in the stream.
- `<Kind>` — the `TokenKind` display name, from the single symbol table (§5), never a `{:?}`.
- `<l>:<c>-<l>:<c>` — 1-based start/end line:col (end is exclusive, one past the last char).
- `<start>..<end>` — byte range into the source; `end - start` is the token's byte length.
- `<repr>` — the token text, quoted and escaped so the line never contains a literal newline/tab (`\n`, `\t`, `\\`, `\"`, `\u{..}` for control chars). For tokens carrying a *parsed value that differs from the text*, append ` value=<v>` (e.g. `0x10` → `value=16`, `"a\nb"` → the decoded bytes). This is what catches value-parsing bugs that span-checks alone miss.

### 2.3 Example

Source: `let x = 0x10\n`

```
[0] KwLet      @ 1:1-1:4   (0..3)   "let"
[1] Whitespace @ 1:4-1:5   (3..4)   " "
[2] Ident      @ 1:5-1:6   (4..5)   "x"
[3] Whitespace @ 1:6-1:7   (5..6)   " "
[4] Eq         @ 1:7-1:8   (6..7)   "="
[5] Whitespace @ 1:8-1:9   (7..8)   " "
[6] IntLit     @ 1:9-1:13  (8..12)  "0x10" value=16
[7] Newline    @ 1:13-2:1  (12..13) "\n"
[8] Eof        @ 2:1-2:1   (13..13) ""
```

(Single spaces shown above are illustrative; the emitter writes exactly one space between fields — no padding.)

---

## 3. The lossless / trivia model

The lexer **preserves everything**. Whitespace and comments are emitted as real tokens (`Whitespace`, `LineComment`, `BlockComment`, `Newline`) rather than discarded. Two payoffs:

1. **The strongest coverage invariant becomes available** — text reconstruction (§4), only possible if nothing is dropped.
2. **Axiom's mandatory formatter needs trivia to round-trip.** Capturing it at the lexer means the formatter (and any future CST/IDE tooling) gets it for free instead of re-deriving it.

The parser does **not** consume this raw stream directly. A thin, well-tested filter (`tokens.iter().filter(|t| !t.kind.is_trivia())`) yields the significant-token view the parser walks. Trivia stays attached/available for the formatter. The filter is *one* function with its own unit tests — the only place trivia is dropped, so it can't be dropped accidentally anywhere else.

---

## 4. The coverage invariants (DRY — defined once, used everywhere)

Two pure helpers, implemented **once** in the lexer crate and called by golden tests, unit tests, *and* the fuzzer. This is the anti-duplication rule in action: the completeness guarantee is not re-implemented three times.

### 4.1 `tiles(tokens, source) -> Result<(), TileError>`
Asserts the tokens **tile** the source:
- `tokens[0]` starts at byte 0;
- each token's `start == previous.end` (no gap, no overlap);
- the last real token's `end == source.len()` (`Eof` is zero-width at the end);
- spans are well-formed (`start <= end`, line/col consistent with byte offset).

### 4.2 `reconstruct(tokens) -> String`
Concatenates every token's *raw text* in order. The invariant: **`reconstruct(tokens) == source`, byte for byte.** Because lexing is lossless, this must hold for *any* input. It is the single most powerful check in the suite — it catches dropped bytes, double-counted chars, and span/text mismatches in one assertion.

> Note: `reconstruct` works on the **token data**, not by parsing the snapshot text (the snapshot escapes characters). The snapshot is for humans and diffs; the invariants run on the structs.

---

## 5. Architecture (binding)

These rules apply to the lexer code itself, on top of `RUST_CONVENTIONS.md`. They are why the suite stays maintainable and extendable.

### 5.1 Functional by default, one stateful core
- **Pure, single-task functions everywhere except the scan loop.** The serializer (`format_kind`, `format_span`, `format_repr`, `format_row`, `assemble`), the classifiers (`classify_ident`, `is_ident_start`), and the invariants (`tiles`, `reconstruct`) take input and return output, touch no shared state, and are unit-testable in isolation.
- **One deliberate exception: the scanner.** A lexer is inherently a cursor walking the source. It is a small struct (`pos`, `line`, `col`) with short single-purpose methods (`peek`, `bump`, `scan_number`, `scan_string`, `scan_ident`), each returning one token. Local mutation, contained, never leaking. Forcing this into folds would make it *less* readable for a non-expert — which violates the top conventions rule.
- **The line, stated once:** *pure transforms by default; localized mutation only inside the scanner, never in the serializer or test helpers.*

### 5.2 Single source of truth (no hardcoded strings)
- Keyword spellings, `TokenKind` → display-name mapping, and the format's fixed labels live in **one** symbol table (Oxy's `symbols.rs` discipline). The lexer, the serializer, and the parser all read from it.
- A **consistency `#[test]`** asserts every `TokenKind` has exactly one display-name entry and no orphans — adding a variant without a name fails the test (Oxy's `symbol_consistency.rs` pattern).
- A **focused `#[test]`** scans the serializer module's own source and fails on any raw `"…"` literal outside the constants block — the format strings must come from named constants.

### 5.3 Data-driven extendability (the real test of the architecture)
Adding a token kind must be:
1. one `TokenKind` enum variant,
2. one display-name table entry,
3. (if it's a keyword) one keyword-table entry.

The serializer, `tiles`, `reconstruct`, the CLI, and the fuzzer need **zero** changes — they're data-driven over the enum and the table. If a new kind forces edits in the serializer, the architecture has leaked; fix the architecture, not the symptom.

### 5.4 Complexity caps (mechanically enforced)
`clippy::too_many_lines` (≤60), `too_many_arguments` (≤5), `cognitive_complexity` — all fail the build via the PostToolUse hook's `-D warnings`. Long match arms in the serializer extract to per-case helper functions rather than tripping the cap.

---

## 6. Directory layout

```
crates/axiom-lexer/
├── src/
│   ├── lib.rs            # pub use of the public API (Token, lex(), serialize())
│   ├── token.rs          # Token, TokenKind, Span — plain data
│   ├── symbols.rs        # SINGLE source of truth: keywords, kind→display-name
│   ├── lexer.rs          # the scanner (the one stateful core)
│   ├── snapshot.rs       # canonical serializer (pure functions)
│   └── invariants.rs     # tiles() + reconstruct() (pure, shared by all tests)
└── tests/
    ├── golden.rs         # globs fixtures/*.ax, compares fixtures/*.tokens
    ├── invariants.rs     # runs tiles + reconstruct over every fixture
    ├── diagnostics.rs    # malformed inputs → snapshotted error + span
    ├── fuzz.rs           # randomized no-panic + tiling (std-only PRNG, fixed seed)
    └── fixtures/
        ├── *.ax          # source samples
        ├── *.tokens      # golden token dumps
        └── errors/*.ax   # malformed samples + *.stderr goldens
```

Per the per-folder-docs rule, `crates/axiom-lexer/README.md` carries the file→responsibility table and is updated in the same change as any file move.

---

## 7. How to run / regenerate

```bash
cargo test -p axiom-lexer                      # full suite
cargo test -p axiom-lexer golden               # snapshots only
UPDATE_SNAPSHOTS=1 cargo test -p axiom-lexer    # regenerate *.tokens / *.stderr goldens
cargo run -p axiom-lexer --example lex -- file.ax   # the debug dump (interactive)
```

(The `lex` example is the CLI face of the serializer until the real `axiom` CLI exists; it then becomes `axiom debug tokens`.)

**Regeneration discipline:** `UPDATE_SNAPSHOTS=1` rewrites goldens — always eyeball the diff before committing. A golden change is a deliberate act, never a reflex to make a red test green.

---

## 8. Fuzzing without a dependency

Consistent with the hand-rolled / minimal-deps stance: `fuzz.rs` uses a tiny **std-only deterministic PRNG** (e.g. a 30-line xorshift) seeded from a fixed constant, generating thousands of random inputs (random bytes via `String::from_utf8_lossy`, all-emoji, deeply nested comments, unterminated everything, huge repeats). For each: assert **no panic** (guaranteed compatible with the hard `panic`/`unwrap` lint ban) and that **`tiles` holds**. A fixed seed keeps failures reproducible.

*Optional later upgrade:* `proptest` (dev-dependency only) or `cargo-fuzz` (nightly) if we want shrinking. Not adopted now — the std-only loop meets the "no hole" bar without a dependency.

---

## 9. What's mechanically enforced vs judgment

| Guarantee | Mechanism | Strength |
|---|---|---|
| Lexer never panics | `panic`/`unwrap`/`expect` denied (lints) + fuzz | **Hard** |
| Every byte consumed once | `tiles` over every fixture + every fuzz input | **Hard** |
| Source reconstructs exactly | `reconstruct == source` (lossless) | **Hard** |
| No silent token-stream regression | golden snapshots | **Hard** |
| No byte silently mis-lexed as `Unknown` on valid input | golden test asserts **zero diagnostics** on happy-path fixtures | **Hard** |
| Symbol table complete | consistency `#[test]` | **Hard** |
| No raw strings in serializer | source-scan `#[test]` | **Hard** (narrow) |
| Small, single-purpose functions | complexity lints (proxies) | **Hard-ish** |
| Error messages are good | diagnostic snapshots pin them; *quality* is review | **Mixed** |
| Truly DRY / one-task semantics | data-driven architecture + review | **Soft at the margin** |

The soft residue is kept small and named — the same philosophy as `ENFORCEMENT.md`: mechanize what we can, be honest about the rest.

---

## 10. Build order (TDD)

1. `token.rs` + `symbols.rs` (data + single source of truth) → consistency test green.
2. `invariants.rs` (`tiles`, `reconstruct`) — pure, testable against hand-built token vecs before any real lexing.
3. `snapshot.rs` — pure serializer, unit-tested on hand-built vecs.
4. `lexer.rs` — the scanner, written test-first against fixtures; each new construct adds a fixture + golden.
5. `diagnostics.rs`, then `fuzz.rs` — once the happy path tiles, harden the edges.

Layers 2–3 exist and are green **before** the scanner does anything real — so the moment the scanner emits its first token, the completeness machinery is already watching it.
