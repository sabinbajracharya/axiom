//! Expression lowering: HIR Expr → IR instructions + register.

use super::helpers::FnLowerCtx;
use crate::ir::{IrConst, IrInstr, Reg};
use axiom_hir::{Expr, LitKind, LoopKind, Pattern};

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
        Expr::Assign(e) => super::assign::lower_assign(e, ctx),
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
    // `HeapBuffer<T>` floor ops (P4) lower to dedicated heap instructions rather
    // than function calls — there is no `heap_*` FnDef; they are compiler
    // intrinsics the VM implements directly.
    if let Some(reg) = lower_heap_intrinsic(e, ctx) {
        return reg;
    }

    // Collect arg expr references for type lookup before lowering to registers.
    let arg_refs: Vec<&Expr> = e.args.iter().collect();
    let callee_id = match &e.callee {
        axiom_hir::NameRef::Resolved(r) => Some(r.def_id),
        _ => None,
    };

    // Try to resolve to a monomorphized mangled name.
    let resolved = ctx.resolve_call_name(callee_id, &arg_refs, ctx.types);
    let base = name_ref_text(&e.callee);
    let function = if !resolved.is_empty() {
        resolved
    } else if let Some(type_name) = assoc_method_type(e, ctx.hir_items) {
        // Associated-function call (`List::new`): use the qualified IR name
        // "Type::method", matching how impl methods are lowered.
        format!("{type_name}::{base}")
    } else {
        // Qualify with the callee FnDef's module_path so call target
        // matches the qualified IR function name.
        find_fn_module_path(callee_id, ctx.hir_items)
            .filter(|p| !p.is_empty())
            .map(|p| format!("{p}::{base}"))
            .unwrap_or(base)
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

/// If this call is a qualified associated-function call (`Type::method(...)`)
/// whose qualifier names a type with an inherent impl method of that name,
/// return the type name. Returns `None` otherwise — enum constructors and
/// module-qualified calls are left to ordinary resolution.
fn assoc_method_type(e: &axiom_hir::CallExpr, items: &[axiom_hir::Item]) -> Option<String> {
    let qualifier = e.qualifier.as_ref()?;
    let method = name_ref_text(&e.callee);
    for item in items {
        if let axiom_hir::Item::ImplDef(impl_def) = item {
            let type_name = match &impl_def.type_name {
                axiom_hir::NameRef::Resolved(r) => &r.text,
                axiom_hir::NameRef::Unresolved(u) => &u.text,
            };
            if type_name == qualifier && impl_def.methods.iter().any(|m| m.name == method) {
                return Some(qualifier.clone());
            }
        }
    }
    None
}

/// Lower a `HeapBuffer<T>` floor-op call (`heap_alloc`/`heap_get`/`heap_set`/
/// `heap_free`) to its dedicated heap instruction. Returns `None` for any other
/// call so normal function-call lowering proceeds.
fn lower_heap_intrinsic(e: &axiom_hir::CallExpr, ctx: &mut FnLowerCtx) -> Option<Reg> {
    let name = name_ref_text(&e.callee);
    let args: Vec<Reg> = match name.as_str() {
        "heap_alloc" | "heap_get" | "heap_set" | "heap_free" => {
            e.args.iter().map(|a| lower_expr(a, ctx)).collect()
        }
        _ => return None,
    };
    let dst = ctx.fresh_reg();
    let instr = match name.as_str() {
        "heap_alloc" => IrInstr::HeapAlloc {
            dst,
            count: args[0],
        },
        "heap_get" => IrInstr::HeapGet {
            dst,
            ptr: args[0],
            index: args[1],
        },
        "heap_set" => IrInstr::HeapSet {
            ptr: args[0],
            index: args[1],
            value: args[2],
        },
        "heap_free" => IrInstr::HeapFree { ptr: args[0] },
        _ => unreachable!("guarded by the match above"),
    };
    ctx.emit(instr);
    Some(dst)
}

fn lower_method_call(e: &axiom_hir::MethodCallExpr, ctx: &mut FnLowerCtx) -> Reg {
    let receiver = lower_expr(&e.receiver, ctx);
    let args: Vec<Reg> = e.args.iter().map(|a| lower_expr(a, ctx)).collect();
    let dst = ctx.fresh_reg();

    // Qualify the method name as "Type::method" to avoid collisions when two
    // impls define the same method name. In a monomorphized body the receiver's
    // type may be a type parameter (`key: K`); substitute it to the concrete
    // type so a trait-method call (`key.hash()`) dispatches to the real impl
    // (`Int::hash`).
    let method = match ctx.receiver_type(e.receiver.id()) {
        Some(ty) => match type_name_from_ty(&ty) {
            Some(type_name) => format!("{type_name}::{}", e.method),
            None => e.method.clone(),
        },
        None => e.method.clone(),
    };

    ctx.emit(IrInstr::MethodCall {
        dst,
        receiver,
        method,
        args,
    });
    dst
}

/// Extract the type name from a Ty for method name qualification.
/// Primitives qualify too, so their intrinsic methods (e.g. `String::as_bytes`)
/// dispatch correctly in the VM rather than being looked up as bare functions.
fn type_name_from_ty(ty: &axiom_typeck::Ty) -> Option<String> {
    match ty {
        axiom_typeck::Ty::Struct(s) => Some(s.name.clone()),
        axiom_typeck::Ty::Enum(e) => Some(e.name.clone()),
        axiom_typeck::Ty::String => Some("String".to_string()),
        axiom_typeck::Ty::Int => Some("Int".to_string()),
        axiom_typeck::Ty::Float => Some("Float".to_string()),
        axiom_typeck::Ty::Bool => Some("Bool".to_string()),
        // Named instances (Bytes, List<T>, Map<K,V>, generic structs) qualify by
        // their base name — matching the typeck side and the "Type::method"
        // impl-method IR names, so intrinsic calls like `Bytes::len` dispatch.
        axiom_typeck::Ty::Instance(inst) => Some(inst.name.clone()),
        _ => None,
    }
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
    let indices: Vec<Reg> = e.indices.iter().map(|idx| lower_expr(idx, ctx)).collect();
    let base_ty = ctx.receiver_type(e.base.id());
    lower_index_read(base, base_ty.as_ref(), &indices, ctx)
}

/// Lower a *read* of `base[indices]` into a fresh register.
///
/// A raw `[T]` heap buffer indexes with the primitive `Index` instruction. Any
/// other indexable base is a struct with a `subscript` operator (the type
/// checker proved one exists), so dispatch `base[i, j]` to its lowered
/// `Type::subscript(self, i, j)` function — the receiver's runtime type
/// resolves the qualified name in the VM.
pub(super) fn lower_index_read(
    base: Reg,
    base_ty: Option<&axiom_typeck::Ty>,
    indices: &[Reg],
    ctx: &mut FnLowerCtx,
) -> Reg {
    let dst = ctx.fresh_reg();
    if matches!(base_ty, Some(axiom_typeck::Ty::HeapBuffer(_))) {
        let index = indices.first().copied().unwrap_or(base);
        ctx.emit(IrInstr::Index { dst, base, index });
        return dst;
    }
    let method = match base_ty.and_then(type_name_from_ty) {
        Some(type_name) => axiom_hir::lang::subscript_fn(type_name.as_str()),
        None => axiom_hir::lang::SUBSCRIPT.to_string(),
    };
    ctx.emit(IrInstr::MethodCall {
        dst,
        receiver: base,
        method,
        args: indices.to_vec(),
    });
    dst
}

/// Lower a *write* of `base[indices] = value`.
///
/// The mirror of [`lower_index_read`]: a raw `[T]` heap buffer writes with the
/// primitive `IndexSet`; any other base dispatches to its `Type::subscript_set`
/// setter as an `inout self` method call.
pub(super) fn lower_index_write(
    base: Reg,
    base_ty: Option<&axiom_typeck::Ty>,
    indices: &[Reg],
    value: Reg,
    ctx: &mut FnLowerCtx,
) {
    if matches!(base_ty, Some(axiom_typeck::Ty::HeapBuffer(_))) {
        let index = indices.first().copied().unwrap_or(base);
        ctx.emit(IrInstr::IndexSet { base, index, value });
        return;
    }
    let method = match base_ty.and_then(type_name_from_ty) {
        Some(type_name) => axiom_hir::lang::subscript_set_fn(type_name.as_str()),
        None => axiom_hir::lang::SUBSCRIPT_SET.to_string(),
    };
    let dst = ctx.fresh_reg();
    let mut args = indices.to_vec();
    args.push(value);
    ctx.emit(IrInstr::MethodCall {
        dst,
        receiver: base,
        method,
        args,
    });
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

/// `[a, b, c]` is sugar for building a stdlib `List<T>` by hand — there is no
/// compiler-native list value. A non-empty literal pre-sizes the list to its
/// known length (`List::with_capacity(n)`) and then `push`es each element, so a
/// fixed-size literal allocates exactly once instead of regrowing 0 → 4 → 8 → …
/// An empty literal has no size to pre-size to, so it lowers to `List::new()`
/// (the first later `push` allocates). `push`'s `inout self` writes the growing
/// list back into `list` after each call; elements evaluate left-to-right first.
fn lower_list_lit(e: &axiom_hir::ListLitExpr, ctx: &mut FnLowerCtx) -> Reg {
    let elements: Vec<Reg> = e.elements.iter().map(|el| lower_expr(el, ctx)).collect();
    let list = ctx.fresh_reg();
    if elements.is_empty() {
        ctx.emit(IrInstr::Call {
            dst: list,
            function: axiom_hir::lang::LIST_NEW.to_string(),
            args: vec![],
        });
        return list;
    }
    let cap = ctx.fresh_reg();
    ctx.emit(IrInstr::Const {
        dst: cap,
        value: IrConst::Int(elements.len() as i64),
    });
    ctx.emit(IrInstr::Call {
        dst: list,
        function: axiom_hir::lang::LIST_WITH_CAPACITY.to_string(),
        args: vec![cap],
    });
    for element in elements {
        let dst = ctx.fresh_reg();
        ctx.emit(IrInstr::MethodCall {
            dst,
            receiver: list,
            method: axiom_hir::lang::LIST_PUSH.to_string(),
            args: vec![element],
        });
    }
    list
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
    // One shared result register, written by whichever branch runs (registers
    // persist across blocks within a frame). An `if` without `else` is
    // Unit-typed: on the false path `dst` is never written and reads as Unit.
    let dst = ctx.fresh_reg();
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

    // Then block: evaluate and store into the shared result.
    ctx.start_block(then_label);
    let then_val = super::stmt::lower_block_expr(&e.then_branch, ctx);
    ctx.emit(IrInstr::Copy { dst, src: then_val });
    ctx.terminate(crate::ir::Terminator::Jump {
        target: merge_label.clone(),
    });

    // Else block (if present): evaluate and store into the same result.
    if let Some(else_expr) = &e.else_branch {
        ctx.start_block(else_label);
        let else_val = lower_expr(else_expr, ctx);
        ctx.emit(IrInstr::Copy { dst, src: else_val });
        ctx.terminate(crate::ir::Terminator::Jump {
            target: merge_label.clone(),
        });
    }

    ctx.start_block(merge_label);
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

    // Build match arms, collecting pattern bindings for payload extraction.
    let mut arm_patterns: Vec<crate::ir::IrPattern> = Vec::new();
    let ir_arms: Vec<crate::ir::MatchArm> = e
        .arms
        .iter()
        .zip(&arm_labels)
        .map(|(arm, label)| {
            let pattern = lower_pattern(&arm.pattern, ctx);
            arm_patterns.push(pattern.clone());
            crate::ir::MatchArm {
                pattern,
                target: label.clone(),
            }
        })
        .collect();

    ctx.terminate(crate::ir::Terminator::Match {
        scrutinee,
        arms: ir_arms,
        fallback,
    });

    let dst = ctx.fresh_reg();
    for ((arm, label), pattern) in e.arms.iter().zip(&arm_labels).zip(&arm_patterns) {
        ctx.start_block(label.clone());
        // Emit VariantPayload instructions for variant pattern bindings.
        if let crate::ir::IrPattern::Variant { bindings, .. } = pattern {
            for (i, binding_reg) in bindings.iter().enumerate() {
                ctx.emit(IrInstr::VariantPayload {
                    dst: *binding_reg,
                    scrutinee,
                    index: i,
                });
            }
        }
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

/// Emit a Unit constant and return its register.
pub(super) fn unit_reg(ctx: &mut FnLowerCtx) -> Reg {
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

/// Look up a FnDef's `module_path` by HirId across all HIR items.
/// Returns `None` if the FnDef is not found or has an empty module_path.
fn find_fn_module_path(id: Option<axiom_hir::HirId>, items: &[axiom_hir::Item]) -> Option<String> {
    let id = id?;
    for item in items {
        match item {
            axiom_hir::Item::FnDef(f) if f.id == id => {
                return Some(f.module_path.clone());
            }
            axiom_hir::Item::ImplDef(impl_def) => {
                for m in &impl_def.methods {
                    if m.id == id {
                        return Some(m.module_path.clone());
                    }
                }
            }
            _ => {}
        }
    }
    None
}
