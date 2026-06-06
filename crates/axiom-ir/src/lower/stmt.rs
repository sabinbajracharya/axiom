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
            // Evaluate any break value for its side effects (loops are
            // Unit-typed, so the value itself is discarded), then jump to the
            // innermost loop's exit. Resolving the target here — rather than at
            // runtime — keeps loop control flow as plain block jumps.
            let _ = s.value.as_ref().map(|v| lower_expr(v, ctx));
            if let Some((_, exit)) = ctx.current_loop() {
                ctx.terminate(crate::ir::Terminator::Jump { target: exit });
            }
        }
        Stmt::ContinueStmt(_) => {
            if let Some((head, _)) = ctx.current_loop() {
                ctx.terminate(crate::ir::Terminator::Jump { target: head });
            }
        }
        Stmt::YieldStmt(s) => {
            // For v0, yield evaluates its expression (same as expr stmt).
            lower_expr(&s.value, ctx);
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
