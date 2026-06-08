//! Core HIR types. Desugared, ID-keyed, name-resolved (or diagnosed).
//! Every node carries a stable `HirId` for type annotation in later stages.

mod items;
mod ty;
pub use items::*;
pub use ty::*;

use lexer::Span;
use std::fmt;

// ── Stable IDs ────────────────────────────────────────────────────────────────

/// A stable identifier for an HIR node, assigned in source order during lowering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HirId(pub usize);

/// A definition ID — the `HirId` of the item/binding/param where a name is defined.
pub type DefId = HirId;

impl fmt::Display for HirId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ── Calling convention ────────────────────────────────────────────────────────

/// The calling convention on a parameter or argument.
/// Present from the start even though enforcement is deferred to v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallingConvention {
    Let,
    Inout,
    Sink,
}

impl fmt::Display for CallingConvention {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CallingConvention::Let => write!(f, "let"),
            CallingConvention::Inout => write!(f, "inout"),
            CallingConvention::Sink => write!(f, "sink"),
        }
    }
}

// ── Visibility ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Private,
    Public,
}

impl fmt::Display for Visibility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Visibility::Private => write!(f, "private"),
            Visibility::Public => write!(f, "pub"),
        }
    }
}

// ── Resolved names ────────────────────────────────────────────────────────────

/// A name that resolved successfully, pointing at its definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedName {
    pub def_id: DefId,
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

impl NameRef {
    pub fn resolved(def_id: DefId, text: impl Into<String>) -> NameRef {
        NameRef::Resolved(ResolvedName {
            def_id,
            text: text.into(),
        })
    }

    pub fn unresolved(text: impl Into<String>) -> NameRef {
        NameRef::Unresolved(UnresolvedName { text: text.into() })
    }
}

// ── Top-level HIR ─────────────────────────────────────────────────────────────

/// The complete output of HIR lowering + name resolution.
#[derive(Debug, Clone)]
pub struct Hir {
    pub items: Vec<Item>,
    pub diagnostics: Vec<crate::HirDiagnostic>,
}

// ── Statements ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Stmt {
    ValStmt(ValStmt),
    VarStmt(VarStmt),
    ExprStmt(ExprStmt),
    ReturnStmt(ReturnStmt),
    BreakStmt(BreakStmt),
    ContinueStmt(ContinueStmt),
    YieldStmt(YieldStmt),
}

impl Stmt {
    pub fn id(&self) -> HirId {
        match self {
            Stmt::ValStmt(s) => s.id,
            Stmt::VarStmt(s) => s.id,
            Stmt::ExprStmt(s) => s.id,
            Stmt::ReturnStmt(s) => s.id,
            Stmt::BreakStmt(s) => s.id,
            Stmt::ContinueStmt(s) => s.id,
            Stmt::YieldStmt(s) => s.id,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ValStmt {
    pub id: HirId,
    pub pattern: Pattern,
    pub ty: Option<HirTy>,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub struct VarStmt {
    pub id: HirId,
    pub pattern: Pattern,
    pub ty: Option<HirTy>,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub struct ExprStmt {
    pub id: HirId,
    pub expr: Expr,
}

#[derive(Debug, Clone)]
pub struct ReturnStmt {
    pub id: HirId,
    pub value: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct BreakStmt {
    pub id: HirId,
    pub value: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct ContinueStmt {
    pub id: HirId,
}

#[derive(Debug, Clone)]
pub struct YieldStmt {
    pub id: HirId,
    pub value: Expr,
}

// ── Expressions ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
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
    ListLit(ListLitExpr),
    Assign(AssignExpr),
}

impl Expr {
    pub fn id(&self) -> HirId {
        match self {
            Expr::Lit(e) => e.id,
            Expr::Path(e) => e.id,
            Expr::Bin(e) => e.id,
            Expr::Unary(e) => e.id,
            Expr::Call(e) => e.id,
            Expr::MethodCall(e) => e.id,
            Expr::Field(e) => e.id,
            Expr::Index(e) => e.id,
            Expr::Block(e) => e.id,
            Expr::If(e) => e.id,
            Expr::Match(e) => e.id,
            Expr::Loop(e) => e.id,
            Expr::StructLit(e) => e.id,
            Expr::Assign(e) => e.id,
            Expr::ListLit(e) => e.id,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LitExpr {
    pub id: HirId,
    pub kind: LitKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LitKind {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Unit,
}

#[derive(Debug, Clone)]
pub struct PathExpr {
    pub id: HirId,
    pub name_ref: NameRef,
}

#[derive(Debug, Clone)]
pub struct BinExpr {
    pub id: HirId,
    pub op: BinOp,
    pub left: Box<Expr>,
    pub right: Box<Expr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Shl,
    Shr,
    BitAnd,
    BitOr,
    BitXor,
}

impl fmt::Display for BinOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Mod => "%",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And => "&&",
            BinOp::Or => "||",
            BinOp::Shl => "<<",
            BinOp::Shr => ">>",
            BinOp::BitAnd => "&",
            BinOp::BitOr => "|",
            BinOp::BitXor => "^",
        };
        write!(f, "{s}")
    }
}

#[derive(Debug, Clone)]
pub struct UnaryExpr {
    pub id: HirId,
    pub op: UnaryOp,
    pub operand: Box<Expr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
}

impl fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnaryOp::Neg => write!(f, "-"),
            UnaryOp::Not => write!(f, "!"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CallExpr {
    pub id: HirId,
    pub callee: NameRef,
    /// The path segment(s) before the callee, joined by `::` — e.g. `List` in
    /// `List::new()`. `None` for an unqualified call. Used to resolve
    /// associated functions (`Type::method`); enum constructors and
    /// module-qualified calls continue to resolve off `callee` (the last
    /// segment).
    pub qualifier: Option<String>,
    pub args: Vec<Expr>,
}

#[derive(Debug, Clone)]
pub struct MethodCallExpr {
    pub id: HirId,
    pub receiver: Box<Expr>,
    pub method: String,
    pub args: Vec<Expr>,
}

#[derive(Debug, Clone)]
pub struct FieldExpr {
    pub id: HirId,
    pub receiver: Box<Expr>,
    pub field: String,
}

#[derive(Debug, Clone)]
pub struct IndexExpr {
    pub id: HirId,
    pub base: Box<Expr>,
    pub indices: Vec<Expr>,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub id: HirId,
    pub stmts: Vec<Stmt>,
    pub tail: Option<Box<Expr>>,
}

#[derive(Debug, Clone)]
pub struct IfExpr {
    pub id: HirId,
    pub condition: Box<Expr>,
    pub then_branch: Block,
    pub else_branch: Option<Box<Expr>>,
}

#[derive(Debug, Clone)]
pub struct MatchExpr {
    pub id: HirId,
    pub scrutinee: Box<Expr>,
    pub arms: Vec<MatchArm>,
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Expr,
}

#[derive(Debug, Clone)]
pub struct LoopExpr {
    pub id: HirId,
    pub kind: LoopKind,
}

#[derive(Debug, Clone)]
pub enum LoopKind {
    Infinite(Block),
    Conditional {
        condition: Box<Expr>,
        body: Block,
    },
    Iterator {
        binding: String,
        binding_id: HirId,
        iterable: Box<Expr>,
        body: Block,
    },
}

#[derive(Debug, Clone)]
pub struct StructLitExpr {
    pub id: HirId,
    pub type_name: NameRef,
    pub fields: Vec<StructLitField>,
}

#[derive(Debug, Clone)]
pub struct StructLitField {
    pub name: String,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub struct ListLitExpr {
    pub id: HirId,
    pub elements: Vec<Expr>,
}

#[derive(Debug, Clone)]
pub struct AssignExpr {
    pub id: HirId,
    pub target: AssignTarget,
    pub value: Box<Expr>,
    pub op: AssignOp,
}

#[derive(Debug, Clone)]
pub enum AssignTarget {
    Name(NameRef),
    Field { receiver: Box<Expr>, field: String },
    Index { base: Box<Expr>, indices: Vec<Expr> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Plain,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}

impl fmt::Display for AssignOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            AssignOp::Plain => "=",
            AssignOp::Add => "+=",
            AssignOp::Sub => "-=",
            AssignOp::Mul => "*=",
            AssignOp::Div => "/=",
            AssignOp::Mod => "%=",
        };
        write!(f, "{s}")
    }
}

// ── Patterns ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Pattern {
    Wildcard(HirId),
    Ident(IdentPat),
    Literal(LitPat),
    TupleStruct(TupleStructPat),
    Struct(StructPat),
    Or(OrPat),
    Range(RangePat),
}

impl Pattern {
    pub fn id(&self) -> HirId {
        match self {
            Pattern::Wildcard(id) => *id,
            Pattern::Ident(p) => p.id,
            Pattern::Literal(p) => p.id,
            Pattern::TupleStruct(p) => p.id,
            Pattern::Struct(p) => p.id,
            Pattern::Or(p) => p.id,
            Pattern::Range(p) => p.id,
        }
    }
}

#[derive(Debug, Clone)]
pub struct IdentPat {
    pub id: HirId,
    pub name: String,
    pub binding: Option<DefId>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct LitPat {
    pub id: HirId,
    pub kind: LitKind,
}

#[derive(Debug, Clone)]
pub struct TupleStructPat {
    pub id: HirId,
    pub path: NameRef,
    pub fields: Vec<Pattern>,
}

#[derive(Debug, Clone)]
pub struct StructPat {
    pub id: HirId,
    pub path: NameRef,
    pub fields: Vec<StructPatField>,
}

#[derive(Debug, Clone)]
pub struct StructPatField {
    pub name: String,
    pub pattern: Pattern,
}

#[derive(Debug, Clone)]
pub struct OrPat {
    pub id: HirId,
    pub alternatives: Vec<Pattern>,
}

#[derive(Debug, Clone)]
pub struct RangePat {
    pub id: HirId,
    pub start: Option<LitKind>,
    pub end: Option<LitKind>,
    pub inclusive: bool,
}
