//! IR type definitions.
//!
//! This module defines the register-based intermediate representation:
//! [`Ir`] (the program), [`IrFunction`], [`IrBlock`], [`IrInstr`],
//! [`Terminator`], and [`Reg`] (virtual registers).

use std::collections::HashMap;

use axiom_hir::{BinOp, UnaryOp};
use axiom_typeck::Ty;

// ── The program ──────────────────────────────────────────────────────────────

/// The complete IR program: all functions + entry point index.
#[derive(Debug, Clone)]
pub struct Ir {
    pub functions: Vec<IrFunction>,
    pub entry: usize,
    /// Maps enum variant name → (enum type name, payload field count).
    /// Populated during IR lowering so the VM can distinguish enum
    /// constructor calls from function calls.
    pub enum_variants: HashMap<String, (String, usize)>,
}

// ── Functions ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct IrFunction {
    pub name: String,
    pub type_params: Vec<String>,
    pub generic_origin: Option<GenericOrigin>,
    pub params: Vec<IrParam>,
    pub return_type: Ty,
    pub blocks: Vec<IrBlock>,
    pub next_reg: u32,
}

#[derive(Debug, Clone)]
pub struct IrParam {
    pub reg: Reg,
    pub name: String,
    pub ty: Ty,
}

/// Links a monomorphized instance back to its generic definition.
#[derive(Debug, Clone)]
pub struct GenericOrigin {
    pub generic_name: String,
    pub concrete_args: Vec<Ty>,
}

// ── Registers ────────────────────────────────────────────────────────────────

/// A virtual register. Assigned once per block (SSA-like).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Reg(pub u32);

impl std::fmt::Display for Reg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "%{}", self.0)
    }
}

// ── Basic blocks ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct IrBlock {
    pub label: String,
    pub instrs: Vec<IrInstr>,
    pub terminator: Terminator,
}

// ── Instructions ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum IrInstr {
    /// r = literal constant
    Const { dst: Reg, value: IrConst },
    /// r = op lhs rhs
    BinOp {
        dst: Reg,
        op: BinOp,
        lhs: Reg,
        rhs: Reg,
    },
    /// r = op src
    UnaryOp { dst: Reg, op: UnaryOp, src: Reg },
    /// r = function(args...)
    Call {
        dst: Reg,
        function: String,
        args: Vec<Reg>,
    },
    /// r = receiver.method(args...)
    MethodCall {
        dst: Reg,
        receiver: Reg,
        method: String,
        args: Vec<Reg>,
    },
    /// r = base.field
    Field { dst: Reg, base: Reg, field: String },
    /// r = base[index]
    Index { dst: Reg, base: Reg, index: Reg },
    /// register copy
    Copy { dst: Reg, src: Reg },
    /// r = Type { field1: v1, field2: v2, ... }
    StructNew {
        dst: Reg,
        type_name: String,
        fields: Vec<(String, Reg)>,
    },
    /// r = Variant(payload...)
    EnumNew {
        dst: Reg,
        type_name: String,
        variant: String,
        payload: Vec<Reg>,
    },
    /// r = [elem1, elem2, ...]
    ListNew { dst: Reg, elements: Vec<Reg> },
    /// r = heap_alloc(count) — allocate a buffer for `count` elements, return pointer.
    HeapAlloc { dst: Reg, count: Reg },
    /// heap_free(ptr) — free a heap-allocated buffer.
    HeapFree { ptr: Reg },
    /// r = heap_get(ptr, index) — read element at index from buffer.
    HeapGet { dst: Reg, ptr: Reg, index: Reg },
    /// heap_set(ptr, index, value) — write value at index in buffer.
    HeapSet { ptr: Reg, index: Reg, value: Reg },
}

#[derive(Debug, Clone, PartialEq)]
pub enum IrConst {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Unit,
}

// ── Terminators ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Terminator {
    /// Return from function (with optional value register).
    Return(Option<Reg>),
    /// Unconditional jump.
    Jump { target: String },
    /// Conditional branch.
    Branch {
        cond: Reg,
        true_target: String,
        false_target: String,
    },
    /// Pattern match.
    Match {
        scrutinee: Reg,
        arms: Vec<MatchArm>,
        fallback: String,
    },
    /// Break from loop.
    Break { value: Option<Reg> },
    /// Continue loop iteration.
    Continue,
    /// Unreachable (after diverging expressions).
    Unreachable,
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: IrPattern,
    pub target: String,
}

#[derive(Debug, Clone)]
pub enum IrPattern {
    Wildcard,
    Literal(IrConst),
    Variant {
        type_name: String,
        variant: String,
        bindings: Vec<Reg>,
    },
}
