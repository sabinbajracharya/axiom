//! HIR → IR lowering.
//!
//! Consumes the THIR (typed HIR) and produces a register-based IR with
//! explicit basic blocks and terminators.

use crate::ir::Ir;
use axiom_typeck::Thir;

pub(super) mod expr;
pub(super) mod helpers;
pub(super) mod item;
pub(super) mod stmt;

/// Lower a typed HIR program to IR.
pub fn lower(thir: &Thir) -> Ir {
    let mut ctx = LowerCtx::new(thir);
    for it in &thir.hir.items {
        item::lower_item(it, &mut ctx);
    }
    ctx.finish()
}

/// Shared state for the lowering process.
pub(super) struct LowerCtx<'a> {
    pub thir: &'a Thir,
    pub functions: Vec<crate::ir::IrFunction>,
}

impl<'a> LowerCtx<'a> {
    fn new(thir: &'a Thir) -> Self {
        Self {
            thir,
            functions: Vec::new(),
        }
    }

    fn finish(self) -> Ir {
        let entry = self
            .functions
            .iter()
            .position(|f| f.name == "main")
            .unwrap_or(0);
        Ir {
            functions: self.functions,
            entry,
        }
    }
}
