//! The parser cursor (`docs/parser-testing.md` §8.1) — one of the two stateful
//! cores. It walks the **significant-token view** (trivia filtered out, the one
//! sanctioned filter from the lexer), emits `Event`s, and collects diagnostics.
//! It never builds the tree directly and never touches trivia or offsets.
//!
//! Recovery & termination (§5): unexpected tokens are wrapped in `Error` nodes
//! via `err_and_bump`, which always consumes a token, so every grammar loop that
//! calls it makes progress. The `at_end` guard plus "every loop either bumps or
//! breaks" is what guarantees the parser always terminates.

use crate::error::ParseError;
use crate::event::Event;
use crate::syntax_kind::SyntaxKind;
use lexer::{Span, Token, TokenKind};

/// A significant token: its kind (in `SyntaxKind` terms), its source span, and
/// its text. Text is also pulled from the full token list at tree-build time,
/// but the parser keeps a copy for the few contextual decisions that need it
/// (e.g. distinguishing the `_` wildcard from an ordinary identifier).
struct SigToken {
    kind: SyntaxKind,
    span: Span,
    text: String,
}

/// The parsing cursor. Produced events + collected errors are taken out by
/// `finish` once the grammar has run.
pub struct Parser {
    tokens: Vec<SigToken>,
    pos: usize,
    events: Vec<Event>,
    errors: Vec<ParseError>,
    /// When set, `{` does not start a struct literal — used while parsing the
    /// condition/scrutinee of `if`/`loop`/`match`, where `{` opens the body.
    no_struct: bool,
    /// Current expression-recursion depth; the guard against stack overflow on
    /// pathologically nested input (totality includes "no crash", §5).
    depth: usize,
    /// Counts of open bracket pairs whose opener has been consumed but whose
    /// closer has not — one per closer kind (`)`, `]`, `}`). Maintained centrally
    /// in `bump`, this is the **recovery set**: leaf recovery (`err_recover`)
    /// refuses to absorb a closer that one of these open constructs is waiting
    /// for, letting that construct claim it instead (§5 recovery quality).
    open_paren: usize,
    open_bracket: usize,
    open_brace: usize,
}

/// Maximum expression nesting before the parser stops descending and recovers.
/// Real code nests far shallower; deeper input is treated as a (recoverable)
/// error rather than risking a stack overflow. A larger limit needs a larger
/// stack — tracked as future work.
const MAX_DEPTH: usize = 64;

/// An open node: created by `start`, closed by `Marker::complete` or discarded
/// by `Marker::abandon`. Points at the placeholder `Start` event.
pub struct Marker {
    pos: usize,
}

/// A closed node, returned by `Marker::complete`. Can be retroactively wrapped
/// by a later node via `precede` (the precedence-climbing mechanism).
#[derive(Clone, Copy)]
pub struct CompletedMarker {
    pos: usize,
}

impl Parser {
    /// Build a cursor over the lexer's tokens, keeping only the significant ones
    /// (trivia and `Eof` dropped — `Eof` is represented by running off the end).
    pub fn new(tokens: &[Token]) -> Parser {
        let sig = tokens
            .iter()
            .filter(|t| !t.kind.is_trivia() && t.kind != TokenKind::Eof)
            .map(|t| SigToken {
                kind: SyntaxKind::from_lexer(&t.kind),
                span: t.span,
                text: t.text.clone(),
            })
            .collect();
        Parser {
            tokens: sig,
            pos: 0,
            events: Vec::new(),
            errors: Vec::new(),
            no_struct: false,
            depth: 0,
            open_paren: 0,
            open_bracket: 0,
            open_brace: 0,
        }
    }

    /// Enter a level of expression recursion. Returns `false` when the depth
    /// limit is hit, signalling the caller to recover instead of descending.
    /// Always pair with `leave_recursion`.
    pub fn enter_recursion(&mut self) -> bool {
        self.depth += 1;
        self.depth < MAX_DEPTH
    }

    pub fn leave_recursion(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    /// Whether a `{` here should be read as a block (not a struct literal).
    pub fn no_struct(&self) -> bool {
        self.no_struct
    }

    /// Set the no-struct restriction, returning the previous value so the caller
    /// can restore it (scoped restriction, rustc-style).
    pub fn set_no_struct(&mut self, value: bool) -> bool {
        std::mem::replace(&mut self.no_struct, value)
    }

    // ── cursor inspection ──────────────────────────────────────────────────

    /// The current token's kind, or `Eof` past the end.
    pub fn current(&self) -> SyntaxKind {
        self.nth(0)
    }

    /// The kind `n` tokens ahead (`Eof` past the end).
    pub fn nth(&self, n: usize) -> SyntaxKind {
        self.tokens
            .get(self.pos + n)
            .map_or(SyntaxKind::Eof, |t| t.kind)
    }

    pub fn at(&self, kind: SyntaxKind) -> bool {
        self.current() == kind
    }

    /// The current token's text (empty past the end). Used only for the handful
    /// of contextual checks (`_`, etc.).
    pub fn current_text(&self) -> &str {
        self.tokens.get(self.pos).map_or("", |t| t.text.as_str())
    }

    /// True if the current token is an identifier with exactly this text.
    pub fn at_contextual(&self, text: &str) -> bool {
        self.current() == SyntaxKind::Ident && self.current_text() == text
    }

    /// True for any of the given kinds — handy for first/follow sets.
    pub fn at_any(&self, kinds: &[SyntaxKind]) -> bool {
        kinds.contains(&self.current())
    }

    pub fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    /// The cursor's current index over significant tokens. Element loops compare
    /// it before and after a sub-parse to detect "no progress" — the signal to
    /// break when recovery declined to consume a closer (so it can bubble out to
    /// the construct that owns it). See `err_recover`.
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Whether the current token is a closing delimiter that some *open* bracket
    /// construct is waiting for — i.e. it is "claimed" by an enclosing scope. A
    /// claimed closer must not be absorbed by leaf recovery; it belongs to its
    /// opener. A stray closer with no matching opener is *not* claimed (and so is
    /// still absorbed, the sensible recovery for genuine garbage).
    pub fn at_claimed_close(&self) -> bool {
        match self.current() {
            SyntaxKind::RParen => self.open_paren > 0,
            SyntaxKind::RBracket => self.open_bracket > 0,
            SyntaxKind::RBrace => self.open_brace > 0,
            _ => false,
        }
    }

    /// The current token's span (or a zero-width span at end of input).
    fn current_span(&self) -> Span {
        match self.tokens.get(self.pos) {
            Some(t) => t.span,
            None => self.tokens.last().map_or(Span { lo: 0, hi: 0 }, |t| Span {
                lo: t.span.hi,
                hi: t.span.hi,
            }),
        }
    }

    // ── consumption ──────────────────────────────────────────────────────

    /// Consume the current token into the tree, advancing the cursor. Emits the
    /// token's kind and byte length so the tree builder can slice the source.
    ///
    /// Every delimiter consumed flows through here, so this is also where the
    /// open-bracket counts (the recovery set) are kept: an opener bumps its count
    /// up, its closer bumps it back down. (A stray closer with no opener simply
    /// saturates at zero.)
    pub fn bump(&mut self) {
        let Some(tok) = self.tokens.get(self.pos) else {
            return;
        };
        match tok.kind {
            SyntaxKind::LParen => self.open_paren += 1,
            SyntaxKind::RParen => self.open_paren = self.open_paren.saturating_sub(1),
            SyntaxKind::LBracket => self.open_bracket += 1,
            SyntaxKind::RBracket => self.open_bracket = self.open_bracket.saturating_sub(1),
            SyntaxKind::LBrace => self.open_brace += 1,
            SyntaxKind::RBrace => self.open_brace = self.open_brace.saturating_sub(1),
            _ => {}
        }
        let len = tok.span.hi - tok.span.lo;
        self.events.push(Event::Token {
            kind: tok.kind,
            len,
        });
        self.pos += 1;
    }

    /// True if the current token can close a generic argument list: a bare `>`,
    /// or a compound beginning with `>` that we split (`>>`, `>=`).
    pub fn at_generic_close(&self) -> bool {
        matches!(
            self.current(),
            SyntaxKind::Gt | SyntaxKind::Shr | SyntaxKind::Ge
        )
    }

    /// Consume a single `>` to close one generic list, splitting a compound token
    /// (`>>` → `>` + `>`, `>=` → `>` + `=`) and leaving the remainder for the
    /// enclosing list. This is what lets nested generics like `Map<K, List<V>>`
    /// close even though `>>` lexes as one `Shr`. Returns whether `>` was eaten.
    pub fn eat_generic_close(&mut self) -> bool {
        match self.current() {
            SyntaxKind::Gt => {
                self.bump();
                true
            }
            SyntaxKind::Shr => {
                self.split_one_gt(SyntaxKind::Gt);
                true
            }
            SyntaxKind::Ge => {
                self.split_one_gt(SyntaxKind::Eq);
                true
            }
            _ => {
                self.expect(SyntaxKind::Gt);
                false
            }
        }
    }

    /// Emit the leading `>` of the current compound token as a `Gt` leaf, then
    /// shrink the current token in place to its single-byte remainder (kept for
    /// the next consumer). The tree builder slices the original source token, so
    /// the two halves still reconstruct byte-for-byte.
    fn split_one_gt(&mut self, remaining_kind: SyntaxKind) {
        self.events.push(Event::Token {
            kind: SyntaxKind::Gt,
            len: 1,
        });
        if let Some(tok) = self.tokens.get_mut(self.pos) {
            tok.kind = remaining_kind;
            tok.span.lo += 1;
            // The split is only ever applied to ASCII `>>`/`>=`, so byte 1 is a
            // valid boundary; `.get` keeps it non-panicking regardless.
            tok.text = tok.text.get(1..).unwrap_or("").to_string();
        }
    }

    /// Consume the current token iff it is `kind`; report whether it was.
    pub fn eat(&mut self, kind: SyntaxKind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Consume the current token iff it is one of `kinds`; report whether it was.
    pub fn eat_any(&mut self, kinds: &[SyntaxKind]) -> bool {
        if self.at_any(kinds) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Consume `kind` or record an "expected" diagnostic (without consuming).
    pub fn expect(&mut self, kind: SyntaxKind) -> bool {
        if self.eat(kind) {
            return true;
        }
        self.error(format!(
            "expected {}, found {}",
            kind.label(),
            self.current().label()
        ));
        false
    }

    // ── diagnostics & recovery ──────────────────────────────────────────────

    /// Record a diagnostic at the current position without consuming anything.
    pub fn error(&mut self, message: impl Into<String>) {
        self.errors
            .push(ParseError::new(message, self.current_span()));
    }

    /// Wrap the current token in an `Error` node and consume it — the recovery
    /// primitive. Always advances (unless at end), guaranteeing loop progress.
    pub fn err_and_bump(&mut self, message: impl Into<String>) {
        self.error(message);
        if self.at_end() {
            return;
        }
        let m = self.start();
        self.bump();
        m.complete(self, SyntaxKind::Error);
    }

    /// Recovery-set-aware variant of `err_and_bump`, for leaf positions (a
    /// missing expression / pattern / member). Reports the error, then absorbs
    /// the current token into an `Error` node **unless** it is a closing
    /// delimiter an enclosing construct is waiting for (`at_claimed_close`) or we
    /// are at end of input — in those cases the token is left in place for its
    /// owner to consume. Returns whether a token was consumed; a `false` is the
    /// caller's cue (in a non-comma loop) to break so the closer can bubble out.
    pub fn err_recover(&mut self, message: impl Into<String>) -> bool {
        self.error(message);
        if self.at_end() || self.at_claimed_close() {
            return false;
        }
        let m = self.start();
        self.bump();
        m.complete(self, SyntaxKind::Error);
        true
    }

    /// Multi-token resynchronizing recovery, for list positions (statements,
    /// items, members). Reports `message` **once**, then absorbs the whole run of
    /// unexpected tokens into a **single** `Error` node, stopping *before* the
    /// first token where `is_sync` holds — the start of the enclosing list's next
    /// element — or a claimed closing delimiter (`at_claimed_close`) or end of
    /// input. Collapsing a garbage run into one node with one diagnostic is what
    /// turns the old per-token cascade into a clean resync to the next
    /// statement/item.
    ///
    /// Returns whether any token was consumed. A `false` means we were already at
    /// a sync point, a claimed closer, or the end — the caller's cue to break so
    /// that token is handled by whoever owns it (the loop then re-dispatches on
    /// the sync token, or the closer bubbles out).
    pub fn recover_to(
        &mut self,
        message: impl Into<String>,
        is_sync: impl Fn(&Parser) -> bool,
    ) -> bool {
        self.error(message);
        if self.at_end() || self.at_claimed_close() || is_sync(self) {
            return false;
        }
        let m = self.start();
        while !self.at_end() && !self.at_claimed_close() && !is_sync(self) {
            self.bump();
        }
        m.complete(self, SyntaxKind::Error);
        true
    }

    // ── markers ──────────────────────────────────────────────────────────

    /// Open a node. Must be `complete`d or `abandon`ed.
    pub fn start(&mut self) -> Marker {
        let pos = self.events.len();
        self.events.push(Event::Tombstone);
        Marker { pos }
    }

    /// Consume the parser, returning the event stream and diagnostics.
    pub fn finish(self) -> (Vec<Event>, Vec<ParseError>) {
        (self.events, self.errors)
    }
}

impl Marker {
    /// Close this node with `kind`. Returns a `CompletedMarker` that can later
    /// `precede` (wrap) this node.
    pub fn complete(self, p: &mut Parser, kind: SyntaxKind) -> CompletedMarker {
        if let Some(slot) = p.events.get_mut(self.pos) {
            *slot = Event::Start {
                kind,
                forward_parent: None,
            };
        }
        p.events.push(Event::Finish);
        CompletedMarker { pos: self.pos }
    }
}

impl CompletedMarker {
    /// Open a new node that becomes the **parent** of this completed one — used
    /// by the Pratt loop to wrap an already-parsed lhs in a binary expression.
    pub fn precede(self, p: &mut Parser) -> Marker {
        let new_marker = p.start();
        if let Some(Event::Start { forward_parent, .. }) = p.events.get_mut(self.pos) {
            *forward_parent = Some(new_marker.pos);
        }
        new_marker
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parser(src: &str) -> Parser {
        Parser::new(&lexer::lex(src).tokens)
    }

    #[test]
    fn test_recover_to_absorbs_run_and_stops_at_sync() {
        // `@ @` is garbage; `fn` is a sync token. recover_to consumes the two
        // `@`, stops *before* `fn`, reports exactly once, and returns true.
        let mut p = parser("@ @ fn");
        let consumed = p.recover_to("expected an item", |p| p.at(SyntaxKind::KwFn));
        assert!(consumed);
        assert!(p.at(SyntaxKind::KwFn), "must stop before the sync token");
        assert_eq!(p.errors.len(), 1, "one run → one diagnostic");
    }

    #[test]
    fn test_recover_to_declines_when_already_at_sync() {
        // Already at a sync token: report, consume nothing, return false (the
        // caller's cue to let the loop re-dispatch on it).
        let mut p = parser("fn");
        let consumed = p.recover_to("expected an item", |p| p.at(SyntaxKind::KwFn));
        assert!(!consumed);
        assert!(p.at(SyntaxKind::KwFn));
    }

    #[test]
    fn test_recover_to_leaves_claimed_closer_for_owner() {
        // An unclosed `(` marks `)` as claimed; recover_to must stop before it so
        // the enclosing construct can consume it (recovery-set awareness).
        let mut p = parser("( @ )");
        p.bump(); // consume `(` → open_paren = 1
        let consumed = p.recover_to("garbage", |_| false);
        assert!(consumed);
        assert!(
            p.at(SyntaxKind::RParen),
            "the claimed `)` must be left in place for its owner"
        );
    }
}
