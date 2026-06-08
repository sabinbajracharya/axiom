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

    /// The program's *real* output: the concatenation of every `output` trace
    /// entry (recorded by `write` in `builtins.rs`), in order. Unlike `format`,
    /// this is exactly what the program printed — value-construction and
    /// instruction noise are excluded — so behavioural tests assert on observed
    /// output rather than on substrings of the full execution trace
    /// (`docs/mutable-subscript-design.md` §7 H1).
    pub fn output(&self) -> String {
        let mut out = String::new();
        for entry in &self.entries {
            if entry.fn_name == "output" {
                out.push_str(&entry.instr);
            }
        }
        out
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
