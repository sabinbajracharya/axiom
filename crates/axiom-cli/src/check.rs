//! The `check` subcommand's pure core: source → (CST dump, HIR dump, THIR dump,
//! rendered diagnostics). Runs lex + parse + HIR lowering + name resolution +
//! type checking, producing the CST, HIR, and THIR canonical dumps plus all
//! diagnostics (parse + HIR + type).
//!
//! Kept side-effect-free so it is trivially testable; `lib.rs` owns the
//! stdout/stderr/exit-code wiring.
//!
//! At **M2** the `check` command runs the full pipeline through type checking.
//! Type errors produce `TypeDiagnostic`s in the report alongside HIR diagnostics.

use axiom_hir::{lower, serialize as hir_serialize, HirDiagnostic};
use axiom_parser::ast::AstNode;
use axiom_parser::{parse, serialize as cst_serialize};
use axiom_typeck::{check as typeck_check, serialize as thir_serialize, Thir, TypeDiagnostic};

/// The outcome of checking one source string.
pub struct CheckReport {
    /// The canonical CST dump (the parser's `serialize`), always present.
    pub tree_dump: String,
    /// The canonical HIR dump (resolved names → def IDs), always present.
    pub hir_dump: String,
    /// The canonical THIR dump (HIR + type annotations), always present.
    pub thir_dump: String,
    /// Human-rendered diagnostics (`line:col: message`); combines parse + HIR
    /// + type diagnostics. Empty means clean.
    pub diagnostics: Vec<String>,
}

impl CheckReport {
    /// Did the source parse, lower, and type-check with no diagnostics?
    pub fn is_clean(&self) -> bool {
        self.diagnostics.is_empty()
    }
}

/// Full pipeline result: the check report plus the typed IR (if parse/lower/typecheck succeeded).
pub struct CompileResult {
    pub report: CheckReport,
    /// The THIR, present when parse + HIR lowering + type checking all succeeded
    /// (diagnostics may still be non-empty from earlier stages).
    pub thir: Option<Thir>,
}

/// Lex + parse + lower + type-check `source`, returning both the diagnostic
/// report and the THIR (for downstream IR lowering / execution).
pub fn compile_source(source: &str) -> CompileResult {
    let result = parse(source);
    let mut diagnostics: Vec<String> = result.errors.iter().map(|e| e.render(source)).collect();
    let tree_dump = cst_serialize(&result.tree);

    let root = match axiom_parser::ast::SourceFile::cast(result.tree) {
        Some(r) => r,
        None => {
            return CompileResult {
                report: CheckReport {
                    tree_dump,
                    hir_dump: String::new(),
                    thir_dump: String::new(),
                    diagnostics,
                },
                thir: None,
            };
        }
    };
    let hir = lower(&root, source);
    for diag in &hir.diagnostics {
        diagnostics.push(HirDiagnostic::render(diag, source));
    }
    let hir_dump = hir_serialize(&hir);

    let thir = typeck_check(hir);
    for diag in &thir.diagnostics {
        diagnostics.push(TypeDiagnostic::render(diag, source));
    }
    let thir_dump = thir_serialize(&thir, None);

    CompileResult {
        report: CheckReport {
            tree_dump,
            hir_dump,
            thir_dump,
            diagnostics,
        },
        thir: Some(thir),
    }
}

/// Lex + parse + lower + type-check `source`, returning the CST dump, HIR dump,
/// THIR dump, and any rendered diagnostics (parse errors + HIR + type).
pub fn check_source(source: &str) -> CheckReport {
    compile_source(source).report
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
        assert!(report.hir_dump.contains("FnDef"));
        assert!(report.hir_dump.contains("name=main"));
        assert!(report.thir_dump.contains("FnDef"));
    }

    #[test]
    fn test_check_reports_diagnostics_for_garbage() {
        let report = check_source("fn @ } )) val");
        assert!(!report.is_clean());
        assert!(report.tree_dump.starts_with("SourceFile @"));
        assert!(report.tree_dump.contains("KwFn"));
    }

    #[test]
    fn test_check_empty_source_is_clean() {
        let report = check_source("");
        assert!(report.is_clean());
        assert!(report.tree_dump.contains("SourceFile"));
    }

    #[test]
    fn test_check_hir_dump_includes_fn_def() {
        let report = check_source("fn main() { val x = 1 + 2 }");
        assert!(report.is_clean(), "diagnostics: {:?}", report.diagnostics);
        assert!(report.hir_dump.contains("FnDef"));
        assert!(report.hir_dump.contains("name=main"));
    }

    #[test]
    fn test_check_unresolved_name_in_hir() {
        let report = check_source("fn main() { val x = unknown_var }");
        assert!(
            !report.diagnostics.is_empty(),
            "expected unresolved diagnostic"
        );
        assert!(report.hir_dump.contains("unknown_var"));
        assert!(report.hir_dump.contains("unresolved"));
    }

    #[test]
    fn test_check_thir_dump_includes_types() {
        let report = check_source("fn main() { val x = 1 }");
        assert!(report.thir_dump.contains("Int"));
    }

    #[test]
    fn test_check_type_error_in_diagnostics() {
        let report = check_source("fn main() { val x: Int = 3.14 }");
        assert!(!report.is_clean(), "expected type mismatch diagnostic");
        assert!(report
            .diagnostics
            .iter()
            .any(|d| d.contains("type mismatch")));
    }
}
