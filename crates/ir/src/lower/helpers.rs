//! Lowering helpers: register allocation, block management, bindings.

use std::collections::HashMap;

use crate::ir::{IrBlock, IrFunction, IrInstr, Reg, Terminator};
use resolver::{Expr, HirId, Item, Pattern};
use specialize::helpers::Substitution;
use typecheck::{Ty, TypeMap};

/// Lookup table for monomorphized function variants.
/// Maps original fn HirId → list of (concrete param types, mangled name).
pub(super) type MonoLookup = HashMap<HirId, Vec<(Vec<Ty>, String)>>;

/// Per-function lowering context. Manages registers, blocks, and bindings.
pub(super) struct FnLowerCtx<'a> {
    pub func: IrFunction,
    pub types: &'a TypeMap,
    /// Map from HirId → register (binding resolution).
    pub bindings: HashMap<HirId, Reg>,
    /// Current block being built (index into func.blocks).
    pub current_block: usize,
    /// Whether the current block has already been terminated. Once a block
    /// diverges (a `break`/`continue`/`return`, or any explicit terminator),
    /// later instructions and terminators in the same block are dead code and
    /// must be dropped — the first terminator wins. Reset by `start_block`.
    block_terminated: bool,
    /// Loop stack: (head_label, exit_label) for break/continue.
    pub loop_stack: Vec<(String, String)>,
    /// Monotonic counter for generating unique labels.
    label_counter: usize,
    /// Monomorphized function lookup: fn_id → [(param_types, mangled_name)].
    pub mono_lookup: &'a MonoLookup,
    /// Active type substitution for monomorphized instance bodies.
    pub subst: Option<&'a Substitution>,
    /// HIR items for looking up FnDef module_path during call lowering.
    pub hir_items: &'a [Item],
}

impl<'a> FnLowerCtx<'a> {
    pub fn new(
        types: &'a TypeMap,
        mono_lookup: &'a MonoLookup,
        subst: Option<&'a Substitution>,
        hir_items: &'a [Item],
    ) -> Self {
        Self {
            func: IrFunction {
                name: String::new(),
                type_params: Vec::new(),
                generic_origin: None,
                params: Vec::new(),
                return_type: Ty::Unit,
                blocks: Vec::new(),
                next_reg: 0,
                is_extern: false,
            },
            types,
            bindings: HashMap::new(),
            current_block: 0,
            block_terminated: false,
            loop_stack: Vec::new(),
            label_counter: 0,
            mono_lookup,
            subst,
            hir_items,
        }
    }

    /// Allocate a fresh register.
    pub fn fresh_reg(&mut self) -> Reg {
        let r = Reg(self.func.next_reg);
        self.func.next_reg += 1;
        r
    }

    /// Generate a fresh label with a prefix. Uses a monotonic counter
    /// to guarantee uniqueness even when called before blocks are created.
    pub fn fresh_label(&mut self, prefix: &str) -> String {
        let idx = self.label_counter;
        self.label_counter += 1;
        format!("{prefix}_{idx}")
    }

    /// Emit an instruction into the current block. Dropped if the block has
    /// already diverged (e.g. statements after a `break`) — that code is
    /// unreachable, and appending it would run after the terminator.
    pub fn emit(&mut self, instr: IrInstr) {
        if self.block_terminated {
            return;
        }
        self.func.blocks[self.current_block].instrs.push(instr);
    }

    /// Terminate the current block. The first terminator wins: once a block has
    /// diverged (a `break`/`continue`/`return` inside an `if` branch or match
    /// arm), a later structural terminator (e.g. the enclosing `if`'s jump to
    /// its merge block) must not overwrite it.
    pub fn terminate(&mut self, term: Terminator) {
        if self.block_terminated {
            return;
        }
        self.func.blocks[self.current_block].terminator = term;
        self.block_terminated = true;
    }

    /// Start a new block and make it current.
    pub fn start_block(&mut self, label: String) {
        self.func.blocks.push(IrBlock {
            label,
            instrs: Vec::new(),
            terminator: Terminator::Unreachable,
        });
        self.current_block = self.func.blocks.len() - 1;
        self.block_terminated = false;
    }

    /// Bind a HirId to a register.
    pub fn bind(&mut self, id: HirId, reg: Reg) {
        self.bindings.insert(id, reg);
    }

    /// Bind a pattern's HirId to a register.
    pub fn bind_pattern(&mut self, pattern: &Pattern, reg: Reg) {
        match pattern {
            Pattern::Ident(p) => {
                self.bind(p.id, reg);
            }
            Pattern::Wildcard(id) => {
                self.bind(*id, reg);
            }
            _ => {
                self.bind(pattern.id(), reg);
            }
        }
    }

    /// Resolve a def_id to a register.
    /// Returns `Reg(u32::MAX)` as a sentinel if the binding is not found.
    /// Downstream code should check for this; it indicates an unresolved name.
    pub fn resolve_name(&self, def_id: Option<HirId>) -> Reg {
        if let Some(id) = def_id {
            if let Some(reg) = self.bindings.get(&id) {
                return *reg;
            }
        }
        Reg(u32::MAX)
    }

    /// The type of an expression, with the active monomorphization
    /// substitution applied. In a generic body the recorded type may be a type
    /// parameter (`T`); in a monomorphized instance this resolves it to the
    /// concrete type so method dispatch can qualify `Type::method`.
    pub fn receiver_type(&self, id: HirId) -> Option<Ty> {
        let ty = self.types.get(&id)?.clone();
        Some(match self.subst {
            Some(subst) => specialize::helpers::substitute(&ty, subst),
            None => ty,
        })
    }

    /// Push a loop context (for break/continue resolution).
    pub fn push_loop(&mut self, head: String, exit: String) {
        self.loop_stack.push((head, exit));
    }

    /// Pop a loop context.
    pub fn pop_loop(&mut self) {
        self.loop_stack.pop();
    }

    /// The innermost loop's (head, exit) labels, if inside a loop. `break`
    /// jumps to the exit; `continue` re-enters at the head. `None` when not in
    /// a loop (typeck rejects `break`/`continue` outside a loop, so the lowerer
    /// just emits nothing in that case rather than panicking).
    pub fn current_loop(&self) -> Option<(String, String)> {
        self.loop_stack.last().cloned()
    }

    /// Get the current loop's head label.
    /// Safety: only called when loop_stack is non-empty (guaranteed by lower_loop).
    pub fn current_loop_head(&self) -> &String {
        // SAFETY: called only from lower_loop body where push_loop was called first.
        // The stack is guaranteed non-empty at this point.
        #[allow(clippy::expect_used)]
        &self
            .loop_stack
            .last()
            .expect("loop_stack is always non-empty when lowering loop body")
            .0
    }

    /// Resolve a function call to its (possibly mangled) name.
    /// If the callee is generic and has monomorphized instances, matches the
    /// argument types against the instance parameter types to find the right
    /// mangled name. Returns the original name for non-generic functions.
    pub fn resolve_call_name(
        &self,
        callee_id: Option<HirId>,
        arg_exprs: &[&Expr],
        thir: &TypeMap,
    ) -> String {
        let callee_id = match callee_id {
            Some(id) => id,
            None => return String::new(),
        };

        let variants = match self.mono_lookup.get(&callee_id) {
            Some(v) if !v.is_empty() => v,
            _ => return String::new(), // Will fall back to name_ref_text
        };

        // Collect arg types from the THIR, applying substitution if active.
        let arg_tys: Vec<Ty> = arg_exprs
            .iter()
            .filter_map(|a| thir.get(&a.id()).cloned())
            .map(|t| {
                if let Some(subst) = self.subst {
                    specialize::helpers::substitute(&t, subst)
                } else {
                    t
                }
            })
            .collect();

        // Find the matching variant by comparing arg types to param types.
        for (param_tys, mangled_name) in variants {
            if param_tys.len() == arg_tys.len()
                && param_tys
                    .iter()
                    .zip(arg_tys.iter())
                    .all(|(p, a)| ty_eq(p, a))
            {
                return mangled_name.clone();
            }
        }

        String::new() // No match — fall back to original name
    }

    /// Ensure the current block ends with a return terminator.
    /// If `value` is `Some(reg)`, the return carries that register's value.
    pub fn ensure_return(&mut self, value: Option<Reg>) {
        let block = &mut self.func.blocks[self.current_block];
        if matches!(block.terminator, Terminator::Unreachable) {
            block.terminator = Terminator::Return(value);
        }
    }
}

/// Structural type equality for matching arg types against mono instance
/// parameter types. Uses `to_string()` for Float (f64 can't implement `Eq`).
fn ty_eq(a: &Ty, b: &Ty) -> bool {
    match (a, b) {
        (Ty::Int, Ty::Int)
        | (Ty::Bool, Ty::Bool)
        | (Ty::String, Ty::String)
        | (Ty::Unit, Ty::Unit)
        | (Ty::Float, Ty::Float)
        | (Ty::Error, Ty::Error) => true,
        (Ty::Struct(a), Ty::Struct(b)) => a.name == b.name,
        (Ty::Enum(a), Ty::Enum(b)) => a.name == b.name,
        (Ty::Instance(a), Ty::Instance(b)) => {
            a.name == b.name
                && a.args.len() == b.args.len()
                && a.args.iter().zip(b.args.iter()).all(|(x, y)| ty_eq(x, y))
        }
        (Ty::Fn(a), Ty::Fn(b)) => {
            a.params.len() == b.params.len()
                && a.params
                    .iter()
                    .zip(b.params.iter())
                    .all(|(x, y)| ty_eq(x, y))
                && ty_eq(&a.return_type, &b.return_type)
        }
        (Ty::Tuple(a), Ty::Tuple(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| ty_eq(x, y))
        }
        (Ty::HeapBuffer(a), Ty::HeapBuffer(b)) => ty_eq(a, b),
        (Ty::TypeParam(a), Ty::TypeParam(b)) => a.index == b.index && a.def_id == b.def_id,
        _ => false,
    }
}
