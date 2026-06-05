//! Instruction execution: exhaustive match on all IrInstr variants.

use axiom_ir::IrInstr;

use crate::error::VmError;
use crate::value::Value;
use crate::Vm;

impl Vm {
    /// Execute the next instruction in the current frame.
    ///
    /// For Call/MethodCall: pushes the callee frame and returns immediately
    /// (does NOT execute the callee). The outer loop handles that.
    /// For all other instructions: executes and advances instr_index.
    #[allow(clippy::too_many_lines)]
    pub fn exec_next_instr(&mut self) -> Result<(), VmError> {
        // Clone the instruction and fn_name to release the immutable borrow.
        let (instr, fn_name) = {
            let frame = self.current_frame()?;
            let block = &frame.blocks[frame.block_index];
            (
                block.instrs[frame.instr_index].clone(),
                frame.fn_name.clone(),
            )
        };

        match instr {
            IrInstr::Const { dst, value } => {
                let val = Value::from_const(&value);
                self.write_and_advance(
                    dst,
                    val,
                    &fn_name,
                    format!("%{} = Const({:?})", dst.0, value),
                )?;
            }
            IrInstr::BinOp { dst, op, lhs, rhs } => {
                let l = self.current_frame()?.read_reg(lhs)?.clone();
                let r = self.current_frame()?.read_reg(rhs)?.clone();
                let val = crate::exec::binop::exec_binop(op, &l, &r)?;
                let text = format!("%{} = BinOp {op:?}(%{}, %{})", dst.0, lhs.0, rhs.0);
                self.write_and_advance(dst, val, &fn_name, text)?;
            }
            IrInstr::UnaryOp { dst, op, src } => {
                let s = self.current_frame()?.read_reg(src)?.clone();
                let val = crate::exec::binop::exec_unaryop(op, &s)?;
                self.write_and_advance(
                    dst,
                    val,
                    &fn_name,
                    format!("%{} = UnaryOp {op:?}(%{})", dst.0, src.0),
                )?;
            }
            IrInstr::Call {
                dst,
                function,
                args,
            } => {
                let arg_vals: Vec<Value> = {
                    let frame = self.current_frame()?;
                    args.iter()
                        .map(|r| frame.read_reg(*r).cloned())
                        .collect::<Result<_, _>>()?
                };

                // Builtins execute inline (no frame push).
                if crate::exec::builtins::is_builtin(&function) {
                    let val =
                        crate::exec::builtins::call_builtin(&function, arg_vals, &mut self.trace)?;
                    self.write_and_advance(dst, val, &fn_name, String::new())?;
                    return Ok(());
                }

                // Enum constructor: the IR lowerer emits Call for enum
                // variants (e.g. Circle(5)). Check enum_variants to
                // distinguish from real function calls.
                if let Some((type_name, _)) = self.ir.enum_variants.get(&function) {
                    let val = Value::Enum {
                        type_name: type_name.clone(),
                        variant: function.clone(),
                        payload: arg_vals,
                    };
                    let text = format!("%{} = {function}(...)", dst.0);
                    self.write_and_advance(dst, val, &fn_name, text)?;
                    return Ok(());
                }

                // Push callee frame. instr_index stays at the Call —
                // Return will advance it when the callee finishes.
                self.push_frame(&function, arg_vals)?;
                let text = format!("Call {function}({})", fmt_regs(&args));
                self.trace_instr(&fn_name, text);
            }
            IrInstr::MethodCall {
                dst,
                receiver,
                method,
                args,
            } => {
                let recv = self.current_frame()?.read_reg(receiver)?.clone();
                let mut all_args = vec![recv];
                {
                    let frame = self.current_frame()?;
                    for r in &args {
                        all_args.push(frame.read_reg(*r)?.clone());
                    }
                }

                if crate::exec::builtins::is_builtin(&method) {
                    let val =
                        crate::exec::builtins::call_builtin(&method, all_args, &mut self.trace)?;
                    self.write_and_advance(dst, val, &fn_name, String::new())?;
                    return Ok(());
                }

                self.push_frame(&method, all_args)?;
                let text = format!("MethodCall %{}.{}({})", receiver.0, method, fmt_regs(&args));
                self.trace_instr(&fn_name, text);
            }
            IrInstr::Field { dst, base, field } => {
                let base_val = self.current_frame()?.read_reg(base)?.clone();
                let val = match &base_val {
                    Value::Struct { fields, .. } => fields
                        .iter()
                        .find(|(name, _)| name == &field)
                        .map(|(_, v)| v.clone())
                        .unwrap_or(Value::Unit),
                    _ => Value::Unit,
                };
                self.write_and_advance(
                    dst,
                    val,
                    &fn_name,
                    format!("%{} = Field %{}.{}", dst.0, base.0, field),
                )?;
            }
            IrInstr::Index { dst, base, index } => {
                let base_val = self.current_frame()?.read_reg(base)?.clone();
                let idx_val = self.current_frame()?.read_reg(index)?.clone();
                let val = match (&base_val, &idx_val) {
                    (Value::List(items), Value::Int(i)) => {
                        items.get(*i as usize).cloned().unwrap_or(Value::Unit)
                    }
                    (Value::HeapPtr(addr), Value::Int(i)) => {
                        self.heap.get(*addr, *i as usize)?.clone()
                    }
                    _ => Value::Unit,
                };
                self.write_and_advance(
                    dst,
                    val,
                    &fn_name,
                    format!("%{} = Index %{}[%{}]", dst.0, base.0, index.0),
                )?;
            }
            IrInstr::Copy { dst, src } => {
                let val = self.current_frame()?.read_reg(src)?.clone();
                self.write_and_advance(
                    dst,
                    val,
                    &fn_name,
                    format!("%{} = Copy %{}", dst.0, src.0),
                )?;
            }
            IrInstr::StructNew {
                dst,
                type_name,
                fields,
            } => {
                let field_vals: Vec<(String, Value)> = {
                    let frame = self.current_frame()?;
                    fields
                        .iter()
                        .map(|(name, reg)| Ok((name.clone(), frame.read_reg(*reg)?.clone())))
                        .collect::<Result<_, VmError>>()?
                };
                let val = Value::Struct {
                    type_name: type_name.clone(),
                    fields: field_vals,
                };
                self.write_and_advance(
                    dst,
                    val,
                    &fn_name,
                    format!("%{} = StructNew {type_name}(...)", dst.0),
                )?;
            }
            IrInstr::EnumNew {
                dst,
                type_name,
                variant,
                payload,
            } => {
                let payload_vals: Vec<Value> = {
                    let frame = self.current_frame()?;
                    payload
                        .iter()
                        .map(|r| frame.read_reg(*r).cloned())
                        .collect::<Result<_, _>>()?
                };
                let val = Value::Enum {
                    type_name: type_name.clone(),
                    variant: variant.clone(),
                    payload: payload_vals,
                };
                let text = format!(
                    "%{} = EnumNew {}.{}({})",
                    dst.0,
                    type_name,
                    variant,
                    fmt_regs(&payload)
                );
                self.write_and_advance(dst, val, &fn_name, text)?;
            }
            IrInstr::ListNew { dst, elements } => {
                let vals: Vec<Value> = {
                    let frame = self.current_frame()?;
                    elements
                        .iter()
                        .map(|r| frame.read_reg(*r).cloned())
                        .collect::<Result<_, _>>()?
                };
                self.write_and_advance(
                    dst,
                    Value::List(vals),
                    &fn_name,
                    format!("%{} = ListNew [{}]", dst.0, fmt_regs(&elements)),
                )?;
            }
            IrInstr::HeapAlloc { dst, count } => {
                let n = match self.current_frame()?.read_reg(count)? {
                    Value::Int(c) => *c as usize,
                    _ => 0,
                };
                let data = vec![Value::Unit; n];
                let addr = self.heap.alloc(data);
                self.write_and_advance(
                    dst,
                    Value::HeapPtr(addr),
                    &fn_name,
                    format!("%{} = HeapAlloc({n})", dst.0),
                )?;
            }
            IrInstr::HeapFree { ptr } => {
                let addr = match self.current_frame()?.read_reg(ptr)? {
                    Value::HeapPtr(a) => *a,
                    _ => 0,
                };
                self.heap.free(addr)?;
                self.trace_instr(&fn_name, format!("HeapFree(%{})", ptr.0));
                self.current_frame_mut()?.instr_index += 1;
            }
            IrInstr::HeapGet { dst, ptr, index } => {
                let addr = match self.current_frame()?.read_reg(ptr)? {
                    Value::HeapPtr(a) => *a,
                    _ => 0,
                };
                let idx = match self.current_frame()?.read_reg(index)? {
                    Value::Int(i) => *i as usize,
                    _ => 0,
                };
                let val = self.heap.get(addr, idx)?.clone();
                self.write_and_advance(
                    dst,
                    val,
                    &fn_name,
                    format!("%{} = HeapGet %{}[%{}]", dst.0, ptr.0, index.0),
                )?;
            }
            IrInstr::HeapSet { ptr, index, value } => {
                let addr = match self.current_frame()?.read_reg(ptr)? {
                    Value::HeapPtr(a) => *a,
                    _ => 0,
                };
                let idx = match self.current_frame()?.read_reg(index)? {
                    Value::Int(i) => *i as usize,
                    _ => 0,
                };
                let val = self.current_frame()?.read_reg(value)?.clone();
                self.heap.set(addr, idx, val)?;
                self.trace_instr(
                    &fn_name,
                    format!("HeapSet %{}[%{}] = %{}", ptr.0, index.0, value.0),
                );
                self.current_frame_mut()?.instr_index += 1;
            }
            IrInstr::VariantPayload {
                dst,
                scrutinee,
                index,
            } => {
                let scrutinee_val = self.current_frame()?.read_reg(scrutinee)?.clone();
                let payload_val = match scrutinee_val {
                    Value::Enum { payload, .. } => {
                        payload.get(index).cloned().unwrap_or(Value::Unit)
                    }
                    _ => Value::Unit,
                };
                self.write_and_advance(
                    dst,
                    payload_val,
                    &fn_name,
                    format!("VariantPayload %{} [{}]", scrutinee.0, index),
                )?;
            }
        }

        Ok(())
    }

    /// Write a value to dst register, record trace, advance instr_index.
    fn write_and_advance(
        &mut self,
        dst: axiom_ir::Reg,
        val: Value,
        fn_name: &str,
        text: String,
    ) -> Result<(), VmError> {
        self.current_frame_mut()?.write_reg(dst, val.clone())?;
        self.trace_instr(fn_name, text);
        self.current_frame_mut()?.instr_index += 1;
        Ok(())
    }

    /// Record a trace entry (no-op if tracing is disabled).
    pub(crate) fn trace_instr(&mut self, fn_name: &str, text: String) {
        if let Some(ref mut t) = self.trace {
            t.record(fn_name, text, None);
        }
    }
}

fn fmt_regs(regs: &[axiom_ir::Reg]) -> String {
    regs.iter()
        .map(|r| format!("%{}", r.0))
        .collect::<Vec<_>>()
        .join(", ")
}
