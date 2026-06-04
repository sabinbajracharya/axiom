//! Statement lowering: HIR Stmt → IR instructions.

use super::expr::lower_expr;
use super::helpers::FnLowerCtx;
use crate::ir::{IrConst, IrInstr, Reg};
use axiom_hir::{Block, Stmt};

/// Lower an HIR statement. Emits instructions into the current block.
pub(super) fn lower_stmt(stmt: &Stmt, ctx: &mut FnLowerCtx) {
    match stmt {
        Stmt::ValStmt(s) => {
            let val = lower_expr(&s.value, ctx);
            ctx.bind_pattern(&s.pattern, val);
        }
        Stmt::VarStmt(s) => {
            let val = lower_expr(&s.value, ctx);
            ctx.bind_pattern(&s.pattern, val);
        }
        Stmt::ExprStmt(s) => {
            lower_expr(&s.expr, ctx);
        }
        Stmt::ReturnStmt(s) => {
            let val = s.value.as_ref().map(|v| lower_expr(v, ctx));
            ctx.terminate(crate::ir::Terminator::Return(val));
        }
        Stmt::BreakStmt(s) => {
            let val = s.value.as_ref().map(|v| lower_expr(v, ctx));
            ctx.terminate(crate::ir::Terminator::Break { value: val });
        }
        Stmt::ContinueStmt(_) => {
            ctx.terminate(crate::ir::Terminator::Continue);
        }
    }
}

/// Lower a block and return the register holding its result value.
pub(super) fn lower_block_expr(block: &Block, ctx: &mut FnLowerCtx) -> Reg {
    for stmt in &block.stmts {
        lower_stmt(stmt, ctx);
    }
    match &block.tail {
        Some(tail) => lower_expr(tail, ctx),
        None => {
            let dst = ctx.fresh_reg();
            ctx.emit(IrInstr::Const {
                dst,
                value: IrConst::Unit,
            });
            dst
        }
    }
}
