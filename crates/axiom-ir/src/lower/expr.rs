//! Expression lowering: HIR Expr → IR instructions + register.

use super::helpers::FnLowerCtx;
use crate::ir::{IrConst, IrInstr, Reg};
use axiom_hir::{AssignTarget, Expr, LitKind, LoopKind, Pattern};

/// Lower an HIR expression to a register. Emits instructions into the current block.
pub(super) fn lower_expr(expr: &Expr, ctx: &mut FnLowerCtx) -> Reg {
    match expr {
        Expr::Lit(e) => lower_lit(e, ctx),
        Expr::Path(e) => {
            let def_id = match &e.name_ref {
                axiom_hir::NameRef::Resolved(r) => Some(r.def_id),
                axiom_hir::NameRef::Unresolved(_) => None,
            };
            ctx.resolve_name(def_id)
        }
        Expr::Bin(e) => {
            let lhs = lower_expr(&e.left, ctx);
            let rhs = lower_expr(&e.right, ctx);
            let dst = ctx.fresh_reg();
            ctx.emit(IrInstr::BinOp {
                dst,
                op: e.op,
                lhs,
                rhs,
            });
            dst
        }
        Expr::Unary(e) => {
            let src = lower_expr(&e.operand, ctx);
            let dst = ctx.fresh_reg();
            ctx.emit(IrInstr::UnaryOp { dst, op: e.op, src });
            dst
        }
        Expr::Call(e) => lower_call(e, ctx),
        Expr::MethodCall(e) => lower_method_call(e, ctx),
        Expr::Field(e) => lower_field(e, ctx),
        Expr::Index(e) => lower_index(e, ctx),
        Expr::StructLit(e) => lower_struct_lit(e, ctx),
        Expr::ListLit(e) => lower_list_lit(e, ctx),
        Expr::Block(e) => lower_block(e, ctx),
        Expr::If(e) => lower_if(e, ctx),
        Expr::Match(e) => lower_match(e, ctx),
        Expr::Loop(e) => lower_loop(e, ctx),
        Expr::Assign(e) => lower_assign(e, ctx),
    }
}

fn lower_lit(e: &axiom_hir::LitExpr, ctx: &mut FnLowerCtx) -> Reg {
    let dst = ctx.fresh_reg();
    let value = match &e.kind {
        LitKind::Int(v) => IrConst::Int(*v),
        LitKind::Float(v) => IrConst::Float(*v),
        LitKind::Bool(v) => IrConst::Bool(*v),
        LitKind::String(v) => IrConst::String(v.clone()),
        LitKind::Unit => IrConst::Unit,
    };
    ctx.emit(IrInstr::Const { dst, value });
    dst
}

fn lower_call(e: &axiom_hir::CallExpr, ctx: &mut FnLowerCtx) -> Reg {
    // Collect arg expr references for type lookup before lowering to registers.
    let arg_refs: Vec<&Expr> = e.args.iter().collect();
    let callee_id = match &e.callee {
        axiom_hir::NameRef::Resolved(r) => Some(r.def_id),
        _ => None,
    };

    // Try to resolve to a monomorphized mangled name.
    let resolved = ctx.resolve_call_name(callee_id, &arg_refs, ctx.types);
    let function = if resolved.is_empty() {
        name_ref_text(&e.callee)
    } else {
        resolved
    };

    let args: Vec<Reg> = e.args.iter().map(|a| lower_expr(a, ctx)).collect();
    let dst = ctx.fresh_reg();
    ctx.emit(IrInstr::Call {
        dst,
        function,
        args,
    });
    dst
}

fn lower_method_call(e: &axiom_hir::MethodCallExpr, ctx: &mut FnLowerCtx) -> Reg {
    let receiver = lower_expr(&e.receiver, ctx);
    let args: Vec<Reg> = e.args.iter().map(|a| lower_expr(a, ctx)).collect();
    let dst = ctx.fresh_reg();
    ctx.emit(IrInstr::MethodCall {
        dst,
        receiver,
        method: e.method.clone(),
        args,
    });
    dst
}

fn lower_field(e: &axiom_hir::FieldExpr, ctx: &mut FnLowerCtx) -> Reg {
    let base = lower_expr(&e.receiver, ctx);
    let dst = ctx.fresh_reg();
    ctx.emit(IrInstr::Field {
        dst,
        base,
        field: e.field.clone(),
    });
    dst
}

fn lower_index(e: &axiom_hir::IndexExpr, ctx: &mut FnLowerCtx) -> Reg {
    let base = lower_expr(&e.base, ctx);
    let index = lower_expr(&e.index, ctx);
    let dst = ctx.fresh_reg();
    ctx.emit(IrInstr::Index { dst, base, index });
    dst
}

fn lower_struct_lit(e: &axiom_hir::StructLitExpr, ctx: &mut FnLowerCtx) -> Reg {
    let fields: Vec<(String, Reg)> = e
        .fields
        .iter()
        .map(|f| {
            let reg = lower_expr(&f.value, ctx);
            (f.name.clone(), reg)
        })
        .collect();
    let dst = ctx.fresh_reg();
    ctx.emit(IrInstr::StructNew {
        dst,
        type_name: name_ref_text(&e.type_name),
        fields,
    });
    dst
}

fn lower_list_lit(e: &axiom_hir::ListLitExpr, ctx: &mut FnLowerCtx) -> Reg {
    let elements: Vec<Reg> = e.elements.iter().map(|el| lower_expr(el, ctx)).collect();
    let dst = ctx.fresh_reg();
    ctx.emit(IrInstr::ListNew { dst, elements });
    dst
}

fn lower_block(e: &axiom_hir::Block, ctx: &mut FnLowerCtx) -> Reg {
    for stmt in &e.stmts {
        super::stmt::lower_stmt(stmt, ctx);
    }
    match &e.tail {
        Some(tail) => lower_expr(tail, ctx),
        None => unit_reg(ctx),
    }
}

fn lower_if(e: &axiom_hir::IfExpr, ctx: &mut FnLowerCtx) -> Reg {
    let cond = lower_expr(&e.condition, ctx);
    let then_label = ctx.fresh_label("then");
    let merge_label = ctx.fresh_label("if_merge");
    let else_label = match &e.else_branch {
        Some(_) => ctx.fresh_label("else"),
        None => merge_label.clone(),
    };

    ctx.terminate(crate::ir::Terminator::Branch {
        cond,
        true_target: then_label.clone(),
        false_target: else_label.clone(),
    });

    // Then block
    ctx.start_block(then_label);
    let then_val = super::stmt::lower_block_expr(&e.then_branch, ctx);
    let then_copy = ctx.fresh_reg();
    ctx.emit(IrInstr::Copy {
        dst: then_copy,
        src: then_val,
    });
    ctx.terminate(crate::ir::Terminator::Jump {
        target: merge_label.clone(),
    });

    // Else block (if present)
    if let Some(else_expr) = &e.else_branch {
        ctx.start_block(else_label);
        let else_val = lower_expr(else_expr, ctx);
        let else_copy = ctx.fresh_reg();
        ctx.emit(IrInstr::Copy {
            dst: else_copy,
            src: else_val,
        });
        ctx.terminate(crate::ir::Terminator::Jump {
            target: merge_label.clone(),
        });
        let _ = else_copy;
    }

    // Merge block
    ctx.start_block(merge_label);
    let dst = ctx.fresh_reg();
    ctx.emit(IrInstr::Copy {
        dst,
        src: then_copy,
    });
    dst
}

fn lower_match(e: &axiom_hir::MatchExpr, ctx: &mut FnLowerCtx) -> Reg {
    let scrutinee = lower_expr(&e.scrutinee, ctx);
    let merge_label = ctx.fresh_label("match_merge");
    let mut arm_labels = Vec::new();

    for _ in &e.arms {
        arm_labels.push(ctx.fresh_label("match_arm"));
    }

    let fallback = arm_labels
        .last()
        .cloned()
        .unwrap_or_else(|| merge_label.clone());

    let ir_arms: Vec<crate::ir::MatchArm> = e
        .arms
        .iter()
        .zip(&arm_labels)
        .map(|(arm, label)| crate::ir::MatchArm {
            pattern: lower_pattern(&arm.pattern, ctx),
            target: label.clone(),
        })
        .collect();

    ctx.terminate(crate::ir::Terminator::Match {
        scrutinee,
        arms: ir_arms,
        fallback,
    });

    let dst = ctx.fresh_reg();
    for (arm, label) in e.arms.iter().zip(&arm_labels) {
        ctx.start_block(label.clone());
        let arm_val = lower_expr(&arm.body, ctx);
        ctx.emit(IrInstr::Copy { dst, src: arm_val });
        ctx.terminate(crate::ir::Terminator::Jump {
            target: merge_label.clone(),
        });
    }

    ctx.start_block(merge_label);
    dst
}

fn lower_loop(e: &axiom_hir::LoopExpr, ctx: &mut FnLowerCtx) -> Reg {
    let head_label = ctx.fresh_label("loop_head");
    let body_label = ctx.fresh_label("loop_body");
    let exit_label = ctx.fresh_label("loop_exit");

    ctx.push_loop(head_label.clone(), exit_label.clone());

    ctx.terminate(crate::ir::Terminator::Jump {
        target: head_label.clone(),
    });

    ctx.start_block(head_label);
    match &e.kind {
        LoopKind::Infinite(block) => {
            ctx.terminate(crate::ir::Terminator::Jump {
                target: body_label.clone(),
            });
            ctx.start_block(body_label);
            super::stmt::lower_block_expr(block, ctx);
            ctx.terminate(crate::ir::Terminator::Jump {
                target: ctx.current_loop_head().clone(),
            });
        }
        LoopKind::Conditional { condition, body } => {
            let cond = lower_expr(condition, ctx);
            ctx.terminate(crate::ir::Terminator::Branch {
                cond,
                true_target: body_label.clone(),
                false_target: exit_label.clone(),
            });
            ctx.start_block(body_label);
            super::stmt::lower_block_expr(body, ctx);
            ctx.terminate(crate::ir::Terminator::Jump {
                target: ctx.current_loop_head().clone(),
            });
        }
        LoopKind::Iterator {
            binding: _,
            binding_id: _,
            iterable: _,
            body,
        } => {
            ctx.terminate(crate::ir::Terminator::Jump {
                target: body_label.clone(),
            });
            ctx.start_block(body_label);
            super::stmt::lower_block_expr(body, ctx);
            ctx.terminate(crate::ir::Terminator::Jump {
                target: ctx.current_loop_head().clone(),
            });
        }
    }

    ctx.pop_loop();

    ctx.start_block(exit_label);
    unit_reg(ctx)
}

fn lower_assign(e: &axiom_hir::AssignExpr, ctx: &mut FnLowerCtx) -> Reg {
    let value = lower_expr(&e.value, ctx);
    let dst = resolve_assign_target(&e.target, ctx);

    match e.op {
        axiom_hir::AssignOp::Plain => {
            ctx.emit(IrInstr::Copy { dst, src: value });
        }
        compound => {
            // x op= val  →  %tmp = BinOp(x, val); Copy(x, %tmp)
            let binop = match compound {
                axiom_hir::AssignOp::Add => axiom_hir::BinOp::Add,
                axiom_hir::AssignOp::Sub => axiom_hir::BinOp::Sub,
                axiom_hir::AssignOp::Mul => axiom_hir::BinOp::Mul,
                axiom_hir::AssignOp::Div => axiom_hir::BinOp::Div,
                axiom_hir::AssignOp::Mod => axiom_hir::BinOp::Mod,
                axiom_hir::AssignOp::Plain => unreachable!(),
            };
            let tmp = ctx.fresh_reg();
            ctx.emit(IrInstr::BinOp {
                dst: tmp,
                op: binop,
                lhs: dst,
                rhs: value,
            });
            ctx.emit(IrInstr::Copy { dst, src: tmp });
        }
    }

    let unit = ctx.fresh_reg();
    ctx.emit(IrInstr::Const {
        dst: unit,
        value: IrConst::Unit,
    });
    unit
}

/// Emit a Unit constant and return its register.
fn unit_reg(ctx: &mut FnLowerCtx) -> Reg {
    let dst = ctx.fresh_reg();
    ctx.emit(IrInstr::Const {
        dst,
        value: IrConst::Unit,
    });
    dst
}

/// Extract the text from a NameRef (resolved or unresolved).
fn name_ref_text(nr: &axiom_hir::NameRef) -> String {
    match nr {
        axiom_hir::NameRef::Resolved(r) => r.text.clone(),
        axiom_hir::NameRef::Unresolved(u) => u.text.clone(),
    }
}

fn resolve_assign_target(target: &AssignTarget, ctx: &FnLowerCtx) -> Reg {
    match target {
        AssignTarget::Name(nr) => {
            let def_id = match nr {
                axiom_hir::NameRef::Resolved(r) => Some(r.def_id),
                axiom_hir::NameRef::Unresolved(_) => None,
            };
            ctx.resolve_name(def_id)
        }
        // TODO(v1): Field/Index assignment needs FieldSet/IndexSet instructions.
        // For now, return sentinel so downstream knows this is unsupported.
        AssignTarget::Field { .. } | AssignTarget::Index { .. } => Reg(u32::MAX),
    }
}

pub(super) fn lower_pattern(pat: &Pattern, ctx: &mut FnLowerCtx) -> crate::ir::IrPattern {
    match pat {
        Pattern::Wildcard(_) => crate::ir::IrPattern::Wildcard,
        Pattern::Literal(lp) => {
            let c = match &lp.kind {
                LitKind::Int(v) => IrConst::Int(*v),
                LitKind::Float(v) => IrConst::Float(*v),
                LitKind::Bool(v) => IrConst::Bool(*v),
                LitKind::String(v) => IrConst::String(v.clone()),
                LitKind::Unit => IrConst::Unit,
            };
            crate::ir::IrPattern::Literal(c)
        }
        Pattern::Ident(_) => crate::ir::IrPattern::Wildcard,
        Pattern::TupleStruct(p) => {
            let bindings: Vec<Reg> = p
                .fields
                .iter()
                .map(|f| {
                    let reg = ctx.fresh_reg();
                    ctx.bind_pattern(f, reg);
                    reg
                })
                .collect();
            crate::ir::IrPattern::Variant {
                type_name: String::new(),
                variant: name_ref_text(&p.path),
                bindings,
            }
        }
        Pattern::Struct(_) => crate::ir::IrPattern::Wildcard,
        Pattern::Or(_) => crate::ir::IrPattern::Wildcard,
        Pattern::Range(_) => crate::ir::IrPattern::Wildcard,
    }
}
