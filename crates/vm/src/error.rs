//! VM error types.

/// All errors the VM can produce.
#[derive(Debug, thiserror::Error)]
pub enum VmError {
    #[error("function not found: {name}")]
    FunctionNotFound { name: String },

    #[error("undefined register %{0}")]
    UndefinedReg(u32),

    #[error("undefined block label: {label}")]
    UndefinedBlock { label: String },

    #[error("empty call stack — cannot return")]
    EmptyCallStack,

    #[error("break outside of loop")]
    BreakOutsideLoop,

    #[error("continue outside of loop")]
    ContinueOutsideLoop,

    #[error("heap slot {0} is not allocated")]
    HeapSlotFreed(usize),

    #[error("heap index {index} out of bounds (len {len})")]
    HeapIndexOutOfBounds { index: usize, len: usize },

    #[error("division by zero")]
    DivisionByZero,

    #[error("unreachable instruction executed")]
    UnreachableReached,

    #[error("expected {expected} args, got {got}")]
    ArityMismatch { expected: usize, got: usize },

    #[error("expected Bool for branch condition, got {got}")]
    BranchTypeMismatch { got: String },

    #[error("no match arm for value {value}")]
    MatchFallthrough { value: String },

    #[error("builtin not found: {name}")]
    BuiltinNotFound { name: String },

    #[error("extern function not implemented in the VM: {name}")]
    ExternNotImplemented { name: String },

    #[error("type error: expected {expected}, got {got}")]
    TypeError { expected: String, got: String },

    #[error(
        "indexed {op} on an unsupported base ({got}): only a `[T]` heap buffer \
         is indexable by the primitive `Index`/`IndexSet` — library collections \
         must lower `base[i]` to a subscript"
    )]
    UnsupportedIndexBase { op: &'static str, got: String },

    #[error("execution step limit exceeded ({limit}) — likely an infinite loop")]
    StepLimitExceeded { limit: u64 },
}
