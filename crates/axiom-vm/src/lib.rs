//! Register-IR interpreter for the Axiom language.
//!
//! Takes an [`IrModule`](axiom_ir::Ir) and executes it by walking basic blocks,
//! dispatching instructions against a register file, and managing a call stack.

mod error;
mod exec;
mod frame;
pub mod trace;
mod value;

use std::collections::HashMap;

pub use error::VmError;
pub use value::Value;

use frame::StackFrame;
use trace::ExecutionTrace;

/// The VM — top-level executor.
pub struct Vm {
    ir: axiom_ir::Ir,
    fn_map: HashMap<String, usize>,
    heap: HeapArena,
    call_stack: Vec<StackFrame>,
    trace: Option<ExecutionTrace>,
    /// Pending return value to write to caller's dst after frame pop.
    pending_return: Option<Value>,
}

/// Simple Vec-backed heap arena.
struct HeapArena {
    slots: Vec<Option<HeapSlot>>,
}

struct HeapSlot {
    data: Vec<Value>,
    #[allow(dead_code)]
    refcount: u32,
}

impl HeapArena {
    fn new() -> Self {
        Self { slots: Vec::new() }
    }

    fn alloc(&mut self, data: Vec<Value>) -> usize {
        let idx = self.slots.len();
        self.slots.push(Some(HeapSlot { data, refcount: 1 }));
        idx
    }

    fn free(&mut self, idx: usize) -> Result<(), VmError> {
        if idx < self.slots.len() {
            self.slots[idx] = None;
            Ok(())
        } else {
            Err(VmError::HeapSlotFreed(idx))
        }
    }

    fn get(&self, idx: usize, index: usize) -> Result<&Value, VmError> {
        let slot = self
            .slots
            .get(idx)
            .and_then(|s| s.as_ref())
            .ok_or(VmError::HeapSlotFreed(idx))?;
        slot.data.get(index).ok_or(VmError::HeapIndexOutOfBounds {
            index,
            len: slot.data.len(),
        })
    }

    fn set(&mut self, idx: usize, index: usize, val: Value) -> Result<(), VmError> {
        let slot = self
            .slots
            .get_mut(idx)
            .and_then(|s| s.as_mut())
            .ok_or(VmError::HeapSlotFreed(idx))?;
        if index >= slot.data.len() {
            return Err(VmError::HeapIndexOutOfBounds {
                index,
                len: slot.data.len(),
            });
        }
        slot.data[index] = val;
        Ok(())
    }
}

impl Vm {
    /// Create a new VM from an IR module.
    pub fn new(ir: axiom_ir::Ir) -> Self {
        let mut fn_map = HashMap::new();
        for (i, func) in ir.functions.iter().enumerate() {
            fn_map.insert(func.name.clone(), i);
        }
        Self {
            ir,
            fn_map,
            heap: HeapArena::new(),
            call_stack: Vec::new(),
            trace: None,
            pending_return: None,
        }
    }

    /// Enable or disable execution tracing.
    pub fn set_tracing(&mut self, enabled: bool) {
        if enabled {
            self.trace = Some(ExecutionTrace::new());
        } else {
            self.trace = None;
        }
    }

    /// Take the recorded trace (clears internal state).
    pub fn take_trace(&mut self) -> Option<ExecutionTrace> {
        self.trace.take()
    }

    /// Execute the entry function and return its value.
    pub fn run(&mut self) -> Result<Value, VmError> {
        let entry_idx = self.ir.entry;
        let entry_fn = &self.ir.functions[entry_idx];
        let args: Vec<Value> = entry_fn
            .params
            .iter()
            .map(|_p| Value::from_const(&axiom_ir::IrConst::Unit))
            .collect();
        self.run_function(&entry_fn.name.clone(), args)
    }

    /// Call a specific function by name with given arguments.
    pub fn run_function(&mut self, name: &str, args: Vec<Value>) -> Result<Value, VmError> {
        // Check builtins first.
        if exec::builtins::is_builtin(name) {
            return exec::builtins::call_builtin(name, args, &mut self.trace);
        }

        // Push the entry frame.
        self.push_frame(name, args)?;

        // Run the single execution loop until the call stack is empty.
        self.run_loop()?;

        self.pending_return.take().ok_or(VmError::EmptyCallStack)
    }

    /// Push a new stack frame for a function call.
    fn push_frame(&mut self, name: &str, args: Vec<Value>) -> Result<(), VmError> {
        let fn_idx = self
            .fn_map
            .get(name)
            .copied()
            .ok_or(VmError::FunctionNotFound {
                name: name.to_string(),
            })?;
        let func = &self.ir.functions[fn_idx];
        if func.params.len() != args.len() {
            return Err(VmError::ArityMismatch {
                expected: func.params.len(),
                got: args.len(),
            });
        }
        let frame = StackFrame::new(func, args);
        self.call_stack.push(frame);
        Ok(())
    }

    /// Single iterative execution loop. Processes instructions and terminators
    /// until the call stack is empty.
    fn run_loop(&mut self) -> Result<(), VmError> {
        loop {
            if self.call_stack.is_empty() {
                return Ok(());
            }

            // Check if current frame has more instructions.
            let has_more = {
                let frame = self.current_frame()?;
                frame.instr_index < frame.blocks[frame.block_index].instrs.len()
            };

            if has_more {
                self.exec_next_instr()?;
            } else {
                self.exec_terminator()?;
            }
        }
    }

    /// Get a reference to the current (top) frame.
    fn current_frame(&self) -> Result<&StackFrame, VmError> {
        self.call_stack.last().ok_or(VmError::EmptyCallStack)
    }

    /// Get a mutable reference to the current (top) frame.
    fn current_frame_mut(&mut self) -> Result<&mut StackFrame, VmError> {
        self.call_stack.last_mut().ok_or(VmError::EmptyCallStack)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use axiom_ir::{IrBlock, IrConst, IrFunction, IrInstr, IrParam, Reg, Terminator};

    /// Build a minimal Ir with one function containing the given instructions
    /// and a Return terminator.
    fn make_ir(instrs: Vec<IrInstr>, return_reg: Option<Reg>) -> axiom_ir::Ir {
        let next_reg = instrs
            .iter()
            .map(|i| match i {
                IrInstr::Const { dst, .. }
                | IrInstr::BinOp { dst, .. }
                | IrInstr::UnaryOp { dst, .. }
                | IrInstr::Copy { dst, .. }
                | IrInstr::Call { dst, .. }
                | IrInstr::MethodCall { dst, .. }
                | IrInstr::Field { dst, .. }
                | IrInstr::Index { dst, .. }
                | IrInstr::StructNew { dst, .. }
                | IrInstr::EnumNew { dst, .. }
                | IrInstr::ListNew { dst, .. }
                | IrInstr::HeapAlloc { dst, .. } => dst.0 + 1,
                IrInstr::HeapFree { .. }
                | IrInstr::HeapSet { .. }
                | IrInstr::FieldSet { .. }
                | IrInstr::IndexSet { .. } => 0,
                IrInstr::HeapGet { dst, .. } | IrInstr::VariantPayload { dst, .. } => dst.0 + 1,
            })
            .max()
            .unwrap_or(0);
        let block = IrBlock {
            label: "entry".to_string(),
            instrs,
            terminator: Terminator::Return(return_reg),
        };
        let func = IrFunction {
            name: "main".to_string(),
            type_params: vec![],
            generic_origin: None,
            params: vec![],
            return_type: axiom_typeck::Ty::Unit,
            blocks: vec![block],
            next_reg: next_reg.max(1),
            is_extern: false,
        };
        axiom_ir::Ir {
            functions: vec![func],
            entry: 0,
            enum_variants: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn test_exec_const_int() {
        let ir = make_ir(
            vec![IrInstr::Const {
                dst: Reg(0),
                value: IrConst::Int(42),
            }],
            Some(Reg(0)),
        );
        let mut vm = Vm::new(ir);
        let result = vm.run().unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_exec_const_unit() {
        let ir = make_ir(vec![], None);
        let mut vm = Vm::new(ir);
        let result = vm.run().unwrap();
        assert_eq!(result, Value::Unit);
    }

    #[test]
    fn test_exec_copy() {
        let ir = make_ir(
            vec![
                IrInstr::Const {
                    dst: Reg(0),
                    value: IrConst::Int(7),
                },
                IrInstr::Copy {
                    dst: Reg(1),
                    src: Reg(0),
                },
            ],
            Some(Reg(1)),
        );
        let mut vm = Vm::new(ir);
        let result = vm.run().unwrap();
        assert_eq!(result, Value::Int(7));
    }

    #[test]
    fn test_exec_binop_add() {
        let ir = make_ir(
            vec![
                IrInstr::Const {
                    dst: Reg(0),
                    value: IrConst::Int(10),
                },
                IrInstr::Const {
                    dst: Reg(1),
                    value: IrConst::Int(20),
                },
                IrInstr::BinOp {
                    dst: Reg(2),
                    op: axiom_hir::BinOp::Add,
                    lhs: Reg(0),
                    rhs: Reg(1),
                },
            ],
            Some(Reg(2)),
        );
        let mut vm = Vm::new(ir);
        let result = vm.run().unwrap();
        assert_eq!(result, Value::Int(30));
    }

    #[test]
    fn test_exec_branch() {
        let block_true = IrBlock {
            label: "then".to_string(),
            instrs: vec![IrInstr::Const {
                dst: Reg(2),
                value: IrConst::Int(100),
            }],
            terminator: Terminator::Return(Some(Reg(2))),
        };
        let block_false = IrBlock {
            label: "else".to_string(),
            instrs: vec![IrInstr::Const {
                dst: Reg(3),
                value: IrConst::Int(200),
            }],
            terminator: Terminator::Return(Some(Reg(3))),
        };
        let entry = IrBlock {
            label: "entry".to_string(),
            instrs: vec![IrInstr::Const {
                dst: Reg(0),
                value: IrConst::Bool(true),
            }],
            terminator: Terminator::Branch {
                cond: Reg(0),
                true_target: "then".to_string(),
                false_target: "else".to_string(),
            },
        };
        let func = IrFunction {
            name: "main".to_string(),
            type_params: vec![],
            generic_origin: None,
            params: vec![],
            return_type: axiom_typeck::Ty::Int,
            blocks: vec![entry, block_true, block_false],
            next_reg: 4,
            is_extern: false,
        };
        let ir = axiom_ir::Ir {
            functions: vec![func],
            entry: 0,
            enum_variants: std::collections::HashMap::new(),
        };
        let mut vm = Vm::new(ir);
        let result = vm.run().unwrap();
        assert_eq!(result, Value::Int(100));
    }

    #[test]
    fn test_exec_function_call() {
        // main() calls add(3, 4) and returns result
        let add_fn = make_add_fn();
        let main_fn = make_main_calls_add();
        let ir = axiom_ir::Ir {
            functions: vec![add_fn, main_fn],
            entry: 1,
            enum_variants: std::collections::HashMap::new(),
        };
        let mut vm = Vm::new(ir);
        let result = vm.run().unwrap();
        assert_eq!(result, Value::Int(7));
    }

    fn make_add_fn() -> IrFunction {
        IrFunction {
            name: "add".to_string(),
            type_params: vec![],
            generic_origin: None,
            params: vec![
                IrParam {
                    reg: Reg(0),
                    name: "a".to_string(),
                    ty: axiom_typeck::Ty::Int,
                },
                IrParam {
                    reg: Reg(1),
                    name: "b".to_string(),
                    ty: axiom_typeck::Ty::Int,
                },
            ],
            return_type: axiom_typeck::Ty::Int,
            blocks: vec![IrBlock {
                label: "entry".to_string(),
                instrs: vec![IrInstr::BinOp {
                    dst: Reg(2),
                    op: axiom_hir::BinOp::Add,
                    lhs: Reg(0),
                    rhs: Reg(1),
                }],
                terminator: Terminator::Return(Some(Reg(2))),
            }],
            next_reg: 3,
            is_extern: false,
        }
    }

    fn make_main_calls_add() -> IrFunction {
        IrFunction {
            name: "main".to_string(),
            type_params: vec![],
            generic_origin: None,
            params: vec![],
            return_type: axiom_typeck::Ty::Int,
            blocks: vec![IrBlock {
                label: "entry".to_string(),
                instrs: vec![
                    IrInstr::Const {
                        dst: Reg(0),
                        value: IrConst::Int(3),
                    },
                    IrInstr::Const {
                        dst: Reg(1),
                        value: IrConst::Int(4),
                    },
                    IrInstr::Call {
                        dst: Reg(2),
                        function: "add".to_string(),
                        args: vec![Reg(0), Reg(1)],
                    },
                ],
                terminator: Terminator::Return(Some(Reg(2))),
            }],
            next_reg: 3,
            is_extern: false,
        }
    }
}
