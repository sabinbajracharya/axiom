//! Expression and pattern serialization for the THIR dump.

use super::helpers::{fmt_lit, indent};
use super::{serialize_block, serialize_name_ref, serialize_stmt};
use crate::thir::Thir;
use hir::*;

pub(super) fn serialize_expr(expr: &Expr, depth: usize, thir: &Thir, out: &mut String) {
    let ty = thir.types.get(&expr.id());
    let type_ann = ty.map(|t| format!(" : {t}")).unwrap_or_default();
    indent(out, depth);

    match expr {
        Expr::Lit(e) => {
            out.push_str(&format!("Lit({}) {}{}", e.id, fmt_lit(&e.kind), type_ann));
        }
        Expr::Path(e) => {
            out.push_str(&format!("Path({}) ", e.id));
            serialize_name_ref(&e.name_ref, out);
            out.push_str(&type_ann);
        }
        Expr::Bin(e) => serialize_bin_expr(e, depth, &type_ann, thir, out),
        Expr::Unary(e) => serialize_unary_expr(e, depth, &type_ann, thir, out),
        Expr::Call(e) => serialize_call_expr(e, depth, &type_ann, thir, out),
        Expr::MethodCall(e) => serialize_method_call_expr(e, depth, &type_ann, thir, out),
        Expr::Field(e) => serialize_field_expr(e, depth, &type_ann, thir, out),
        Expr::Index(e) => serialize_index_expr(e, depth, &type_ann, thir, out),
        Expr::Block(b) => serialize_block_expr(b, depth, &type_ann, thir, out),
        Expr::If(e) => serialize_if_expr(e, depth, &type_ann, thir, out),
        Expr::Match(e) => serialize_match_expr(e, depth, &type_ann, thir, out),
        Expr::Loop(e) => serialize_loop_expr(e, depth, &type_ann, thir, out),
        Expr::StructLit(e) => serialize_struct_lit_expr(e, depth, &type_ann, thir, out),
        Expr::Assign(e) => serialize_assign_expr(e, depth, &type_ann, thir, out),
        Expr::ListLit(e) => serialize_list_lit_expr(e, depth, &type_ann, thir, out),
    }
}

fn serialize_bin_expr(e: &BinExpr, depth: usize, type_ann: &str, thir: &Thir, out: &mut String) {
    out.push_str(&format!("Bin({}) op={}{}", e.id, e.op, type_ann));
    out.push('\n');
    serialize_expr(&e.left, depth + 1, thir, out);
    out.push('\n');
    serialize_expr(&e.right, depth + 1, thir, out);
}

fn serialize_unary_expr(
    e: &UnaryExpr,
    depth: usize,
    type_ann: &str,
    thir: &Thir,
    out: &mut String,
) {
    out.push_str(&format!("Unary({}) {}{}", e.id, e.op, type_ann));
    out.push('\n');
    serialize_expr(&e.operand, depth + 1, thir, out);
}

fn serialize_call_expr(e: &CallExpr, depth: usize, type_ann: &str, thir: &Thir, out: &mut String) {
    out.push_str(&format!("Call({}) callee=", e.id));
    serialize_name_ref(&e.callee, out);
    out.push_str(type_ann);
    out.push('\n');
    for arg in &e.args {
        serialize_expr(arg, depth + 1, thir, out);
        out.push('\n');
    }
}

fn serialize_method_call_expr(
    e: &MethodCallExpr,
    depth: usize,
    type_ann: &str,
    thir: &Thir,
    out: &mut String,
) {
    out.push_str(&format!(
        "MethodCall({}) method={}{}",
        e.id, e.method, type_ann
    ));
    out.push('\n');
    serialize_expr(&e.receiver, depth + 1, thir, out);
    out.push('\n');
    for arg in &e.args {
        serialize_expr(arg, depth + 1, thir, out);
        out.push('\n');
    }
}

fn serialize_field_expr(
    e: &FieldExpr,
    depth: usize,
    type_ann: &str,
    thir: &Thir,
    out: &mut String,
) {
    out.push_str(&format!("Field({}) {}{}", e.id, e.field, type_ann));
    out.push('\n');
    serialize_expr(&e.receiver, depth + 1, thir, out);
}

fn serialize_index_expr(
    e: &IndexExpr,
    depth: usize,
    type_ann: &str,
    thir: &Thir,
    out: &mut String,
) {
    out.push_str(&format!("Index({}){}", e.id, type_ann));
    out.push('\n');
    serialize_expr(&e.base, depth + 1, thir, out);
    for idx in &e.indices {
        out.push('\n');
        serialize_expr(idx, depth + 1, thir, out);
    }
}

fn serialize_block_expr(b: &Block, depth: usize, type_ann: &str, thir: &Thir, out: &mut String) {
    out.push_str(&format!("Block({}){}", b.id, type_ann));
    out.push('\n');
    for stmt in &b.stmts {
        serialize_stmt(stmt, depth + 1, thir, out);
    }
    if let Some(tail) = &b.tail {
        serialize_expr(tail, depth + 1, thir, out);
        out.push('\n');
    }
}

fn serialize_if_expr(e: &IfExpr, depth: usize, type_ann: &str, thir: &Thir, out: &mut String) {
    out.push_str(&format!("If({}){}", e.id, type_ann));
    out.push('\n');
    serialize_expr(&e.condition, depth + 1, thir, out);
    out.push('\n');
    serialize_block_inline(&e.then_branch, depth + 1, thir, out);
    if let Some(els) = &e.else_branch {
        indent(out, depth + 1);
        out.push_str("else");
        out.push('\n');
        serialize_expr(els, depth + 2, thir, out);
        out.push('\n');
    }
}

fn serialize_match_expr(
    e: &MatchExpr,
    depth: usize,
    type_ann: &str,
    thir: &Thir,
    out: &mut String,
) {
    out.push_str(&format!("Match({}){}", e.id, type_ann));
    out.push('\n');
    serialize_expr(&e.scrutinee, depth + 1, thir, out);
    out.push('\n');
    for arm in &e.arms {
        indent(out, depth + 1);
        out.push_str("Arm pattern=");
        serialize_pattern_inline(&arm.pattern, thir, out);
        out.push('\n');
        serialize_expr(&arm.body, depth + 2, thir, out);
        out.push('\n');
    }
}

fn serialize_loop_expr(e: &LoopExpr, depth: usize, type_ann: &str, thir: &Thir, out: &mut String) {
    match &e.kind {
        LoopKind::Infinite(body) => {
            out.push_str(&format!("Loop({}){}", e.id, type_ann));
            out.push('\n');
            serialize_block(body, depth + 1, thir, out);
        }
        LoopKind::Conditional { condition, body } => {
            out.push_str(&format!("LoopCond({}){}", e.id, type_ann));
            out.push('\n');
            serialize_expr(condition, depth + 1, thir, out);
            out.push('\n');
            serialize_block(body, depth + 1, thir, out);
        }
        LoopKind::Iterator {
            binding,
            binding_id,
            iterable,
            body,
        } => {
            out.push_str(&format!(
                "LoopIter({}) {}:{} in ",
                e.id, binding_id, binding
            ));
            out.push_str(type_ann);
            out.push('\n');
            serialize_expr(iterable, depth + 1, thir, out);
            out.push('\n');
            serialize_block(body, depth + 1, thir, out);
        }
    }
}

fn serialize_struct_lit_expr(
    e: &StructLitExpr,
    depth: usize,
    type_ann: &str,
    thir: &Thir,
    out: &mut String,
) {
    out.push_str(&format!("StructLit({}) ", e.id));
    serialize_name_ref(&e.type_name, out);
    out.push_str(type_ann);
    out.push('\n');
    for f in &e.fields {
        indent(out, depth + 1);
        out.push_str(&format!("{}: ", f.name));
        serialize_expr(&f.value, depth + 1, thir, out);
        out.push('\n');
    }
}

fn serialize_list_lit_expr(
    e: &ListLitExpr,
    depth: usize,
    type_ann: &str,
    thir: &Thir,
    out: &mut String,
) {
    out.push_str(&format!("ListLit({}){} [", e.id, type_ann));
    for (i, elem) in e.elements.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        serialize_expr(elem, depth, thir, out);
    }
    out.push(']');
}

fn serialize_assign_expr(
    e: &AssignExpr,
    depth: usize,
    type_ann: &str,
    thir: &Thir,
    out: &mut String,
) {
    out.push_str(&format!("Assign({}) {} ", e.id, e.op));
    out.push_str(type_ann);
    serialize_assign_target(&e.target, depth, thir, out);
    out.push_str(" = ");
    serialize_expr(&e.value, depth, thir, out);
}

fn serialize_block_inline(block: &Block, depth: usize, thir: &Thir, out: &mut String) {
    let block_type = thir
        .types
        .get(&block.id)
        .map(|t| format!(" : {t}"))
        .unwrap_or_default();
    indent(out, depth);
    out.push_str(&format!("Block({}) {{", block.id));
    out.push_str(&block_type);
    out.push('\n');
    for stmt in &block.stmts {
        serialize_stmt(stmt, depth + 1, thir, out);
    }
    if let Some(tail) = &block.tail {
        serialize_expr(tail, depth + 1, thir, out);
        out.push('\n');
    }
    indent(out, depth);
    out.push('}');
}

fn serialize_assign_target(target: &AssignTarget, depth: usize, thir: &Thir, out: &mut String) {
    match target {
        AssignTarget::Name(nr) => serialize_name_ref(nr, out),
        AssignTarget::Field { receiver, field } => {
            serialize_expr(receiver, depth, thir, out);
            out.push('.');
            out.push_str(field);
        }
        AssignTarget::Index { base, indices } => {
            serialize_expr(base, depth, thir, out);
            out.push('[');
            for (i, idx) in indices.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                serialize_expr(idx, depth, thir, out);
            }
            out.push(']');
        }
    }
}

// ── Patterns ──────────────────────────────────────────────────────────────────

pub(super) fn serialize_pattern_inline(pat: &Pattern, thir: &Thir, out: &mut String) {
    let type_ann = pattern_type_ann(pat, thir);
    match pat {
        Pattern::Wildcard(id) => {
            out.push_str(&format!("Wild({}){}", id, type_ann));
        }
        Pattern::Ident(p) => {
            let binding = p.binding.map(|b| format!("→{b}")).unwrap_or_default();
            out.push_str(&format!(
                "Ident({}) {}{}{}",
                p.id, p.name, binding, type_ann
            ));
        }
        Pattern::Literal(p) => {
            out.push_str(&format!("Lit({}) {}{}", p.id, fmt_lit(&p.kind), type_ann));
        }
        Pattern::TupleStruct(ts) => {
            serialize_pat_tuple_struct(ts, thir, &type_ann, out);
        }
        Pattern::Struct(sp) => {
            serialize_pat_struct(sp, thir, &type_ann, out);
        }
        Pattern::Or(op) => {
            serialize_pat_or(op, thir, &type_ann, out);
        }
        Pattern::Range(rp) => {
            serialize_pat_range(rp, &type_ann, out);
        }
    }
}

fn pattern_type_ann(pat: &Pattern, thir: &Thir) -> String {
    thir.types
        .get(&pat.id())
        .map(|t| format!(" : {t}"))
        .unwrap_or_default()
}

fn serialize_pat_tuple_struct(ts: &TupleStructPat, thir: &Thir, type_ann: &str, out: &mut String) {
    serialize_name_ref(&ts.path, out);
    out.push('(');
    for (i, f) in ts.fields.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        serialize_pattern_inline(f, thir, out);
    }
    out.push(')');
    out.push_str(type_ann);
}

fn serialize_pat_struct(sp: &StructPat, thir: &Thir, type_ann: &str, out: &mut String) {
    serialize_name_ref(&sp.path, out);
    out.push_str(" { ");
    for (i, f) in sp.fields.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&format!("{}: ", f.name));
        serialize_pattern_inline(&f.pattern, thir, out);
    }
    out.push_str(" }");
    out.push_str(type_ann);
}

fn serialize_pat_or(op: &OrPat, thir: &Thir, type_ann: &str, out: &mut String) {
    for (i, alt) in op.alternatives.iter().enumerate() {
        if i > 0 {
            out.push_str(" | ");
        }
        serialize_pattern_inline(alt, thir, out);
    }
    out.push_str(type_ann);
}

fn serialize_pat_range(rp: &RangePat, type_ann: &str, out: &mut String) {
    if let Some(s) = &rp.start {
        out.push_str(&fmt_lit(s));
    }
    out.push_str(if rp.inclusive { "..=" } else { ".." });
    if let Some(e) = &rp.end {
        out.push_str(&fmt_lit(e));
    }
    out.push_str(type_ann);
}
