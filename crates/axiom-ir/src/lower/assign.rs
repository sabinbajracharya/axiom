//! Assignment lowering: HIR `AssignExpr` → IR `Copy`/`FieldSet`/`IndexSet`.
//!
//! Dispatches on the assignment target (name, field, or index). Compound
//! operators (`+=`, …) read the current value, apply the binary op, then store.

use super::expr::{lower_expr, unit_reg};
use super::helpers::FnLowerCtx;
use crate::ir::{IrInstr, Reg};
use axiom_hir::{AssignTarget, Expr};

/// Lower an assignment expression. Assignments evaluate to `Unit`.
pub(super) fn lower_assign(e: &axiom_hir::AssignExpr, ctx: &mut FnLowerCtx) -> Reg {
    match &e.target {
        AssignTarget::Name(nr) => lower_assign_name(nr, e, ctx),
        AssignTarget::Field { receiver, field } => lower_assign_field(receiver, field, e, ctx),
        AssignTarget::Index { base, index } => lower_assign_index(base, index, e, ctx),
    }
    unit_reg(ctx)
}

/// Map a compound assignment operator (`+=`, …) to its binary operator.
/// `Plain` (`=`) is never a compound op.
fn assign_binop(op: axiom_hir::AssignOp) -> axiom_hir::BinOp {
    match op {
        axiom_hir::AssignOp::Add => axiom_hir::BinOp::Add,
        axiom_hir::AssignOp::Sub => axiom_hir::BinOp::Sub,
        axiom_hir::AssignOp::Mul => axiom_hir::BinOp::Mul,
        axiom_hir::AssignOp::Div => axiom_hir::BinOp::Div,
        axiom_hir::AssignOp::Mod => axiom_hir::BinOp::Mod,
        axiom_hir::AssignOp::Plain => unreachable!("Plain is not a compound op"),
    }
}

/// `name op= value`: combine the current register value (for compound ops) and
/// copy the result back into the binding's register.
fn lower_assign_name(nr: &axiom_hir::NameRef, e: &axiom_hir::AssignExpr, ctx: &mut FnLowerCtx) {
    let value = lower_expr(&e.value, ctx);
    let def_id = match nr {
        axiom_hir::NameRef::Resolved(r) => Some(r.def_id),
        axiom_hir::NameRef::Unresolved(_) => None,
    };
    let dst = ctx.resolve_name(def_id);
    match e.op {
        axiom_hir::AssignOp::Plain => ctx.emit(IrInstr::Copy { dst, src: value }),
        compound => {
            let tmp = ctx.fresh_reg();
            ctx.emit(IrInstr::BinOp {
                dst: tmp,
                op: assign_binop(compound),
                lhs: dst,
                rhs: value,
            });
            ctx.emit(IrInstr::Copy { dst, src: tmp });
        }
    }
}

/// `receiver.field op= value`: emit a `FieldSet`, reading the current field
/// first for compound ops.
fn lower_assign_field(
    receiver: &Expr,
    field: &str,
    e: &axiom_hir::AssignExpr,
    ctx: &mut FnLowerCtx,
) {
    let base = lower_expr(receiver, ctx);
    let value = lower_expr(&e.value, ctx);
    let final_val = match e.op {
        axiom_hir::AssignOp::Plain => value,
        compound => {
            let cur = ctx.fresh_reg();
            ctx.emit(IrInstr::Field {
                dst: cur,
                base,
                field: field.to_string(),
            });
            let tmp = ctx.fresh_reg();
            ctx.emit(IrInstr::BinOp {
                dst: tmp,
                op: assign_binop(compound),
                lhs: cur,
                rhs: value,
            });
            tmp
        }
    };
    ctx.emit(IrInstr::FieldSet {
        base,
        field: field.to_string(),
        value: final_val,
    });
}

/// `base[index] op= value`: emit an `IndexSet`, reading the current element
/// first for compound ops.
fn lower_assign_index(base: &Expr, index: &Expr, e: &axiom_hir::AssignExpr, ctx: &mut FnLowerCtx) {
    let base_r = lower_expr(base, ctx);
    let idx_r = lower_expr(index, ctx);
    let value = lower_expr(&e.value, ctx);
    let final_val = match e.op {
        axiom_hir::AssignOp::Plain => value,
        compound => {
            let cur = ctx.fresh_reg();
            ctx.emit(IrInstr::Index {
                dst: cur,
                base: base_r,
                index: idx_r,
            });
            let tmp = ctx.fresh_reg();
            ctx.emit(IrInstr::BinOp {
                dst: tmp,
                op: assign_binop(compound),
                lhs: cur,
                rhs: value,
            });
            tmp
        }
    };
    ctx.emit(IrInstr::IndexSet {
        base: base_r,
        index: idx_r,
        value: final_val,
    });
}
