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
        }
    }

    /// Read a register value.
    pub fn read_reg(&self, reg: Reg) -> Result<&Value, VmError> {
        self.regs
            .get(reg.0 as usize)
            .ok_or(VmError::UndefinedReg(reg.0))
    }

    /// Write a register value.
    pub fn write_reg(&mut self, reg: Reg, val: Value) -> Result<(), VmError> {
        let slot = self
            .regs
            .get_mut(reg.0 as usize)
            .ok_or(VmError::UndefinedReg(reg.0))?;
        *slot = val;
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
    fn test_frame_undefined_reg() {
        let func = simple_func();
        let frame = StackFrame::new(&func, vec![]);
        assert!(frame.read_reg(Reg(99)).is_err());
    }

    #[test]
    fn test_frame_resolve_block() {
        let func = simple_func();
        let frame = StackFrame::new(&func, vec![]);
        assert_eq!(frame.resolve_block("entry").unwrap(), 0);
        assert!(frame.resolve_block("nope").is_err());
    }
}
