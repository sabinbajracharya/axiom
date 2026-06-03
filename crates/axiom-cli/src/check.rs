//! The `check` subcommand's pure core: source → ([CST dump], rendered
//! diagnostics). Reuses `axiom_parser::parse` + `serialize` + `ParseError::render`
//! verbatim — `check` adds no analysis of its own at M0, it just surfaces what
//! lex+parse already produce. Kept side-effect-free so it is trivially testable;
//! `lib.rs` owns the stdout/stderr/exit-code wiring.

use axiom_parser::{parse, serialize};

/// The outcome of checking one source string.
pub struct CheckReport {
    /// The canonical CST dump (the parser's `serialize`), always present.
    pub tree_dump: String,
    /// Human-rendered diagnostics (`line:col: message`); empty means clean.
    pub diagnostics: Vec<String>,
}

impl CheckReport {
    /// Did the source lex + parse with no diagnostics?
    pub fn is_clean(&self) -> bool {
        self.diagnostics.is_empty()
    }
}

/// Lex + parse `source`, returning the CST dump and any rendered diagnostics.
pub fn check_source(source: &str) -> CheckReport {
    let result = parse(source);
    let diagnostics = result.errors.iter().map(|e| e.render(source)).collect();
    CheckReport {
        tree_dump: serialize(&result.tree),
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_clean_source_has_no_diagnostics() {
        let report = check_source("fn main() {\n    val x = 1 + 2\n}\n");
        assert!(report.is_clean(), "diagnostics: {:?}", report.diagnostics);
        assert!(report.tree_dump.contains("SourceFile"));
        assert!(report.tree_dump.contains("FnDef"));
    }

    #[test]
    fn test_check_reports_diagnostics_for_garbage() {
        let report = check_source("fn @ } )) val");
        assert!(!report.is_clean());
        // A tree is still produced — parsing is total.
        assert!(report.tree_dump.contains("SourceFile"));
    }

    #[test]
    fn test_check_empty_source_is_clean() {
        let report = check_source("");
        assert!(report.is_clean());
        assert!(report.tree_dump.contains("SourceFile"));
    }
}
