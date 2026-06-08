//! HIR snapshot serializer (`docs/hir-testing.md` §2). Pure function:
//! `&Hir → String`. This output is both the debug dump and the golden-test oracle.
//!
//! Kind names come from the `Display` impls on the enum types — never hardcoded
//! strings (enforced by `test_no_hardcoded_kind_labels`).

use crate::hir_types::*;

mod escape;
mod formatting;
mod types;
use types::{fmt_ty, fmt_ty_maybe};

mod patterns;
use patterns::serialize_pattern_inline;

// ── Public entry point ─────────────────────────────────────────────────────────

/// Serialize an HIR to the canonical dump format: one line per node, two-space
/// indentation per depth, HirIds shown, resolved names link to DefIds.
pub fn serialize(hir: &Hir) -> String {
    let mut out = String::new();
    for item in &hir.items {
        serialize_item(item, 0, &mut out);
    }
    out
}

// ── Items ──────────────────────────────────────────────────────────────────────

fn serialize_item(item: &Item, depth: usize, out: &mut String) {
    match item {
        Item::FnDef(f) => serialize_fn_def(f, depth, out),
        Item::StructDef(s) => serialize_struct_def(s, depth, out),
        Item::EnumDef(e) => serialize_enum_def(e, depth, out),
        Item::TraitDef(t) => serialize_trait_def(t, depth, out),
        Item::ImplDef(i) => serialize_impl_def(i, depth, out),
        Item::SubscriptDef(s) => serialize_subscript_def(s, depth, out),
        Item::UseItem(u) => serialize_use_item(u, depth, out),
        Item::ErrorSetDef(e) => fmt_error_set_def(e, depth, out),
    }
}

fn fmt_type_params(params: &[HirTypeParam]) -> String {
    if params.is_empty() {
        return String::new();
    }
    let inner = params
        .iter()
        .map(|tp| {
            if tp.bounds.is_empty() {
                tp.name.clone()
            } else {
                let bounds = tp
                    .bounds
                    .iter()
                    .map(|b| match &b.name {
                        NameRef::Resolved(r) => format!("{}→{}", r.text, r.def_id),
                        NameRef::Unresolved(u) => format!("{}→<unresolved>", u.text),
                    })
                    .collect::<Vec<_>>()
                    .join(" + ");
                format!("{}: {}", tp.name, bounds)
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("<{}>", inner)
}

/// Render a trait's supertrait list as `: A→.. + B→..`, or empty if none.
fn fmt_supertraits(supertraits: &[HirTraitBound]) -> String {
    if supertraits.is_empty() {
        return String::new();
    }
    let names = supertraits
        .iter()
        .map(|b| match &b.name {
            NameRef::Resolved(r) => format!("{}→{}", r.text, r.def_id),
            NameRef::Unresolved(u) => format!("{}→<unresolved>", u.text),
        })
        .collect::<Vec<_>>()
        .join(" + ");
    format!(": {names}")
}

fn serialize_fn_def(f: &FnDef, depth: usize, out: &mut String) {
    let type_params = fmt_type_params(&f.type_params);
    let params = f
        .params
        .iter()
        .map(|p| format!("{} {}: {}", p.convention, p.name, fmt_ty_maybe(&p.ty)))
        .collect::<Vec<_>>()
        .join(", ");
    let ret = fmt_ty_maybe(&f.return_type);
    indent(out, depth);
    out.push_str(&format!(
        "FnDef({}) name={}{} vis={} params=[{}] return={} {{\n",
        f.id, f.name, type_params, f.visibility, params, ret,
    ));
    serialize_block(&f.body, depth + 1, out);
    indent(out, depth);
    out.push_str("}\n");
}

fn serialize_struct_def(s: &StructDef, depth: usize, out: &mut String) {
    let type_params = fmt_type_params(&s.type_params);
    let fields = s
        .fields
        .iter()
        .map(|f| format!("{} {}: {}", f.visibility, f.name, fmt_ty(&f.ty)))
        .collect::<Vec<_>>()
        .join(", ");
    indent(out, depth);
    out.push_str(&format!(
        "StructDef({}) name={}{} vis={} fields=[{}]\n",
        s.id, s.name, type_params, s.visibility, fields,
    ));
}

fn serialize_trait_def(t: &TraitDef, depth: usize, out: &mut String) {
    let type_params = fmt_type_params(&t.type_params);
    let supertraits = fmt_supertraits(&t.supertraits);
    indent(out, depth);
    out.push_str(&format!(
        "TraitDef({}) name={}{}{} vis={} methods=[\n",
        t.id, t.name, type_params, supertraits, t.visibility,
    ));
    for method in &t.methods {
        let params = method
            .params
            .iter()
            .map(|p| format!("{} {}: {}", p.convention, p.name, fmt_ty_maybe(&p.ty)))
            .collect::<Vec<_>>()
            .join(", ");
        let ret = fmt_ty_maybe(&method.return_type);
        indent(out, depth + 1);
        if let Some(body) = &method.body {
            out.push_str(&format!(
                "Method({}) name={} params=[{}] return={} {{\n",
                method.id, method.name, params, ret,
            ));
            serialize_block(body, depth + 2, out);
            indent(out, depth + 1);
            out.push_str("}\n");
        } else {
            out.push_str(&format!(
                "Method({}) name={} params=[{}] return={}\n",
                method.id, method.name, params, ret,
            ));
        }
    }
    indent(out, depth);
    out.push_str("]\n");
}

fn serialize_impl_def(i: &ImplDef, depth: usize, out: &mut String) {
    let type_params = fmt_type_params(&i.type_params);
    let trait_part = match &i.trait_name {
        Some(nr) => format!("{} for ", fmt_name_ref(nr)),
        None => String::new(),
    };
    indent(out, depth);
    out.push_str(&format!(
        "ImplDef({}) {}{}{} methods=[\n",
        i.id,
        trait_part,
        fmt_name_ref(&i.type_name),
        type_params,
    ));
    for method in &i.methods {
        serialize_fn_def(method, depth + 1, out);
    }
    for sub in &i.subscripts {
        serialize_subscript_def(sub, depth + 1, out);
    }
    indent(out, depth);
    out.push_str("]\n");
}

fn serialize_subscript_def(s: &SubscriptDef, depth: usize, out: &mut String) {
    indent(out, depth);
    let kind = if s.is_setter { "set " } else { "" };
    out.push_str(&format!("SubscriptDef({}) {kind}", s.id));
    let params: Vec<String> = s.params.iter().map(|p| p.name.clone()).collect();
    out.push_str(&format!("({})", params.join(", ")));
    if let Some(ret) = &s.return_type {
        out.push_str(&format!(" -> {}", fmt_ty(ret)));
    }
    out.push_str(" body\n");
    serialize_block(&s.body, depth + 1, out);
}

fn serialize_use_item(u: &UseItem, depth: usize, out: &mut String) {
    indent(out, depth);
    out.push_str(&format!("UseItem({}) ", u.id));
    serialize_use_tree(&u.tree, out);
    out.push('\n');
}

fn serialize_use_tree(tree: &UseTree, out: &mut String) {
    out.push_str(&tree.path.join("::"));
    match &tree.kind {
        UseTreeKind::Single { rename } => {
            if let Some(alias) = rename {
                out.push_str(&format!(" as {alias}"));
            }
        }
        UseTreeKind::Group(trees) => {
            out.push('{');
            for (i, t) in trees.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                serialize_use_tree(t, out);
            }
            out.push('}');
        }
        UseTreeKind::Glob => {
            out.push_str("::*");
        }
    }
}

pub fn fmt_error_set_def(e: &ErrorSetDef, depth: usize, out: &mut String) {
    indent(out, depth);
    out.push_str(&format!(
        "ErrorSetDef({}) name={} vis={} vars=[",
        e.id, e.name, e.visibility
    ));
    for (i, v) in e.variants.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&format!("{}({})", v.name, v.id));
    }
    out.push_str("]\n");
}

fn fmt_name_ref(nr: &NameRef) -> String {
    match nr {
        NameRef::Resolved(r) => format!("{}→{}", r.text, r.def_id),
        NameRef::Unresolved(u) => format!("{}→<unresolved>", u.text),
    }
}

fn serialize_enum_def(e: &EnumDef, depth: usize, out: &mut String) {
    let type_params = fmt_type_params(&e.type_params);
    indent(out, depth);
    out.push_str(&format!(
        "EnumDef({}) name={}{} vis={} variants=[\n",
        e.id, e.name, type_params, e.visibility
    ));
    for v in &e.variants {
        let payload = if v.payload.is_empty() {
            String::new()
        } else {
            format!(
                "({})",
                v.payload.iter().map(fmt_ty).collect::<Vec<_>>().join(", ")
            )
        };
        indent(out, depth + 1);
        out.push_str(&format!("Variant({}) name={}{}\n", v.id, v.name, payload));
    }
    indent(out, depth);
    out.push_str("]\n");
}

// ── Statements ─────────────────────────────────────────────────────────────────

fn serialize_stmt(stmt: &Stmt, depth: usize, out: &mut String) {
    match stmt {
        Stmt::ValStmt(s) => {
            indent(out, depth);
            out.push_str(&format!("ValStmt({}) ", s.id));
            serialize_pattern_inline(&s.pattern, out);
            out.push_str(&format!(": {} = ", fmt_ty_maybe(&s.ty)));
            serialize_expr(&s.value, depth, out);
            out.push('\n');
        }
        Stmt::VarStmt(s) => {
            indent(out, depth);
            out.push_str(&format!("VarStmt({}) ", s.id));
            serialize_pattern_inline(&s.pattern, out);
            out.push_str(&format!(": {} = ", fmt_ty_maybe(&s.ty)));
            serialize_expr(&s.value, depth, out);
            out.push('\n');
        }
        Stmt::ExprStmt(s) => {
            indent(out, depth);
            out.push_str(&format!("ExprStmt({}) ", s.id));
            serialize_expr(&s.expr, depth, out);
            out.push('\n');
        }
        Stmt::ReturnStmt(s) => {
            indent(out, depth);
            out.push_str(&format!("ReturnStmt({})", s.id));
            if let Some(v) = &s.value {
                out.push(' ');
                serialize_expr(v, depth, out);
            }
            out.push('\n');
        }
        Stmt::BreakStmt(s) => {
            indent(out, depth);
            out.push_str(&format!("BreakStmt({})", s.id));
            if let Some(v) = &s.value {
                out.push(' ');
                serialize_expr(v, depth, out);
            }
            out.push('\n');
        }
        Stmt::ContinueStmt(s) => {
            indent(out, depth);
            out.push_str(&format!("ContinueStmt({})\n", s.id));
        }
        Stmt::YieldStmt(s) => {
            indent(out, depth);
            out.push_str(&format!("YieldStmt({}) ", s.id));
            serialize_expr(&s.value, depth, out);
            out.push('\n');
        }
    }
}

// ── Expressions ────────────────────────────────────────────────────────────────

fn serialize_expr(expr: &Expr, depth: usize, out: &mut String) {
    match expr {
        Expr::Lit(e) => {
            out.push_str(&format!("Lit({}) {}", e.id, fmt_lit(&e.kind)));
        }
        Expr::Path(e) => {
            out.push_str(&format!("Path({}) ", e.id));
            serialize_name_ref(&e.name_ref, out);
        }
        Expr::Bin(e) => serialize_bin_expr(e, depth, out),
        Expr::Unary(e) => serialize_unary_expr(e, depth, out),
        Expr::Call(e) => serialize_call_expr(e, depth, out),
        Expr::MethodCall(e) => serialize_method_call_expr(e, depth, out),
        Expr::Field(e) => serialize_field_expr(e, depth, out),
        Expr::Index(e) => serialize_index_expr(e, depth, out),
        Expr::Block(b) => serialize_block_inline(b, depth, out),
        Expr::If(e) => serialize_if_expr(e, depth, out),
        Expr::Match(e) => serialize_match_expr(e, depth, out),
        Expr::Loop(e) => serialize_loop_expr(e, depth, out),
        Expr::StructLit(e) => serialize_struct_lit_expr(e, depth, out),
        Expr::Assign(e) => serialize_assign_expr(e, depth, out),
        Expr::ListLit(e) => serialize_list_lit_expr(e, depth, out),
        Expr::Try(e) => {
            let label = if e.is_option { "OptionTry" } else { "Try" };
            out.push_str(&format!("{label}({})(", e.id));
            serialize_expr(&e.expr, depth, out);
            out.push(')');
        }
        Expr::Catch(e) => {
            out.push_str(&format!("Catch({})", e.id));
            if let Some(ref name) = e.error_binding {
                out.push_str(&format!(" |{name}|"));
            }
            out.push('(');
            serialize_expr(&e.expr, depth, out);
            out.push_str(", ");
            serialize_expr(&e.fallback, depth, out);
            out.push(')');
        }
        Expr::Else(e) => {
            out.push_str(&format!("Else({})(", e.id));
            serialize_expr(&e.expr, depth, out);
            out.push_str(", ");
            serialize_expr(&e.fallback, depth, out);
            out.push(')');
        }
    }
}

fn serialize_bin_expr(e: &BinExpr, depth: usize, out: &mut String) {
    out.push_str(&format!("Bin({}) {}(", e.id, e.op));
    serialize_expr(&e.left, depth, out);
    out.push_str(", ");
    serialize_expr(&e.right, depth, out);
    out.push(')');
}

fn serialize_unary_expr(e: &UnaryExpr, depth: usize, out: &mut String) {
    out.push_str(&format!("Unary({}) {}(", e.id, e.op));
    serialize_expr(&e.operand, depth, out);
    out.push(')');
}

fn serialize_call_expr(e: &CallExpr, depth: usize, out: &mut String) {
    out.push_str(&format!("Call({}) ", e.id));
    if let Some(qualifier) = &e.qualifier {
        out.push_str(qualifier);
        out.push_str("::");
    }
    serialize_name_ref(&e.callee, out);
    out.push('(');
    for (i, arg) in e.args.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        serialize_expr(arg, depth, out);
    }
    out.push(')');
}

fn serialize_method_call_expr(e: &MethodCallExpr, depth: usize, out: &mut String) {
    serialize_expr(&e.receiver, depth, out);
    out.push_str(&format!(".Method({}) {}(", e.id, e.method));
    for (i, arg) in e.args.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        serialize_expr(arg, depth, out);
    }
    out.push(')');
}

fn serialize_field_expr(e: &FieldExpr, depth: usize, out: &mut String) {
    serialize_expr(&e.receiver, depth, out);
    out.push_str(&format!(".Field({}) {}", e.id, e.field));
}

fn serialize_index_expr(e: &IndexExpr, depth: usize, out: &mut String) {
    serialize_expr(&e.base, depth, out);
    out.push('[');
    for (i, idx) in e.indices.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        serialize_expr(idx, depth, out);
    }
    out.push(']');
}

fn serialize_if_expr(e: &IfExpr, depth: usize, out: &mut String) {
    out.push_str(&format!("If({}) ", e.id));
    serialize_expr(&e.condition, depth, out);
    out.push(' ');
    serialize_block_inline(&e.then_branch, depth, out);
    if let Some(els) = &e.else_branch {
        out.push_str(" else ");
        serialize_expr(els, depth, out);
    }
}

fn serialize_match_expr(e: &MatchExpr, depth: usize, out: &mut String) {
    out.push_str(&format!("Match({}) ", e.id));
    serialize_expr(&e.scrutinee, depth, out);
    out.push_str(" {\n");
    for arm in &e.arms {
        indent(out, depth + 1);
        serialize_pattern_inline(&arm.pattern, out);
        if let Some(g) = &arm.guard {
            out.push_str(" if ");
            serialize_expr(g, depth + 1, out);
        }
        out.push_str(" => ");
        serialize_expr(&arm.body, depth + 1, out);
        out.push('\n');
    }
    indent(out, depth);
    out.push('}');
}

fn serialize_loop_expr(e: &LoopExpr, depth: usize, out: &mut String) {
    match &e.kind {
        LoopKind::Infinite(body) => {
            out.push_str(&format!("Loop({}) ", e.id));
            serialize_block_inline(body, depth, out);
        }
        LoopKind::Conditional { condition, body } => {
            out.push_str(&format!("LoopCond({}) ", e.id));
            serialize_expr(condition, depth, out);
            out.push(' ');
            serialize_block_inline(body, depth, out);
        }
        LoopKind::Iterator {
            binding,
            binding_id,
            iterable,
            body,
        } => {
            out.push_str(&format!(
                "LoopIter({}) {}:{} in ",
                e.id, binding_id, binding,
            ));
            serialize_expr(iterable, depth, out);
            out.push(' ');
            serialize_block_inline(body, depth, out);
        }
    }
}

fn serialize_struct_lit_expr(e: &StructLitExpr, depth: usize, out: &mut String) {
    out.push_str(&format!("StructLit({}) ", e.id));
    serialize_name_ref(&e.type_name, out);
    out.push_str(" { ");
    for (i, f) in e.fields.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&f.name);
        out.push_str(": ");
        serialize_expr(&f.value, depth, out);
    }
    out.push_str(" }");
}

fn serialize_list_lit_expr(e: &ListLitExpr, depth: usize, out: &mut String) {
    out.push_str(&format!("ListLit({}) [", e.id));
    for (i, elem) in e.elements.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        serialize_expr(elem, depth, out);
    }
    out.push(']');
}

fn serialize_assign_expr(e: &AssignExpr, depth: usize, out: &mut String) {
    serialize_assign_target(&e.target, depth, out);
    out.push_str(&format!(" {} ", e.op));
    serialize_expr(&e.value, depth, out);
}

fn serialize_block(block: &Block, depth: usize, out: &mut String) {
    for stmt in &block.stmts {
        serialize_stmt(stmt, depth, out);
    }
    if let Some(tail) = &block.tail {
        indent(out, depth);
        out.push_str("tail: ");
        serialize_expr(tail, depth, out);
        out.push('\n');
    }
}

fn serialize_block_inline(block: &Block, depth: usize, out: &mut String) {
    out.push_str(&format!("Block({}) {{\n", block.id));
    serialize_block(block, depth + 1, out);
    indent(out, depth);
    out.push('}');
}

fn serialize_name_ref(nr: &NameRef, out: &mut String) {
    match nr {
        NameRef::Resolved(r) => out.push_str(&format!("{}→{}", r.text, r.def_id)),
        NameRef::Unresolved(u) => out.push_str(&format!("{}→<unresolved>", u.text)),
    }
}

fn serialize_assign_target(target: &AssignTarget, depth: usize, out: &mut String) {
    match target {
        AssignTarget::Name(nr) => serialize_name_ref(nr, out),
        AssignTarget::Field { receiver, field } => {
            serialize_expr(receiver, depth, out);
            out.push('.');
            out.push_str(field);
        }
        AssignTarget::Index { base, indices } => {
            serialize_expr(base, depth, out);
            out.push('[');
            for (i, idx) in indices.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                serialize_expr(idx, depth, out);
            }
            out.push(']');
        }
    }
}

use formatting::{fmt_lit, indent};
