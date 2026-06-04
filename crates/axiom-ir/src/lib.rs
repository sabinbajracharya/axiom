//! axiom-ir — Register-based intermediate representation with CFG.
//!
//! The IR layer bridges type checking → codegen. It consumes the THIR
//! (typed HIR) and produces a register-based IR with explicit basic blocks
//! and terminators. Monomorphized generic instances appear as separate
//! concrete IR functions.
//!
//! See [`docs/ir-design.md`](../../docs/ir-design.md) for the full design.

pub mod invariants;
pub mod ir;
pub mod lower;
pub mod serialize;

pub use invariants::check_invariants;
pub use ir::{
    GenericOrigin, Ir, IrBlock, IrConst, IrFunction, IrInstr, IrParam, IrPattern, MatchArm, Reg,
    Terminator,
};
pub use lower::lower;
pub use serialize::serialize;
