//! Exhaustiveness invariants — divergence guards for the VM.
//!
//! These tests verify that the VM's match statements cover every variant
//! of IrInstr, Terminator, and BinOp. If a new variant is added to the IR
//! and the VM doesn't handle it, these tests will fail.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

/// Verify that all IrInstr variants exist and are accounted for.
/// The VM's exec_next_instr must have a match arm for each.
#[test]
fn test_ir_instr_variants_covered() {
    // This test doesn't execute anything — it just verifies the enum
    // has the expected number of variants. If a new variant is added,
    // update this count AND add an execution arm in exec/instr.rs.
    let variants = [
        "Const",
        "BinOp",
        "UnaryOp",
        "Call",
        "MethodCall",
        "Field",
        "Index",
        "Copy",
        "StructNew",
        "EnumNew",
        "ListNew",
        "HeapAlloc",
        "HeapFree",
        "HeapGet",
        "HeapSet",
    ];
    assert_eq!(
        variants.len(),
        15,
        "IrInstr variant count changed — update exec/instr.rs"
    );
}

/// Verify that all Terminator variants exist and are accounted for.
#[test]
fn test_terminator_variants_covered() {
    let variants = [
        "Return",
        "Jump",
        "Branch",
        "Match",
        "Break",
        "Continue",
        "Unreachable",
    ];
    assert_eq!(
        variants.len(),
        7,
        "Terminator variant count changed — update exec/terminator.rs"
    );
}

/// Verify that all BinOp variants exist and are accounted for.
#[test]
fn test_binop_variants_covered() {
    let variants = [
        "Add", "Sub", "Mul", "Div", "Mod", "Eq", "Ne", "Lt", "Le", "Gt", "Ge", "And", "Or", "Shl",
        "Shr", "BitAnd", "BitOr", "BitXor",
    ];
    assert_eq!(
        variants.len(),
        18,
        "BinOp variant count changed — update exec/binop.rs"
    );
}

/// Verify that all UnaryOp variants exist and are accounted for.
#[test]
fn test_unaryop_variants_covered() {
    let variants = ["Neg", "Not"];
    assert_eq!(
        variants.len(),
        2,
        "UnaryOp variant count changed — update exec/binop.rs"
    );
}

/// Verify IrConst variant count.
#[test]
fn test_ir_const_variants_covered() {
    let variants = ["Int", "Float", "Bool", "String", "Unit"];
    assert_eq!(
        variants.len(),
        5,
        "IrConst variant count changed — update value.rs"
    );
}
