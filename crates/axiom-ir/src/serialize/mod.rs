//! IR serializer: produces a human-readable dump of the IR.

mod helpers;

use crate::ir::{Ir, IrBlock, IrConst, IrFunction, IrInstr, IrPattern, Terminator};
use helpers::{fmt_reg, indent};

/// Serialize an IR program to a deterministic string.
pub fn serialize(ir: &Ir) -> String {
    let mut out = String::new();
    for func in &ir.functions {
        serialize_fn(func, &mut out);
    }
    out
}

fn serialize_fn(func: &IrFunction, out: &mut String) {
    let params: Vec<String> = func
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, helpers::fmt_ty(&p.ty)))
        .collect();
    let ret = helpers::fmt_ty(&func.return_type);
    out.push_str(&format!(
        "fn {}({}) -> {} {{\n",
        func.name,
        params.join(", "),
        ret
    ));

    if let Some(origin) = &func.generic_origin {
        out.push_str(&format!(
            "  [generic_origin: {}, concrete_args: [{}]]\n",
            origin.generic_name,
            origin
                .concrete_args
                .iter()
                .map(helpers::fmt_ty)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    for block in &func.blocks {
        serialize_block(block, out);
    }

    out.push_str("}\n\n");
}

fn serialize_block(block: &IrBlock, out: &mut String) {
    out.push_str(&format!("  {}:\n", block.label));
    for instr in &block.instrs {
        serialize_instr(instr, out);
    }
    serialize_terminator(&block.terminator, out);
}

fn serialize_instr(instr: &IrInstr, out: &mut String) {
    let line = match instr {
        IrInstr::Const { dst, value } => {
            format!("{} = Const {}", fmt_reg(*dst), fmt_const(value))
        }
        IrInstr::BinOp { dst, op, lhs, rhs } => {
            format!(
                "{} = BinOp {} {} {}",
                fmt_reg(*dst),
                op,
                fmt_reg(*lhs),
                fmt_reg(*rhs)
            )
        }
        IrInstr::UnaryOp { dst, op, src } => {
            format!("{} = UnaryOp {} {}", fmt_reg(*dst), op, fmt_reg(*src))
        }
        IrInstr::Field { dst, base, field } => {
            format!("{} = Field {} {}", fmt_reg(*dst), fmt_reg(*base), field)
        }
        IrInstr::Index { dst, base, index } => {
            format!(
                "{} = Index {} {}",
                fmt_reg(*dst),
                fmt_reg(*base),
                fmt_reg(*index)
            )
        }
        IrInstr::Copy { dst, src } => {
            format!("{} = Copy {}", fmt_reg(*dst), fmt_reg(*src))
        }
        IrInstr::HeapAlloc { dst, count } => {
            format!("{} = HeapAlloc {}", fmt_reg(*dst), fmt_reg(*count))
        }
        IrInstr::HeapGet { dst, ptr, index } => {
            format!(
                "{} = HeapGet {} {}",
                fmt_reg(*dst),
                fmt_reg(*ptr),
                fmt_reg(*index)
            )
        }
        _ => serialize_instr_complex(instr),
    };
    out.push_str(&format!("{}{}\n", indent(1), line));
}

fn serialize_instr_complex(instr: &IrInstr) -> String {
    match instr {
        IrInstr::Call {
            dst,
            function,
            args,
        } => format!("{} = Call {} [{}]", fmt_reg(*dst), function, fmt_regs(args)),
        IrInstr::MethodCall {
            dst,
            receiver,
            method,
            args,
        } => format!(
            "{} = MethodCall {} {} [{}]",
            fmt_reg(*dst),
            fmt_reg(*receiver),
            method,
            fmt_regs(args)
        ),
        IrInstr::StructNew {
            dst,
            type_name,
            fields,
        } => {
            let f: Vec<String> = fields
                .iter()
                .map(|(n, r)| format!("{}: {}", n, fmt_reg(*r)))
                .collect();
            format!(
                "{} = StructNew {} {{{}}}",
                fmt_reg(*dst),
                type_name,
                f.join(", ")
            )
        }
        IrInstr::EnumNew {
            dst,
            type_name,
            variant,
            payload,
        } => format!(
            "{} = EnumNew {}.{}({})",
            fmt_reg(*dst),
            type_name,
            variant,
            fmt_regs(payload)
        ),
        IrInstr::ListNew { dst, elements } => {
            format!("{} = ListNew [{}]", fmt_reg(*dst), fmt_regs(elements))
        }
        IrInstr::HeapFree { ptr } => format!("HeapFree {}", fmt_reg(*ptr)),
        IrInstr::HeapSet { ptr, index, value } => format!(
            "HeapSet {} {} {}",
            fmt_reg(*ptr),
            fmt_reg(*index),
            fmt_reg(*value)
        ),
        _ => unreachable!("non-complex instruction passed to serialize_instr_complex"),
    }
}

fn serialize_terminator(term: &Terminator, out: &mut String) {
    let line = match term {
        Terminator::Return(Some(r)) => format!("Return {}", fmt_reg(*r)),
        Terminator::Return(None) => "Return Unit".to_string(),
        Terminator::Jump { target } => format!("Jump {}", target),
        Terminator::Branch {
            cond,
            true_target,
            false_target,
        } => {
            format!("Branch {} {} {}", fmt_reg(*cond), true_target, false_target)
        }
        Terminator::Match {
            scrutinee,
            arms,
            fallback,
        } => {
            let arm_strs: Vec<String> = arms
                .iter()
                .map(|a| format!("{} -> {}", fmt_pattern(&a.pattern), a.target))
                .collect();
            format!(
                "Match {} [{}] fallback {}",
                fmt_reg(*scrutinee),
                arm_strs.join(", "),
                fallback
            )
        }
        Terminator::Break { value } => match value {
            Some(v) => format!("Break {}", fmt_reg(*v)),
            None => "Break".to_string(),
        },
        Terminator::Continue => "Continue".to_string(),
        Terminator::Unreachable => "Unreachable".to_string(),
    };
    out.push_str(&format!("{}{}\n", indent(1), line));
}

fn fmt_regs(regs: &[crate::ir::Reg]) -> String {
    regs.iter()
        .map(|r| fmt_reg(*r))
        .collect::<Vec<_>>()
        .join(", ")
}

fn fmt_const(c: &IrConst) -> String {
    match c {
        IrConst::Int(v) => format!("Int({})", v),
        IrConst::Float(v) => format!("Float({})", v),
        IrConst::Bool(v) => format!("Bool({})", v),
        IrConst::String(v) => format!("String(\"{}\")", v),
        IrConst::Unit => "Unit".to_string(),
    }
}

fn fmt_pattern(p: &IrPattern) -> String {
    match p {
        IrPattern::Wildcard => "_".to_string(),
        IrPattern::Literal(c) => fmt_const(c),
        IrPattern::Variant {
            variant, bindings, ..
        } => format!("{}({})", variant, fmt_regs(bindings)),
    }
}
