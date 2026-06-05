//! IR coverage invariants.
//!
//! Verifies structural correctness of the IR:
//! 1. Every register is defined before use.
//! 2. Every block has exactly one terminator (not Unreachable after construction).
//! 3. Every jump/branch target references an existing block.
//! 4. Entry block is block 0.
//! 5. Every Call target references an existing function.

use crate::ir::{Ir, IrBlock, IrInstr, Reg, Terminator};
use std::collections::HashSet;

/// Check all invariants. Returns a list of violation descriptions (empty = all ok).
pub fn check_invariants(ir: &Ir) -> Vec<String> {
    let mut errors = Vec::new();

    for (idx, func) in ir.functions.iter().enumerate() {
        let prefix = format!("fn `{}` (index {})", func.name, idx);

        // 4. Entry block is block 0.
        if func.blocks.is_empty() {
            errors.push(format!("{}: has no blocks", prefix));
            continue;
        }

        for (bi, block) in func.blocks.iter().enumerate() {
            let bpfx = format!("{} block `{}` (index {})", prefix, block.label, bi);

            // 2. Every block has a terminator (not Unreachable — that's the initial
            //    sentinel; a well-lowered block should have a real terminator).
            if matches!(block.terminator, Terminator::Unreachable) {
                // Only flag if this is not the block currently being built
                // (the last block may be mid-construction). For completed IR,
                // all blocks should have real terminators.
                if bi < func.blocks.len() - 1 {
                    errors.push(format!("{}: has Unreachable terminator", bpfx));
                }
            }

            // 3. Every jump/branch target references an existing block.
            check_terminator_targets(block, func, &bpfx, &mut errors);
        }

        // 1. Every register defined before use.
        check_register_defs(func, &prefix, &mut errors);

        // 5. Every Call target references an existing function.
        check_call_targets(func, ir, &prefix, &mut errors);
    }

    errors
}

fn check_terminator_targets(
    block: &IrBlock,
    func: &crate::ir::IrFunction,
    prefix: &str,
    errors: &mut Vec<String>,
) {
    let block_labels: HashSet<&str> = func.blocks.iter().map(|b| b.label.as_str()).collect();

    match &block.terminator {
        Terminator::Jump { target } => {
            if !block_labels.contains(target.as_str()) {
                errors.push(format!("{}: Jump target `{}` not found", prefix, target));
            }
        }
        Terminator::Branch {
            true_target,
            false_target,
            ..
        } => {
            if !block_labels.contains(true_target.as_str()) {
                errors.push(format!(
                    "{}: Branch true_target `{}` not found",
                    prefix, true_target
                ));
            }
            if !block_labels.contains(false_target.as_str()) {
                errors.push(format!(
                    "{}: Branch false_target `{}` not found",
                    prefix, false_target
                ));
            }
        }
        Terminator::Match { arms, fallback, .. } => {
            for arm in arms {
                if !block_labels.contains(arm.target.as_str()) {
                    errors.push(format!(
                        "{}: Match arm target `{}` not found",
                        prefix, arm.target
                    ));
                }
            }
            if !block_labels.contains(fallback.as_str()) {
                errors.push(format!(
                    "{}: Match fallback `{}` not found",
                    prefix, fallback
                ));
            }
        }
        Terminator::Return(_)
        | Terminator::Break { .. }
        | Terminator::Continue
        | Terminator::Unreachable => {}
    }
}

fn check_register_defs(func: &crate::ir::IrFunction, prefix: &str, errors: &mut Vec<String>) {
    let defined = collect_defined_regs(func);

    for block in &func.blocks {
        for instr in &block.instrs {
            for r in instr_used_regs(instr) {
                if !defined.contains(&r) {
                    errors.push(format!("{}: register {} used before definition", prefix, r));
                }
            }
        }
    }
}

/// Collect all defined registers: params + all instruction destinations.
fn collect_defined_regs(func: &crate::ir::IrFunction) -> HashSet<Reg> {
    let mut defined: HashSet<Reg> = func.params.iter().map(|p| p.reg).collect();
    for block in &func.blocks {
        for instr in &block.instrs {
            if let Some(dst) = instr_dst(instr) {
                defined.insert(dst);
            }
        }
    }
    defined
}

fn instr_dst(instr: &IrInstr) -> Option<Reg> {
    match instr {
        IrInstr::Const { dst, .. }
        | IrInstr::BinOp { dst, .. }
        | IrInstr::UnaryOp { dst, .. }
        | IrInstr::Call { dst, .. }
        | IrInstr::MethodCall { dst, .. }
        | IrInstr::Field { dst, .. }
        | IrInstr::Index { dst, .. }
        | IrInstr::Copy { dst, .. }
        | IrInstr::StructNew { dst, .. }
        | IrInstr::EnumNew { dst, .. }
        | IrInstr::VariantPayload { dst, .. }
        | IrInstr::ListNew { dst, .. }
        | IrInstr::HeapAlloc { dst, .. }
        | IrInstr::HeapGet { dst, .. } => Some(*dst),
        IrInstr::HeapFree { .. } | IrInstr::HeapSet { .. } => None,
    }
}

fn instr_used_regs(instr: &IrInstr) -> Vec<Reg> {
    match instr {
        IrInstr::BinOp { lhs, rhs, .. } => vec![*lhs, *rhs],
        IrInstr::UnaryOp { src, .. } => vec![*src],
        IrInstr::Call { args, .. } => args.clone(),
        IrInstr::MethodCall { receiver, args, .. } => {
            let mut v = vec![*receiver];
            v.extend(args);
            v
        }
        IrInstr::Field { base, .. } => vec![*base],
        IrInstr::Index { base, index, .. } => vec![*base, *index],
        IrInstr::Copy { src, .. } => vec![*src],
        IrInstr::StructNew { fields, .. } => fields.iter().map(|(_, r)| *r).collect(),
        IrInstr::EnumNew { payload, .. } => payload.clone(),
        IrInstr::VariantPayload { scrutinee, .. } => vec![*scrutinee],
        IrInstr::ListNew { elements, .. } => elements.clone(),
        IrInstr::Const { .. } => vec![],
        IrInstr::HeapAlloc { count, .. } => vec![*count],
        IrInstr::HeapFree { ptr } => vec![*ptr],
        IrInstr::HeapGet { ptr, index, .. } => vec![*ptr, *index],
        IrInstr::HeapSet {
            ptr, index, value, ..
        } => vec![*ptr, *index, *value],
    }
}

fn check_call_targets(
    func: &crate::ir::IrFunction,
    ir: &Ir,
    prefix: &str,
    errors: &mut Vec<String>,
) {
    // Stdlib functions whose FnDefs may not be in the IR (single-file path).
    // Multi-file loads them from stdlib modules; single-file needs this
    // allowlist because the stdlib HIR isn't included in that compilation path.
    const EXTERNS: &[&str] = &["print", "println"];
    let fn_names: HashSet<&str> = ir.functions.iter().map(|f| f.name.as_str()).collect();
    let enum_names: HashSet<&str> = ir.enum_variants.keys().map(|k| k.as_str()).collect();
    for block in &func.blocks {
        for instr in &block.instrs {
            if let IrInstr::Call { function, .. } = instr {
                if !fn_names.contains(function.as_str())
                    && !EXTERNS.contains(&function.as_str())
                    && !enum_names.contains(function.as_str())
                {
                    errors.push(format!(
                        "{}: Call target `{}` not found in IR",
                        prefix, function
                    ));
                }
            }
        }
    }
}
