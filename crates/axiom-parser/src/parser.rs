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
use axiom_lexer::{Span, Token, TokenKind};

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

    /// Consume the current token into the tree, advancing the cursor.
    pub fn bump(&mut self) {
        if self.at_end() {
            return;
        }
        self.events.push(Event::Token);
        self.pos += 1;
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
