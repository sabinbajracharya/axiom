//! Canonical THIR serialization. Pure function: `&Thir → String`.
//!
//! Per `docs/typeck-testing.md` §2: one node per line, two-space indentation,
//! every expression shows its type. This is both the debug dump and the
//! golden-test oracle. Deterministic, diff-friendly, LF-only.
//!
//! Kind labels come from the type system (never hardcoded strings).

mod exprs;
mod helpers;

use crate::mono::MonoResult;
use crate::thir::Thir;
use axiom_hir::*;
use helpers::{fmt_hir_ty, fmt_type_params, indent};

// Re-export for use by exprs module.
use exprs::{serialize_expr, serialize_pattern_inline};

/// Serialize a THIR to the canonical dump format.
///
/// If a `MonoResult` is provided, monomorphized instances are appended
/// after the main items with `generic_origin` annotations.
pub fn serialize(thir: &Thir, mono: Option<&MonoResult>) -> String {
    let mut out = String::new();
    for item in &thir.hir.items {
        serialize_item(item, 0, thir, &mut out);
    }
    // Append monomorphized instances.
    if let Some(mono) = mono {
        for inst in &mono.instances {
            serialize_mono_instance(inst, thir, &mut out);
        }
    }
    // Drop trailing newline for consistency.
    let trimmed = out.trim_end_matches('\n');
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}\n")
    }
}

fn serialize_mono_instance(inst: &crate::mono::MonoInstance, thir: &Thir, out: &mut String) {
    let type_args = inst
        .type_args
        .iter()
        .map(|t| t.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let params = inst
        .param_types
        .iter()
        .enumerate()
        .map(|(i, ty)| format!("let _{}: {}", i, ty))
        .collect::<Vec<_>>()
        .join(", ");
    let return_type = &inst.return_type;

    out.push_str(&format!("FnDef(name={}) vis=private", inst.name));
    out.push_str(&format!(
        " generic_origin={}<{}>",
        inst.original_name, type_args
    ));
    out.push_str(&format!(" params=[{}] return={}", params, return_type));

    // Show the function type.
    let param_tys = inst
        .param_types
        .iter()
        .map(|t| t.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    out.push_str(&format!(" : ({}) -> {}", param_tys, return_type));
    out.push('\n');

    // Try to find the original function body and serialize it.
    for item in &thir.hir.items {
        if let Item::FnDef(f) = item {
            if f.id == inst.original_id {
                serialize_block(&f.body, 1, thir, out);
                break;
            }
        }
    }
}

fn serialize_item(item: &Item, depth: usize, thir: &Thir, out: &mut String) {
    match item {
        Item::FnDef(f) => serialize_fn_def(f, depth, thir, out),
        Item::StructDef(s) => serialize_struct_def(s, depth, thir, out),
        Item::EnumDef(e) => serialize_enum_def(e, depth, thir, out),
        Item::TraitDef(t) => serialize_trait_def(t, depth, thir, out),
        Item::ImplDef(i) => serialize_impl_def(i, depth, thir, out),
    }
}

fn serialize_fn_def(f: &FnDef, depth: usize, thir: &Thir, out: &mut String) {
    let params = f
        .params
        .iter()
        .map(|p| {
            let param_ty = thir
                .types
                .get(&p.id)
                .map(|t| t.to_string())
                .unwrap_or_else(|| "_".to_string());
            format!("{} {}: {}", p.convention, p.name, param_ty)
        })
        .collect::<Vec<_>>()
        .join(", ");
    let return_type = f
        .return_type
        .as_ref()
        .map(fmt_hir_ty)
        .unwrap_or_else(|| "()".to_string());

    // The fn itself gets a function type.
    let fn_type = thir
        .types
        .get(&f.id)
        .map(|t| format!(" : {t}"))
        .unwrap_or_default();

    indent(out, depth);
    out.push_str(&format!(
        "FnDef({}) name={} vis={}",
        f.id, f.name, f.visibility,
    ));
    if !f.type_params.is_empty() {
        out.push_str(&format!(
            " type_params=[{}]",
            fmt_type_params(&f.type_params)
        ));
    }
    out.push_str(&format!(" params=[{}] return={}", params, return_type));
    out.push_str(&fn_type);
    out.push('\n');
    serialize_block(&f.body, depth + 1, thir, out);
}

fn serialize_struct_def(s: &StructDef, depth: usize, thir: &Thir, out: &mut String) {
    let fields = s
        .fields
        .iter()
        .map(|f| {
            let field_ty = thir
                .types
                .get(&f.id)
                .map(|t| t.to_string())
                .unwrap_or_else(|| fmt_hir_ty(&f.ty));
            format!("{} {}: {}", f.visibility, f.name, field_ty)
        })
        .collect::<Vec<_>>()
        .join(", ");
    indent(out, depth);
    out.push_str(&format!(
        "StructDef({}) name={} vis={}",
        s.id, s.name, s.visibility,
    ));
    if !s.type_params.is_empty() {
        out.push_str(&format!(
            " type_params=[{}]",
            fmt_type_params(&s.type_params)
        ));
    }
    out.push_str(&format!(" fields=[{}]\n", fields));
}

fn serialize_enum_def(e: &EnumDef, depth: usize, _thir: &Thir, out: &mut String) {
    indent(out, depth);
    out.push_str(&format!(
        "EnumDef({}) name={} vis={} variants=[\n",
        e.id, e.name, e.visibility,
    ));
    for v in &e.variants {
        let payload = if v.payload.is_empty() {
            String::new()
        } else {
            format!(
                "({})",
                v.payload
                    .iter()
                    .map(fmt_hir_ty)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        indent(out, depth + 1);
        out.push_str(&format!("Variant({}) name={}{}\n", v.id, v.name, payload));
    }
    indent(out, depth);
    out.push_str("]\n");
}

fn serialize_trait_def(t: &TraitDef, depth: usize, thir: &Thir, out: &mut String) {
    indent(out, depth);
    out.push_str(&format!(
        "TraitDef({}) name={} vis={}",
        t.id, t.name, t.visibility
    ));
    if !t.type_params.is_empty() {
        out.push_str(&format!(
            " type_params=[{}]",
            fmt_type_params(&t.type_params)
        ));
    }
    out.push('\n');
    for method in &t.methods {
        indent(out, depth + 1);
        let default_tag = if method.body.is_some() {
            " DEFAULT"
        } else {
            ""
        };
        out.push_str(&format!(
            "Method({}) name={}{}\n",
            method.id, method.name, default_tag
        ));
        for param in &method.params {
            let param_ty = thir
                .types
                .get(&param.id)
                .map(|t| t.to_string())
                .unwrap_or_else(|| {
                    param
                        .ty
                        .as_ref()
                        .map(fmt_hir_ty)
                        .unwrap_or_else(|| "_".to_string())
                });
            indent(out, depth + 2);
            out.push_str(&format!(
                "Param({}) {} : {}\n",
                param.id, param.name, param_ty
            ));
        }
        let ret_ty = method
            .return_type
            .as_ref()
            .map(fmt_hir_ty)
            .unwrap_or_else(|| "()".to_string());
        indent(out, depth + 2);
        out.push_str(&format!("Return : {}\n", ret_ty));
    }
}

fn serialize_impl_def(i: &ImplDef, depth: usize, thir: &Thir, out: &mut String) {
    indent(out, depth);
    let trait_part = i.trait_name.as_ref().map(name_ref_text).unwrap_or_default();
    let type_name = name_ref_text(&i.type_name);
    if trait_part.is_empty() {
        out.push_str(&format!("ImplDef({}) for={}", i.id, type_name));
    } else {
        out.push_str(&format!(
            "ImplDef({}) trait={} for={}",
            i.id, trait_part, type_name
        ));
    }
    if !i.type_params.is_empty() {
        out.push_str(&format!(
            " type_params=[{}]",
            fmt_type_params(&i.type_params)
        ));
    }
    out.push('\n');
    for method in &i.methods {
        serialize_fn_def(method, depth + 1, thir, out);
    }
}

fn name_ref_text(nr: &NameRef) -> String {
    match nr {
        NameRef::Resolved(r) => r.text.clone(),
        NameRef::Unresolved(u) => u.text.clone(),
    }
}

pub(super) fn serialize_block(block: &Block, depth: usize, thir: &Thir, out: &mut String) {
    let block_type = thir
        .types
        .get(&block.id)
        .map(|t| format!(" : {t}"))
        .unwrap_or_default();
    indent(out, depth);
    out.push_str(&format!(
        "Block({}) stmts=[{}] tail={}",
        block.id,
        block.stmts.len(),
        if block.tail.is_some() { "Some" } else { "None" }
    ));
    out.push_str(&block_type);
    out.push('\n');
    for stmt in &block.stmts {
        serialize_stmt(stmt, depth + 1, thir, out);
    }
    if let Some(tail) = &block.tail {
        serialize_expr(tail, depth + 1, thir, out);
        out.push('\n');
    }
}

fn stmt_type_annotation(id: HirId, thir: &Thir) -> String {
    thir.types
        .get(&id)
        .map(|t| format!(" : {t}"))
        .unwrap_or_default()
}

pub(super) fn serialize_stmt(stmt: &Stmt, depth: usize, thir: &Thir, out: &mut String) {
    match stmt {
        Stmt::ValStmt(s) => {
            let ty = stmt_type_annotation(s.id, thir);
            indent(out, depth);
            out.push_str(&format!("ValStmt({}){ty}\n", s.id));
            serialize_pattern_inline(&s.pattern, thir, out);
            out.push('\n');
            serialize_expr(&s.value, depth + 1, thir, out);
            out.push('\n');
        }
        Stmt::VarStmt(s) => {
            let ty = stmt_type_annotation(s.id, thir);
            indent(out, depth);
            out.push_str(&format!("VarStmt({}){ty}\n", s.id));
            serialize_pattern_inline(&s.pattern, thir, out);
            out.push('\n');
            serialize_expr(&s.value, depth + 1, thir, out);
            out.push('\n');
        }
        Stmt::ExprStmt(s) => {
            let ty = stmt_type_annotation(s.id, thir);
            indent(out, depth);
            out.push_str(&format!("ExprStmt({}){ty}\n", s.id));
            serialize_expr(&s.expr, depth + 1, thir, out);
            out.push('\n');
        }
        Stmt::ReturnStmt(s) => {
            let ty = stmt_type_annotation(s.id, thir);
            indent(out, depth);
            out.push_str(&format!("ReturnStmt({}){ty}", s.id));
            if let Some(v) = &s.value {
                out.push(' ');
                serialize_expr(v, depth + 1, thir, out);
            }
            out.push('\n');
        }
        Stmt::BreakStmt(s) => {
            let ty = stmt_type_annotation(s.id, thir);
            indent(out, depth);
            out.push_str(&format!("BreakStmt({}){ty}", s.id));
            if let Some(v) = &s.value {
                out.push(' ');
                serialize_expr(v, depth + 1, thir, out);
            }
            out.push('\n');
        }
        Stmt::ContinueStmt(s) => {
            let ty = stmt_type_annotation(s.id, thir);
            indent(out, depth);
            out.push_str(&format!("ContinueStmt({}){ty}\n", s.id));
        }
    }
}

pub(super) fn serialize_name_ref(nr: &NameRef, out: &mut String) {
    match nr {
        NameRef::Resolved(r) => out.push_str(&format!("{}→{}", r.text, r.def_id)),
        NameRef::Unresolved(u) => out.push_str(&format!("{}→<unresolved>", u.text)),
    }
}
