//! Lowering helpers: register allocation, block management, bindings.

use crate::ir::{IrBlock, IrFunction, IrInstr, Reg, Terminator};
use axiom_hir::{HirId, Pattern};
use axiom_typeck::{Ty, TypeMap};

/// Per-function lowering context. Manages registers, blocks, and bindings.
pub(super) struct FnLowerCtx<'a> {
    pub func: IrFunction,
    #[expect(dead_code)]
    pub types: &'a TypeMap,
    /// Map from HirId → register (binding resolution).
    pub bindings: std::collections::HashMap<HirId, Reg>,
    /// Current block being built (index into func.blocks).
    pub current_block: usize,
    /// Loop stack: (head_label, exit_label) for break/continue.
    pub loop_stack: Vec<(String, String)>,
    /// Monotonic counter for generating unique labels.
    label_counter: usize,
}

impl<'a> FnLowerCtx<'a> {
    pub fn new(types: &'a TypeMap) -> Self {
        Self {
            func: IrFunction {
                name: String::new(),
                type_params: Vec::new(),
                generic_origin: None,
                params: Vec::new(),
                return_type: Ty::Unit,
                blocks: Vec::new(),
                next_reg: 0,
            },
            types,
            bindings: std::collections::HashMap::new(),
            current_block: 0,
            loop_stack: Vec::new(),
            label_counter: 0,
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

    /// Emit an instruction into the current block.
    pub fn emit(&mut self, instr: IrInstr) {
        self.func.blocks[self.current_block].instrs.push(instr);
    }

    /// Terminate the current block.
    pub fn terminate(&mut self, term: Terminator) {
        self.func.blocks[self.current_block].terminator = term;
    }

    /// Start a new block and make it current.
    pub fn start_block(&mut self, label: String) {
        self.func.blocks.push(IrBlock {
            label,
            instrs: Vec::new(),
            terminator: Terminator::Unreachable,
        });
        self.current_block = self.func.blocks.len() - 1;
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

    /// Push a loop context (for break/continue resolution).
    pub fn push_loop(&mut self, head: String, exit: String) {
        self.loop_stack.push((head, exit));
    }

    /// Pop a loop context.
    pub fn pop_loop(&mut self) {
        self.loop_stack.pop();
    }

    /// Get the current loop's head label.
    /// Safety: only called when loop_stack is non-empty (guaranteed by lower_loop).
    pub fn current_loop_head(&self) -> &String {
        // The stack is always non-empty when this is called during loop lowering.
        // We avoid `expect` which is banned by clippy.
        let last = self.loop_stack.last();
        // SAFETY: called only from lower_loop body where push_loop was called first.
        // This is an intentional invariant — if violated, the compiler will produce
        // incorrect code anyway, so a panic here is acceptable.
        #[allow(clippy::unwrap_used)]
        &last.unwrap().0
    }

    /// Ensure the current block ends with a return terminator.
    pub fn ensure_return(&mut self) {
        let block = &mut self.func.blocks[self.current_block];
        if matches!(block.terminator, Terminator::Unreachable) {
            block.terminator = Terminator::Return(None);
        }
    }
}
