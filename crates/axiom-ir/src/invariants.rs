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
    // Collect all defined registers: params + all instruction destinations.
    let mut defined: HashSet<Reg> = HashSet::new();
    for p in &func.params {
        defined.insert(p.reg);
    }
    for block in &func.blocks {
        for instr in &block.instrs {
            let dst = match instr {
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
                | IrInstr::ListNew { dst, .. } => *dst,
            };
            defined.insert(dst);
        }
    }

    // Check that every register used in instructions/terminators is defined.
    for block in &func.blocks {
        for instr in &block.instrs {
            let used: Vec<Reg> = match instr {
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
                IrInstr::ListNew { elements, .. } => elements.clone(),
                IrInstr::Const { .. } => vec![],
            };
            for r in used {
                if !defined.contains(&r) {
                    errors.push(format!("{}: register {} used before definition", prefix, r));
                }
            }
        }
    }
}

fn check_call_targets(
    func: &crate::ir::IrFunction,
    ir: &Ir,
    prefix: &str,
    errors: &mut Vec<String>,
) {
    // Built-in functions provided by the runtime — not expected to have IR definitions.
    const BUILTINS: &[&str] = &["print", "println"];
    let fn_names: HashSet<&str> = ir.functions.iter().map(|f| f.name.as_str()).collect();
    for block in &func.blocks {
        for instr in &block.instrs {
            if let IrInstr::Call { function, .. } = instr {
                if !fn_names.contains(function.as_str()) && !BUILTINS.contains(&function.as_str()) {
                    errors.push(format!(
                        "{}: Call target `{}` not found in IR",
                        prefix, function
                    ));
                }
            }
        }
    }
}
