//! HIR → IR lowering.
//!
//! Consumes the THIR (typed HIR) and produces a register-based IR with
//! explicit basic blocks and terminators. Generic functions are
//! monomorphized: each unique `(fn, type_args)` pair becomes a separate
//! concrete IR function with a mangled name (e.g., `id__Int`).

use std::collections::HashMap;

use crate::ir::Ir;
use axiom_typeck::mono::MonoResult;
use axiom_typeck::Thir;

pub(super) mod assign;
pub(super) mod expr;
pub(super) mod helpers;
pub(super) mod item;
pub(super) mod stmt;

/// Lower a typed HIR program to IR.
///
/// Generic functions are monomorphized using `mono`: each unique
/// `(fn, type_args)` pair produces a separate concrete IR function.
pub fn lower(thir: &Thir, mono: &MonoResult) -> Ir {
    let mut ctx = LowerCtx::new(thir, mono);
    for it in &thir.hir.items {
        item::lower_item(it, &mut ctx);
    }
    // Lower monomorphized instances (concrete specializations of generic fns).
    item::lower_mono_instances(&mut ctx);
    ctx.finish()
}

/// Shared state for the lowering process.
pub(super) struct LowerCtx<'a> {
    pub thir: &'a Thir,
    pub functions: Vec<crate::ir::IrFunction>,
    /// Maps enum variant name → (enum type name, payload count).
    pub enum_variants: HashMap<String, (String, usize)>,
    /// Monomorphized function lookup: fn_id → [(param_types, mangled_name)].
    pub mono_lookup: helpers::MonoLookup,
    /// Monomorphization result.
    pub mono: &'a MonoResult,
}

impl<'a> LowerCtx<'a> {
    fn new(thir: &'a Thir, mono: &'a MonoResult) -> Self {
        let mut mono_lookup: helpers::MonoLookup = HashMap::new();
        for inst in &mono.instances {
            mono_lookup
                .entry(inst.original_id)
                .or_default()
                .push((inst.param_types.clone(), inst.name.clone()));
        }
        Self {
            thir,
            functions: Vec::new(),
            enum_variants: HashMap::new(),
            mono_lookup,
            mono,
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
            enum_variants: self.enum_variants,
        }
    }
}
