//! Name resolution: two-pass resolution of identifiers to definitions.
//!
//! Pass 1 collects top-level item definitions (fn, struct, enum) into a symbol table.
//! Pass 2 resolves name references in bodies against lexical scopes.
//!
//! Per `docs/hir-testing.md` §4: same-scope shadowing is disallowed;
//! nested-scope shadowing is allowed.

use crate::hir::*;
use crate::lower::DefKind;
use crate::HirDiagnostic;
use axiom_lexer::Span;
use std::collections::{HashMap, HashSet};

/// Run name resolution over the HIR built by lowering.
/// Mutates the HIR in-place: resolves `NameRef::Unresolved` entries
/// to `NameRef::Resolved` where names are found, and emits diagnostics
/// where they are not.
pub fn resolve(ctx: &mut crate::lower::LowerCtx) {
    // Pass 1: top-level item defs are already collected in ctx.defs during lowering.
    // Build a top-level scope map.
    let mut top_level: HashMap<String, (DefId, DefKind)> = HashMap::new();
    for def in &ctx.defs {
        if matches!(
            def.kind,
            DefKind::Fn | DefKind::Struct | DefKind::Enum | DefKind::Trait | DefKind::Variant
        ) {
            if top_level.contains_key(&def.name) {
                ctx.diagnostics.push(HirDiagnostic::DuplicateDefinition {
                    name: def.name.clone(),
                    span: Span { lo: 0, hi: 0 },
                });
            } else {
                top_level.insert(def.name.clone(), (def.def_id, def.kind));
            }
        }
    }

    // Pass 2: resolve name references in all items.
    for item in &mut ctx.items {
        resolve_item_names(item, &top_level, &mut ctx.diagnostics);
    }
}

fn resolve_item_names(
    item: &mut Item,
    top_level: &HashMap<String, (DefId, DefKind)>,
    diagnostics: &mut Vec<HirDiagnostic>,
) {
    match item {
        Item::FnDef(f) => {
            let mut scope = Scope::new_child(top_level);
            // Register type params in scope so T in param types resolves.
            for tp in &f.type_params {
                scope.define(tp.name.clone(), tp.id, DefKind::TypeParam);
            }
            // Resolve type param references in param and return types.
            for param in &mut f.params {
                if let Some(ty) = &mut param.ty {
                    resolve_ty_names(ty, &scope.bindings);
                }
                scope.define(param.name.clone(), param.id, DefKind::Param);
            }
            if let Some(ret) = &mut f.return_type {
                resolve_ty_names(ret, &scope.bindings);
            }
            resolve_block_names(&mut f.body, &scope, diagnostics);
        }
        Item::StructDef(s) => {
            let mut scope = Scope::new_child(top_level);
            for tp in &s.type_params {
                scope.define(tp.name.clone(), tp.id, DefKind::TypeParam);
            }
            for field in &mut s.fields {
                resolve_ty_names(&mut field.ty, &scope.bindings);
            }
        }
        Item::EnumDef(e) => {
            let mut scope = Scope::new_child(top_level);
            for tp in &e.type_params {
                scope.define(tp.name.clone(), tp.id, DefKind::TypeParam);
            }
            for variant in &mut e.variants {
                for payload_ty in &mut variant.payload {
                    resolve_ty_names(payload_ty, &scope.bindings);
                }
            }
        }
        Item::TraitDef(t) => resolve_trait_def(t, top_level, diagnostics),
        Item::ImplDef(impl_def) => resolve_impl_def(impl_def, top_level, diagnostics),
    }
}

fn resolve_trait_def(
    t: &mut TraitDef,
    top_level: &HashMap<String, (DefId, DefKind)>,
    diagnostics: &mut Vec<HirDiagnostic>,
) {
    let mut scope = Scope::new_child(top_level);
    for tp in &t.type_params {
        scope.define(tp.name.clone(), tp.id, DefKind::TypeParam);
    }
    for method in &mut t.methods {
        resolve_method_sig(
            &mut method.params,
            &mut method.return_type,
            &scope,
            method.body.as_mut(),
            diagnostics,
        );
    }
}

fn resolve_impl_def(
    impl_def: &mut ImplDef,
    top_level: &HashMap<String, (DefId, DefKind)>,
    diagnostics: &mut Vec<HirDiagnostic>,
) {
    if let Some(trait_nr) = &mut impl_def.trait_name {
        resolve_name_ref(trait_nr, top_level, diagnostics);
    }
    resolve_name_ref(&mut impl_def.type_name, top_level, diagnostics);
    let mut scope = Scope::new_child(top_level);
    for tp in &impl_def.type_params {
        scope.define(tp.name.clone(), tp.id, DefKind::TypeParam);
    }
    for method in &mut impl_def.methods {
        resolve_method_sig(
            &mut method.params,
            &mut method.return_type,
            &scope,
            Some(&mut method.body),
            diagnostics,
        );
    }
}

/// Resolve param types, register param names, resolve return type,
/// and optionally resolve a body with the param scope.
fn resolve_method_sig(
    params: &mut [Param],
    return_type: &mut Option<HirTy>,
    scope: &Scope,
    body: Option<&mut Block>,
    diagnostics: &mut Vec<HirDiagnostic>,
) {
    let mut mscope = Scope::new_child(&scope.bindings);
    for param in params.iter_mut() {
        if let Some(ty) = &mut param.ty {
            resolve_ty_names(ty, &mscope.bindings);
        }
        mscope.define(param.name.clone(), param.id, DefKind::Param);
    }
    if let Some(ret) = return_type {
        resolve_ty_names(ret, &mscope.bindings);
    }
    if let Some(body) = body {
        resolve_block_names(body, &mscope, diagnostics);
    }
}

/// Resolve type parameter names within a `HirTy`.
/// Converts `HirTy::Named("T")` to `HirTy::TypeParam(...)` when `T` is in scope
/// as a `DefKind::TypeParam`. Also resolves names inside `HirTy::Instance` args.
fn resolve_ty_names(ty: &mut HirTy, bindings: &HashMap<String, (DefId, DefKind)>) {
    match ty {
        HirTy::Named(nr) => {
            let text = match nr {
                NameRef::Resolved(_) => return,
                NameRef::Unresolved(u) => u.text.clone(),
            };
            if let Some((def_id, kind)) = bindings.get(&text) {
                if *kind == DefKind::TypeParam {
                    *ty = HirTy::TypeParam(HirTypeParam {
                        id: *def_id,
                        name: text,
                        bounds: Vec::new(), // Bounds are on the declaration, not the use.
                    });
                } else {
                    *nr = NameRef::resolved(*def_id, &text);
                }
            }
            // If not in bindings, leave as Unresolved (type checker or builtins handle it).
        }
        HirTy::Instance(inst) => {
            // Resolve the base name.
            let text = match &inst.name {
                NameRef::Resolved(_) => String::new(),
                NameRef::Unresolved(u) => u.text.clone(),
            };
            if !text.is_empty() {
                if let Some((def_id, _)) = bindings.get(&text) {
                    inst.name = NameRef::resolved(*def_id, &text);
                }
            }
            // Recursively resolve type args.
            for arg in &mut inst.args {
                resolve_ty_names(arg, bindings);
            }
        }
        HirTy::Tuple(elems) => {
            for elem in elems {
                resolve_ty_names(elem, bindings);
            }
        }
        HirTy::Fn(f) => {
            for param in &mut f.params {
                resolve_ty_names(param, bindings);
            }
            resolve_ty_names(&mut f.return_type, bindings);
        }
        HirTy::TypeParam(_) | HirTy::Unit | HirTy::Error => {}
    }
}

fn resolve_block_names(
    block: &mut Block,
    parent_scope: &Scope,
    diagnostics: &mut Vec<HirDiagnostic>,
) {
    let mut scope = Scope::new_child(&parent_scope.bindings);
    for stmt in &mut block.stmts {
        resolve_stmt_names(stmt, &mut scope, diagnostics);
    }
    if let Some(tail) = &mut block.tail {
        resolve_expr_names(tail, &mut scope, diagnostics);
    }
}

fn resolve_stmt_names(stmt: &mut Stmt, scope: &mut Scope, diagnostics: &mut Vec<HirDiagnostic>) {
    match stmt {
        Stmt::ValStmt(s) => {
            resolve_expr_names(&mut s.value, scope, diagnostics);
            define_pattern_bindings(&mut s.pattern, scope, diagnostics);
        }
        Stmt::VarStmt(s) => {
            resolve_expr_names(&mut s.value, scope, diagnostics);
            define_pattern_bindings(&mut s.pattern, scope, diagnostics);
        }
        Stmt::ExprStmt(s) => {
            resolve_expr_names(&mut s.expr, scope, diagnostics);
        }
        Stmt::ReturnStmt(s) => {
            if let Some(v) = &mut s.value {
                resolve_expr_names(v, scope, diagnostics);
            }
        }
        Stmt::BreakStmt(s) => {
            if let Some(v) = &mut s.value {
                resolve_expr_names(v, scope, diagnostics);
            }
        }
        Stmt::ContinueStmt(_) => {}
    }
}

fn define_pattern_bindings(
    pat: &mut Pattern,
    scope: &mut Scope,
    diagnostics: &mut Vec<HirDiagnostic>,
) {
    match pat {
        Pattern::Ident(p) => {
            if scope.define(p.name.clone(), p.id, DefKind::Local) {
                diagnostics.push(HirDiagnostic::DuplicateDefinition {
                    name: p.name.clone(),
                    span: Span { lo: 0, hi: 0 },
                });
            }
            p.binding = Some(p.id);
        }
        Pattern::Wildcard(_) | Pattern::Literal(_) | Pattern::Range(_) => {}
        Pattern::TupleStruct(ts) => {
            resolve_name_ref(&mut ts.path, &scope.bindings, diagnostics);
            for field in &mut ts.fields {
                define_pattern_bindings(field, scope, diagnostics);
            }
        }
        Pattern::Struct(sp) => {
            resolve_name_ref(&mut sp.path, &scope.bindings, diagnostics);
            for field in &mut sp.fields {
                define_pattern_bindings(&mut field.pattern, scope, diagnostics);
            }
        }
        Pattern::Or(op) => {
            for alt in &mut op.alternatives {
                define_pattern_bindings(alt, scope, diagnostics);
            }
        }
    }
}

fn resolve_expr_names(expr: &mut Expr, scope: &mut Scope, diagnostics: &mut Vec<HirDiagnostic>) {
    match expr {
        Expr::Lit(_) => {}
        Expr::Path(p) => {
            resolve_name_ref(&mut p.name_ref, &scope.bindings, diagnostics);
        }
        Expr::Bin(b) => {
            resolve_expr_names(&mut b.left, scope, diagnostics);
            resolve_expr_names(&mut b.right, scope, diagnostics);
        }
        Expr::Unary(u) => {
            resolve_expr_names(&mut u.operand, scope, diagnostics);
        }
        Expr::Call(c) => resolve_call_names(c, scope, diagnostics),
        Expr::MethodCall(m) => resolve_method_call_names(m, scope, diagnostics),
        Expr::Field(f) => {
            resolve_expr_names(&mut f.receiver, scope, diagnostics);
        }
        Expr::Index(i) => {
            resolve_expr_names(&mut i.base, scope, diagnostics);
            resolve_expr_names(&mut i.index, scope, diagnostics);
        }
        Expr::Block(b) => {
            resolve_block_names(b, scope, diagnostics);
        }
        Expr::If(i) => resolve_if_names(i, scope, diagnostics),
        Expr::Match(m) => resolve_match_names(m, scope, diagnostics),
        Expr::Loop(l) => resolve_loop_names(l, scope, diagnostics),
        Expr::StructLit(s) => resolve_struct_lit_names(s, scope, diagnostics),
        Expr::Assign(a) => {
            resolve_assign_target_names(&mut a.target, scope, diagnostics);
            resolve_expr_names(&mut a.value, scope, diagnostics);
        }
    }
}

fn resolve_call_names(c: &mut CallExpr, scope: &mut Scope, diagnostics: &mut Vec<HirDiagnostic>) {
    resolve_name_ref(&mut c.callee, &scope.bindings, diagnostics);
    for arg in &mut c.args {
        resolve_expr_names(arg, scope, diagnostics);
    }
}

fn resolve_method_call_names(
    m: &mut MethodCallExpr,
    scope: &mut Scope,
    diagnostics: &mut Vec<HirDiagnostic>,
) {
    resolve_expr_names(&mut m.receiver, scope, diagnostics);
    for arg in &mut m.args {
        resolve_expr_names(arg, scope, diagnostics);
    }
}

fn resolve_if_names(i: &mut IfExpr, scope: &mut Scope, diagnostics: &mut Vec<HirDiagnostic>) {
    resolve_expr_names(&mut i.condition, scope, diagnostics);
    resolve_block_names(&mut i.then_branch, scope, diagnostics);
    if let Some(els) = &mut i.else_branch {
        resolve_expr_names(els, scope, diagnostics);
    }
}

fn resolve_match_names(m: &mut MatchExpr, scope: &mut Scope, diagnostics: &mut Vec<HirDiagnostic>) {
    resolve_expr_names(&mut m.scrutinee, scope, diagnostics);
    for arm in &mut m.arms {
        let mut arm_scope = Scope::new_child(&scope.bindings);
        resolve_pattern_names(&mut arm.pattern, &arm_scope.bindings, diagnostics);
        define_pattern_bindings(&mut arm.pattern, &mut arm_scope, diagnostics);
        if let Some(g) = &mut arm.guard {
            resolve_expr_names(g, &mut arm_scope, diagnostics);
        }
        resolve_expr_names(&mut arm.body, &mut arm_scope, diagnostics);
    }
}

fn resolve_loop_names(l: &mut LoopExpr, scope: &mut Scope, diagnostics: &mut Vec<HirDiagnostic>) {
    match &mut l.kind {
        LoopKind::Infinite(body) => {
            resolve_block_names(body, scope, diagnostics);
        }
        LoopKind::Conditional { condition, body } => {
            resolve_expr_names(condition, scope, diagnostics);
            resolve_block_names(body, scope, diagnostics);
        }
        LoopKind::Iterator {
            binding,
            binding_id,
            iterable,
            body,
        } => {
            resolve_expr_names(iterable, scope, diagnostics);
            scope.define(binding.clone(), *binding_id, DefKind::Local);
            resolve_block_names(body, scope, diagnostics);
        }
    }
}

fn resolve_struct_lit_names(
    s: &mut StructLitExpr,
    scope: &mut Scope,
    diagnostics: &mut Vec<HirDiagnostic>,
) {
    resolve_name_ref(&mut s.type_name, &scope.bindings, diagnostics);
    for field in &mut s.fields {
        resolve_expr_names(&mut field.value, scope, diagnostics);
    }
}

fn resolve_assign_target_names(
    target: &mut AssignTarget,
    scope: &mut Scope,
    diagnostics: &mut Vec<HirDiagnostic>,
) {
    match target {
        AssignTarget::Name(nr) => {
            resolve_name_ref(nr, &scope.bindings, diagnostics);
        }
        AssignTarget::Field { receiver, field: _ } => {
            resolve_expr_names(receiver, scope, diagnostics);
        }
        AssignTarget::Index { base, index } => {
            resolve_expr_names(base, scope, diagnostics);
            resolve_expr_names(index, scope, diagnostics);
        }
    }
}

fn resolve_pattern_names(
    pat: &mut Pattern,
    bindings: &HashMap<String, (DefId, DefKind)>,
    diagnostics: &mut Vec<HirDiagnostic>,
) {
    match pat {
        Pattern::Wildcard(_) | Pattern::Literal(_) | Pattern::Range(_) => {}
        Pattern::Ident(_p) => {
            // Ident patterns introduce bindings (handled in define_pattern_bindings).
        }
        Pattern::TupleStruct(ts) => {
            resolve_name_ref(&mut ts.path, bindings, diagnostics);
            for field in &mut ts.fields {
                resolve_pattern_names(field, bindings, diagnostics);
            }
        }
        Pattern::Struct(sp) => {
            resolve_name_ref(&mut sp.path, bindings, diagnostics);
            for field in &mut sp.fields {
                resolve_pattern_names(&mut field.pattern, bindings, diagnostics);
            }
        }
        Pattern::Or(op) => {
            for alt in &mut op.alternatives {
                resolve_pattern_names(alt, bindings, diagnostics);
            }
        }
    }
}

/// Resolve a NameRef by looking it up in the given scope.
fn resolve_name_ref(
    nr: &mut NameRef,
    bindings: &HashMap<String, (DefId, DefKind)>,
    diagnostics: &mut Vec<HirDiagnostic>,
) {
    let text = match nr {
        NameRef::Resolved(_) => return,
        NameRef::Unresolved(u) => u.text.clone(),
    };

    if let Some((def_id, _kind)) = bindings.get(&text) {
        *nr = NameRef::Resolved(ResolvedName {
            def_id: *def_id,
            text,
        });
        return;
    }

    if let Some(def_id) = builtin_def_id(&text) {
        *nr = NameRef::Resolved(ResolvedName { def_id, text });
        return;
    }

    diagnostics.push(HirDiagnostic::UnresolvedName {
        name: text,
        span: Span { lo: 0, hi: 0 },
    });
}

/// Reserved HirId range for builtins. Real definitions start above this.
const BUILTIN_HIR_ID_START: usize = 1_000_000;

/// Built-in names that are always available, mapped to reserved HirIds.
fn builtin_def_id(name: &str) -> Option<DefId> {
    let idx = match name {
        "print" => 0,
        "println" => 1,
        "Int" => 2,
        "Float" => 3,
        "Bool" => 4,
        "String" => 5,
        "Unit" => 6,
        _ => return None,
    };
    Some(HirId(BUILTIN_HIR_ID_START + idx))
}

// ── Scope ──────────────────────────────────────────────────────────────────────

struct Scope {
    /// All bindings visible in this scope (own + inherited from parent).
    bindings: HashMap<String, (DefId, DefKind)>,
    /// Names defined in THIS scope only (not inherited).
    /// Used to detect same-scope redefinition, which is an error,
    /// while allowing shadowing of parent-scope names, which is allowed.
    own_names: HashSet<String>,
}

impl Scope {
    fn new_child(parent: &HashMap<String, (DefId, DefKind)>) -> Self {
        Scope {
            bindings: parent.clone(),
            own_names: HashSet::new(),
        }
    }

    /// Define a binding in this scope. Returns `true` if this is a same-scope
    /// redefinition (error), `false` if it's a new name or shadowing a parent name.
    fn define(&mut self, name: String, id: DefId, kind: DefKind) -> bool {
        let redefines_own = self.own_names.contains(&name);
        self.bindings.insert(name.clone(), (id, kind));
        self.own_names.insert(name);
        redefines_own
    }
}
