// Tests legitimately use unwrap/expect/panic. RUST_CONVENTIONS §3.4.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::green::GreenNodeBuilder;
use crate::parse;

// ── Consistency test infrastructure ──────────────────────────────────────

fn can_cast_item(kind: SyntaxKind) -> bool {
    FnDef::can_cast(kind)
        || StructDef::can_cast(kind)
        || EnumDef::can_cast(kind)
        || TraitDef::can_cast(kind)
        || ImplBlock::can_cast(kind)
        || ModDef::can_cast(kind)
        || UseDecl::can_cast(kind)
        || ConstDef::can_cast(kind)
        || ErrorSetDef::can_cast(kind)
}

fn can_cast_item_part(kind: SyntaxKind) -> bool {
    Visibility::can_cast(kind)
        || ParamList::can_cast(kind)
        || Param::can_cast(kind)
        || SelfParam::can_cast(kind)
        || FieldList::can_cast(kind)
        || Field::can_cast(kind)
        || VariantList::can_cast(kind)
        || Variant::can_cast(kind)
        || VariantPayload::can_cast(kind)
        || GenericParamList::can_cast(kind)
        || GenericParam::can_cast(kind)
        || TraitBounds::can_cast(kind)
        || RetType::can_cast(kind)
        || UseTree::can_cast(kind)
        || UseGroup::can_cast(kind)
        || UseRename::can_cast(kind)
        || ErrorVariantList::can_cast(kind)
        || ErrorVariant::can_cast(kind)
        || AssocItemList::can_cast(kind)
        || TraitItemList::can_cast(kind)
}

fn can_cast_stmt(kind: SyntaxKind) -> bool {
    LetStmt::can_cast(kind)
        || ExprStmt::can_cast(kind)
        || ReturnStmt::can_cast(kind)
        || BreakStmt::can_cast(kind)
        || ContinueStmt::can_cast(kind)
        || ErrdeferStmt::can_cast(kind)
}

fn can_cast_expr(kind: SyntaxKind) -> bool {
    BlockExpr::can_cast(kind)
        || LiteralExpr::can_cast(kind)
        || PathExpr::can_cast(kind)
        || BinExpr::can_cast(kind)
        || PrefixExpr::can_cast(kind)
        || CallExpr::can_cast(kind)
        || MethodCallExpr::can_cast(kind)
        || FieldExpr::can_cast(kind)
        || IndexExpr::can_cast(kind)
        || ParenExpr::can_cast(kind)
        || IfExpr::can_cast(kind)
        || MatchExpr::can_cast(kind)
        || LoopExpr::can_cast(kind)
        || ClosureExpr::can_cast(kind)
        || StructLitExpr::can_cast(kind)
        || CastExpr::can_cast(kind)
        || RangeExpr::can_cast(kind)
        || TryExpr::can_cast(kind)
        || AssignExpr::can_cast(kind)
        || CatchExpr::can_cast(kind)
        || ScopeExpr::can_cast(kind)
        || SpawnExpr::can_cast(kind)
        || ListLitExpr::can_cast(kind)
}

fn can_cast_path_name(kind: SyntaxKind) -> bool {
    Path::can_cast(kind)
        || PathSegment::can_cast(kind)
        || Name::can_cast(kind)
        || NameRef::can_cast(kind)
}

fn can_cast_loop_match_misc(kind: SyntaxKind) -> bool {
    LoopCondition::can_cast(kind)
        || LoopIter::can_cast(kind)
        || LoopLabel::can_cast(kind)
        || MatchArmList::can_cast(kind)
        || MatchArm::can_cast(kind)
        || MatchGuard::can_cast(kind)
        || ClosureParamList::can_cast(kind)
        || ClosureParam::can_cast(kind)
        || StructLitFieldList::can_cast(kind)
        || StructLitField::can_cast(kind)
        || ArgList::can_cast(kind)
}

fn can_cast_pattern(kind: SyntaxKind) -> bool {
    WildcardPat::can_cast(kind)
        || LiteralPat::can_cast(kind)
        || IdentPat::can_cast(kind)
        || TupleStructPat::can_cast(kind)
        || StructPat::can_cast(kind)
        || PathPat::can_cast(kind)
        || OrPat::can_cast(kind)
        || RestPat::can_cast(kind)
        || RangePat::can_cast(kind)
        || StructPatFieldList::can_cast(kind)
        || StructPatField::can_cast(kind)
        || TuplePatFieldList::can_cast(kind)
}

fn can_cast_type(kind: SyntaxKind) -> bool {
    PathType::can_cast(kind)
        || GenericArgList::can_cast(kind)
        || ErrorUnionType::can_cast(kind)
        || ErrorSetUnionType::can_cast(kind)
        || DynType::can_cast(kind)
        || UnitType::can_cast(kind)
        || FnType::can_cast(kind)
        || FnTypeParams::can_cast(kind)
}

fn can_cast_any(kind: SyntaxKind) -> bool {
    SourceFile::can_cast(kind)
        || can_cast_item(kind)
        || can_cast_item_part(kind)
        || can_cast_stmt(kind)
        || can_cast_expr(kind)
        || can_cast_path_name(kind)
        || can_cast_loop_match_misc(kind)
        || can_cast_pattern(kind)
        || can_cast_type(kind)
}

// ── Consistency ───────────────────────────────────────────────────────────

/// Every non-Error node kind must have a corresponding AST view.
/// Adding a node kind to `SyntaxKind` without a view causes this test to
/// fail; adding a view without registering it in `can_cast_any` also fails.
#[test]
fn test_ast_every_node_kind_covered() {
    for &kind in SyntaxKind::ALL {
        if kind.is_node() && kind != SyntaxKind::Error {
            assert!(
                can_cast_any(kind),
                "{kind:?} is a node kind but has no AST view — \
                 add one and register it in can_cast_any"
            );
        }
    }
}

/// `cast(node).syntax().kind() == node.kind()` holds for every view.
#[test]
fn test_ast_cast_round_trip() {
    fn build(kind: SyntaxKind) -> SyntaxNode {
        let mut b = GreenNodeBuilder::new();
        b.start_node(kind);
        b.finish_node();
        SyntaxNode::new_root(b.finish())
    }

    // Each line: cast the right kind, assert the round-trip, then assert a
    // wrong-kind node is rejected.
    let fn_def = FnDef::cast(build(SyntaxKind::FnDef)).expect("FnDef::cast");
    assert_eq!(fn_def.syntax().kind(), SyntaxKind::FnDef);
    assert!(FnDef::cast(build(SyntaxKind::StructDef)).is_none());

    let struct_def = StructDef::cast(build(SyntaxKind::StructDef)).expect("StructDef::cast");
    assert_eq!(struct_def.syntax().kind(), SyntaxKind::StructDef);

    let let_stmt = LetStmt::cast(build(SyntaxKind::LetStmt)).expect("LetStmt::cast");
    assert_eq!(let_stmt.syntax().kind(), SyntaxKind::LetStmt);

    let bin_expr = BinExpr::cast(build(SyntaxKind::BinExpr)).expect("BinExpr::cast");
    assert_eq!(bin_expr.syntax().kind(), SyntaxKind::BinExpr);

    let name = Name::cast(build(SyntaxKind::Name)).expect("Name::cast");
    assert_eq!(name.syntax().kind(), SyntaxKind::Name);

    let path_type = PathType::cast(build(SyntaxKind::PathType)).expect("PathType::cast");
    assert_eq!(path_type.syntax().kind(), SyntaxKind::PathType);

    let ident_pat = IdentPat::cast(build(SyntaxKind::IdentPat)).expect("IdentPat::cast");
    assert_eq!(ident_pat.syntax().kind(), SyntaxKind::IdentPat);
    assert!(IdentPat::cast(build(SyntaxKind::WildcardPat)).is_none());
}

// ── Per-view unit tests ───────────────────────────────────────────────────

#[test]
fn test_fn_def_cast_round_trip() {
    let mut b = GreenNodeBuilder::new();
    b.start_node(SyntaxKind::FnDef);
    b.finish_node();
    let node = SyntaxNode::new_root(b.finish());
    let view = FnDef::cast(node).expect("FnDef::cast must succeed");
    assert_eq!(view.syntax().kind(), SyntaxKind::FnDef);
}

#[test]
fn test_fn_def_cast_rejects_wrong_kind() {
    let mut b = GreenNodeBuilder::new();
    b.start_node(SyntaxKind::StructDef);
    b.finish_node();
    let node = SyntaxNode::new_root(b.finish());
    assert!(FnDef::cast(node).is_none());
}

#[test]
fn test_fn_def_name_accessor() {
    let result = parse("fn greet() {}");
    let fn_def = result
        .tree
        .child_nodes()
        .into_iter()
        .find_map(FnDef::cast)
        .expect("should have a FnDef");
    let name = fn_def.name().expect("FnDef should have a name");
    assert_eq!(name.text(), Some("greet".to_string()));
}

#[test]
fn test_fn_def_name_skips_trivia() {
    // Trivia (whitespace) between `fn` and the name must not appear in
    // the Name node's token — name.text() returns the identifier text only.
    let result = parse("fn   spaced() {}");
    let fn_def = result
        .tree
        .child_nodes()
        .into_iter()
        .find_map(FnDef::cast)
        .expect("should have a FnDef");
    let name = fn_def.name().expect("FnDef should have a name");
    assert_eq!(name.text(), Some("spaced".to_string()));
}

#[test]
fn test_fn_def_param_list() {
    let result = parse("fn add(x: i32, y: i32) -> i32 {}");
    let fn_def = result
        .tree
        .child_nodes()
        .into_iter()
        .find_map(FnDef::cast)
        .expect("should have a FnDef");
    let params = fn_def.param_list().expect("should have param list");
    assert_eq!(params.params().len(), 2);
}

#[test]
fn test_fn_def_ret_type() {
    let result = parse("fn inc(x: i32) -> i32 { x }");
    let fn_def = result
        .tree
        .child_nodes()
        .into_iter()
        .find_map(FnDef::cast)
        .expect("should have a FnDef");
    assert!(fn_def.ret_type().is_some(), "should have a return type");
}

#[test]
fn test_struct_def_name_and_fields() {
    let result = parse("struct Point { x: f64, y: f64 }");
    let def = result
        .tree
        .child_nodes()
        .into_iter()
        .find_map(StructDef::cast)
        .expect("should have a StructDef");
    assert_eq!(def.name().and_then(|n| n.text()), Some("Point".to_string()));
    let fields = def.field_list().expect("should have field list").fields();
    assert_eq!(fields.len(), 2);
}

#[test]
fn test_let_stmt_accessors() {
    let result = parse("fn f() { val x: i32 = 1 }");
    let fn_def = result
        .tree
        .child_nodes()
        .into_iter()
        .find_map(FnDef::cast)
        .expect("FnDef");
    let body = fn_def.body().expect("body");
    let stmt = body
        .stmts()
        .into_iter()
        .find_map(LetStmt::cast)
        .expect("LetStmt");
    let kw = stmt.binding_kw().expect("binding keyword");
    assert_eq!(kw.kind(), SyntaxKind::KwVal);
    assert!(stmt.pattern().is_some(), "should have a pattern");
    assert!(stmt.ty().is_some(), "should have a type annotation");
    assert!(stmt.value().is_some(), "should have an initializer");
}

#[test]
fn test_bin_expr_operands_and_operator() {
    let result = parse("fn f() { 1 + 2 }");
    let fn_def = result
        .tree
        .child_nodes()
        .into_iter()
        .find_map(FnDef::cast)
        .expect("FnDef");
    let body = fn_def.body().expect("body");
    // The expression statement wraps the BinExpr.
    let expr_stmt = body
        .stmts()
        .into_iter()
        .find_map(ExprStmt::cast)
        .expect("ExprStmt");
    let bin = BinExpr::cast(expr_stmt.expr().expect("ExprStmt has expr")).expect("BinExpr");
    assert!(bin.lhs().is_some(), "lhs should exist");
    assert!(bin.rhs().is_some(), "rhs should exist");
    let op = bin.op_token().expect("operator token");
    assert_eq!(op.kind(), SyntaxKind::Plus);
}

#[test]
fn test_name_text() {
    let result = parse("fn hello() {}");
    let fn_def = result
        .tree
        .child_nodes()
        .into_iter()
        .find_map(FnDef::cast)
        .expect("FnDef");
    let name = fn_def.name().expect("name");
    assert_eq!(
        name.ident_token().map(|t| t.kind()),
        Some(SyntaxKind::Ident)
    );
    assert_eq!(name.text(), Some("hello".to_string()));
}

#[test]
fn test_source_file_items_excludes_error_nodes() {
    // Error nodes in the source must not show up in `SourceFile::items()`.
    let result = parse("@ fn f() {}");
    let sf = SourceFile::cast(result.tree).expect("SourceFile");
    let items = sf.items();
    assert!(
        items.iter().all(|n| n.kind() != SyntaxKind::Error),
        "items() must exclude Error nodes"
    );
    assert_eq!(items.len(), 1, "only the fn should be an item");
}
