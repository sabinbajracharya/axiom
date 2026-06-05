//! Terminator execution: exhaustive match on all Terminator variants.

use axiom_ir::{IrPattern, Terminator};

use crate::error::VmError;
use crate::value::Value;
use crate::Vm;

impl Vm {
    /// Execute the terminator of the current block.
    #[allow(clippy::too_many_lines)]
    pub fn exec_terminator(&mut self) -> Result<Value, VmError> {
        let terminator = {
            let frame = self.current_frame()?;
            let block = &frame.blocks[frame.block_index];
            block.terminator.clone()
        };

        match terminator {
            Terminator::Return(reg) => {
                let val = match reg {
                    Some(r) => self.current_frame()?.read_reg(r)?.clone(),
                    None => Value::Unit,
                };
                let fn_name = self.current_frame()?.fn_name.clone();

                // Record trace.
                let trace_text = match reg {
                    Some(r) => format!("Return %{}", r.0),
                    None => "Return".to_string(),
                };
                self.trace_instr(&fn_name, trace_text);

                // Pop frame.
                self.call_stack.pop();

                // If there's a caller, write the return value to its Call dst
                // and advance past the Call instruction.
                if let Some(caller) = self.call_stack.last_mut() {
                    let idx = caller.instr_index;
                    let call_instr = &caller.blocks[caller.block_index].instrs[idx];
                    if let axiom_ir::IrInstr::Call { dst, .. }
                    | axiom_ir::IrInstr::MethodCall { dst, .. } = call_instr
                    {
                        caller.write_reg(*dst, val.clone())?;
                    }
                    caller.instr_index += 1;
                } else {
                    // Top-level return: store in pending_return.
                    self.pending_return = Some(val.clone());
                }

                Ok(val)
            }

            Terminator::Jump { target } => {
                let idx = self.current_frame()?.resolve_block(&target)?;
                let fn_name = self.current_frame()?.fn_name.clone();
                if let Some(ref mut t) = self.trace {
                    t.record(&fn_name, format!("Jump {target}"), None);
                }
                self.current_frame_mut()?.block_index = idx;
                self.current_frame_mut()?.instr_index = 0;
                Ok(Value::Unit)
            }

            Terminator::Branch {
                cond,
                true_target,
                false_target,
            } => {
                let cond_val = self.current_frame()?.read_reg(cond)?.clone();
                let bool_val = match cond_val {
                    Value::Bool(b) => b,
                    _ => {
                        return Err(VmError::BranchTypeMismatch {
                            got: cond_val.type_name().to_string(),
                        })
                    }
                };
                let target = if bool_val {
                    &true_target
                } else {
                    &false_target
                };
                let idx = self.current_frame()?.resolve_block(target)?;
                let fn_name = self.current_frame()?.fn_name.clone();
                if let Some(ref mut t) = self.trace {
                    t.record(&fn_name, format!("Branch %{} => {target}", cond.0), None);
                }
                self.current_frame_mut()?.block_index = idx;
                self.current_frame_mut()?.instr_index = 0;
                Ok(Value::Unit)
            }

            Terminator::Match {
                scrutinee,
                arms,
                fallback,
            } => {
                let scrutinee_val = self.current_frame()?.read_reg(scrutinee)?.clone();
                let mut matched_target = None;

                for arm in &arms {
                    match &arm.pattern {
                        IrPattern::Wildcard => {
                            matched_target = Some(arm.target.clone());
                            break;
                        }
                        IrPattern::Literal(lit) => {
                            let lit_val = Value::from_const(lit);
                            if scrutinee_val == lit_val {
                                matched_target = Some(arm.target.clone());
                                break;
                            }
                        }
                        IrPattern::Variant {
                            type_name: _,
                            variant,
                            bindings,
                        } => {
                            if let Value::Enum {
                                variant: ref v,
                                ref payload,
                                ..
                            } = scrutinee_val
                            {
                                if v == variant {
                                    // Bind payload to binding registers.
                                    for (i, bind_reg) in bindings.iter().enumerate() {
                                        let val = payload.get(i).cloned().unwrap_or(Value::Unit);
                                        self.current_frame_mut()?.write_reg(*bind_reg, val)?;
                                    }
                                    matched_target = Some(arm.target.clone());
                                    break;
                                }
                            }
                        }
                    }
                }

                let target = matched_target.unwrap_or_else(|| fallback.clone());
                let idx = self.current_frame()?.resolve_block(&target)?;
                let fn_name = self.current_frame()?.fn_name.clone();
                if let Some(ref mut t) = self.trace {
                    t.record(
                        &fn_name,
                        format!("Match %{} => {target}", scrutinee.0),
                        None,
                    );
                }
                self.current_frame_mut()?.block_index = idx;
                self.current_frame_mut()?.instr_index = 0;
                Ok(Value::Unit)
            }

            Terminator::Break { value } => {
                let val = match value {
                    Some(r) => self.current_frame()?.read_reg(r)?.clone(),
                    None => Value::Unit,
                };
                let loop_frame = self
                    .current_frame()?
                    .loop_stack
                    .last()
                    .cloned()
                    .ok_or(VmError::BreakOutsideLoop)?;
                let fn_name = self.current_frame()?.fn_name.clone();
                if let Some(ref mut t) = self.trace {
                    t.record(&fn_name, "Break".to_string(), Some(val.clone()));
                }
                self.current_frame_mut()?.block_index = loop_frame.loop_exit;
                self.current_frame_mut()?.instr_index = 0;
                // Store break value in a special register (reg 0 convention for now).
                // The loop exit block should read it.
                Ok(val)
            }

            Terminator::Continue => {
                let loop_frame = self
                    .current_frame()?
                    .loop_stack
                    .last()
                    .cloned()
                    .ok_or(VmError::ContinueOutsideLoop)?;
                let fn_name = self.current_frame()?.fn_name.clone();
                if let Some(ref mut t) = self.trace {
                    t.record(&fn_name, "Continue".to_string(), None);
                }
                self.current_frame_mut()?.block_index = loop_frame.loop_head;
                self.current_frame_mut()?.instr_index = 0;
                Ok(Value::Unit)
            }

            Terminator::Unreachable => Err(VmError::UnreachableReached),
        }
    }
}
