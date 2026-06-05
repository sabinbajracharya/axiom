//! Stack frame: register file + block traversal state.

use std::collections::HashMap;

use axiom_ir::{IrBlock, IrFunction, Reg};

use crate::error::VmError;
use crate::value::Value;

/// Saved loop position for Break/Continue.
#[derive(Debug, Clone)]
pub struct LoopFrame {
    /// Block index to jump to on Continue.
    pub loop_head: usize,
    /// Block index to jump to on Break.
    pub loop_exit: usize,
}

/// Per-call stack frame.
pub struct StackFrame {
    pub fn_name: String,
    pub regs: Vec<Value>,
    pub block_index: usize,
    pub instr_index: usize,
    pub blocks: Vec<IrBlock>,
    pub label_map: HashMap<String, usize>,
    pub loop_stack: Vec<LoopFrame>,
    /// Last value written to the sentinel register (u32::MAX).
    /// The IR lowerer uses this register for `let` bindings that are
    /// "unused" from its perspective but still referenced in later
    /// instructions (e.g. `let p = StructNew...; p.x`).
    sentinel_val: Value,
}

impl StackFrame {
    /// Create a new frame for a function, binding args to param registers.
    pub fn new(func: &IrFunction, args: Vec<Value>) -> Self {
        let num_regs = func.next_reg as usize;
        let mut regs = vec![Value::Unit; num_regs];

        // Bind params.
        for (param, arg) in func.params.iter().zip(args) {
            regs[param.reg.0 as usize] = arg;
        }

        // Build label → index map.
        let mut label_map = HashMap::new();
        for (i, block) in func.blocks.iter().enumerate() {
            label_map.insert(block.label.clone(), i);
        }

        Self {
            fn_name: func.name.clone(),
            regs,
            block_index: 0,
            instr_index: 0,
            blocks: func.blocks.clone(),
            label_map,
            loop_stack: Vec::new(),
            sentinel_val: Value::Unit,
        }
    }

    /// Read a register value. Returns Unit for out-of-bounds registers.
    /// Reads from `u32::MAX` (the IR lowerer's sentinel for let bindings)
    /// return the last value stored there.
    pub fn read_reg(&self, reg: Reg) -> Result<&Value, VmError> {
        if reg.0 == u32::MAX {
            return Ok(&self.sentinel_val);
        }
        static UNIT: Value = Value::Unit;
        match self.regs.get(reg.0 as usize) {
            Some(v) => Ok(v),
            None => Ok(&UNIT),
        }
    }

    /// Write a register value. Extends the register file on demand
    /// to accommodate dynamically-generated register indices.
    ///
    /// Writes to `u32::MAX` store into `sentinel_val` — the IR lowerer
    /// uses that index for `let` bindings that may still be referenced.
    pub fn write_reg(&mut self, reg: Reg, val: Value) -> Result<(), VmError> {
        if reg.0 == u32::MAX {
            self.sentinel_val = val;
            return Ok(());
        }
        let idx = reg.0 as usize;
        if idx >= self.regs.len() {
            self.regs.resize(idx + 1, Value::Unit);
        }
        self.regs[idx] = val;
        Ok(())
    }

    /// Resolve a block label to its index.
    pub fn resolve_block(&self, label: &str) -> Result<usize, VmError> {
        self.label_map
            .get(label)
            .copied()
            .ok_or(VmError::UndefinedBlock {
                label: label.to_string(),
            })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use axiom_ir::{IrFunction, IrParam};
    use axiom_typeck::Ty;

    fn simple_func() -> IrFunction {
        IrFunction {
            name: "f".to_string(),
            type_params: vec![],
            generic_origin: None,
            params: vec![
                IrParam {
                    reg: Reg(0),
                    name: "x".to_string(),
                    ty: Ty::Int,
                },
                IrParam {
                    reg: Reg(1),
                    name: "y".to_string(),
                    ty: Ty::Int,
                },
            ],
            return_type: Ty::Unit,
            blocks: vec![IrBlock {
                label: "entry".to_string(),
                instrs: vec![],
                terminator: axiom_ir::Terminator::Return(None),
            }],
            next_reg: 3,
        }
    }

    #[test]
    fn test_frame_binds_params() {
        let func = simple_func();
        let frame = StackFrame::new(&func, vec![Value::Int(10), Value::Int(20)]);
        assert_eq!(frame.regs[0], Value::Int(10));
        assert_eq!(frame.regs[1], Value::Int(20));
        assert_eq!(frame.regs[2], Value::Unit);
    }

    #[test]
    fn test_frame_read_reg() {
        let func = simple_func();
        let frame = StackFrame::new(&func, vec![Value::Int(5), Value::Unit]);
        assert_eq!(*frame.read_reg(Reg(0)).unwrap(), Value::Int(5));
    }

    #[test]
    fn test_frame_write_reg() {
        let func = simple_func();
        let mut frame = StackFrame::new(&func, vec![Value::Int(0), Value::Unit]);
        frame.write_reg(Reg(0), Value::Int(99)).unwrap();
        assert_eq!(frame.regs[0], Value::Int(99));
    }

    #[test]
    fn test_frame_out_of_bounds_reg_returns_unit() {
        let func = simple_func();
        let frame = StackFrame::new(&func, vec![]);
        // Out-of-bounds reads return Unit (sentinel registers).
        assert_eq!(*frame.read_reg(Reg(99)).unwrap(), Value::Unit);
        assert_eq!(*frame.read_reg(Reg(u32::MAX)).unwrap(), Value::Unit);
    }

    #[test]
    fn test_frame_write_extends_register_file() {
        let func = simple_func();
        let mut frame = StackFrame::new(&func, vec![]);
        // Writing to a high register extends the file.
        frame.write_reg(Reg(100), Value::Int(42)).unwrap();
        assert_eq!(*frame.read_reg(Reg(100)).unwrap(), Value::Int(42));
    }

    #[test]
    fn test_frame_resolve_block() {
        let func = simple_func();
        let frame = StackFrame::new(&func, vec![]);
        assert_eq!(frame.resolve_block("entry").unwrap(), 0);
        assert!(frame.resolve_block("nope").is_err());
    }
}
