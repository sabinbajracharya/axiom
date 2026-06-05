//! Execution trace: records every instruction for debugging.

use crate::value::Value;

/// A single trace entry.
#[derive(Debug, Clone)]
pub struct TraceEntry {
    pub fn_name: String,
    pub instr: String,
    pub result: Option<Value>,
}

/// Records execution steps for debugging and golden tests.
#[derive(Debug, Default)]
pub struct ExecutionTrace {
    pub entries: Vec<TraceEntry>,
}

impl ExecutionTrace {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Record an instruction execution.
    pub fn record(&mut self, fn_name: &str, instr: String, result: Option<Value>) {
        self.entries.push(TraceEntry {
            fn_name: fn_name.to_string(),
            instr,
            result,
        });
    }

    /// Format the trace as text (one line per entry).
    pub fn format(&self) -> String {
        let mut out = String::new();
        for entry in &self.entries {
            out.push_str(&format!("[fn {}] {}", entry.fn_name, entry.instr));
            if let Some(val) = &entry.result {
                out.push_str(&format!(" => {val}"));
            }
            out.push('\n');
        }
        out
    }
}
