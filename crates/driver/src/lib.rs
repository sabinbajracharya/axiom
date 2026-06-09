//! Pipeline orchestrator — the single multi-module compilation driver.
//!
//! `check_modules` is the entry point: parse → lower → resolve → annotation
//! validation → type-check. All compilation paths (single-file, project, stdlib-
//! backed tests) funnel through here — they differ only in *which* modules they
//! pass. See `docs/stdlib-loading-unification.md`.

/// Compile a set of `(module_name, source)` modules together into one `Thir`.
///
/// This is the **single multi-module pipeline**: structural lowering (with linear
/// DefIds across modules) → cross-module export building → name resolution →
/// type-checking the combined HIR. Single-file, project, and stdlib-backed test
/// compilation all funnel through here — they differ only in *which* modules they
/// pass. See `docs/stdlib-loading-unification.md`.
pub fn check_modules(modules: &[(&str, &str)]) -> typecheck::Thir {
    use parser::ast::AstNode;

    type Lowered = (
        String,
        Vec<resolver::Item>,
        Vec<resolver::Def>,
        Vec<resolver::HirDiagnostic>,
    );

    let mut lowered: Vec<Lowered> = Vec::new();
    let mut next_id = 0usize;
    for (name, source) in modules {
        let result = parser::parse(source);
        let Some(root) = parser::ast::SourceFile::cast(result.tree) else {
            continue;
        };
        let (items, defs, diags, nid) = resolver::lower_structural(&root, source, next_id);
        next_id = nid;
        lowered.push(((*name).to_string(), items, defs, diags));
    }

    let export_input: Vec<(String, Vec<resolver::Def>)> = lowered
        .iter()
        .map(|(name, _, defs, _)| (name.clone(), defs.clone()))
        .collect();
    let exports = resolver::build_global_exports(&export_input);

    let mut all_items: Vec<resolver::Item> = Vec::new();
    let mut all_diags: Vec<resolver::HirDiagnostic> = Vec::new();
    let mut stdlib_bindings: Vec<resolver::LangBinding> = Vec::new();
    let mut stdlib_present = false;
    for (name, items, defs, diags) in &mut lowered {
        let mut items = std::mem::take(items);
        let mut diagnostics = std::mem::take(diags);
        resolver::resolve_with_globals(&mut items, defs, &mut diagnostics, &exports, name);
        if is_stdlib_module(name) {
            stdlib_present = true;
        }
        validate_module_annotations(
            &items,
            is_stdlib_module(name),
            &mut stdlib_bindings,
            &mut diagnostics,
        );
        all_diags.append(&mut diagnostics);
        all_items.append(&mut items);
    }

    let (lang_items, mut lang_diags) =
        resolver::resolve_lang_items(&stdlib_bindings, stdlib_present);
    all_diags.append(&mut lang_diags);

    let mut hir = resolver::Hir {
        items: all_items,
        diagnostics: all_diags,
    };
    let max_id = typecheck::hir_max_id(&hir);
    let _next_id = desugar::pre_typecheck(&mut hir, &lang_items, max_id + 1);
    let mut thir = typecheck::check_with_lang_items(hir, lang_items);
    let max_id = typecheck::hir_max_id(&thir.hir);
    desugar::post_typecheck(&mut thir.hir, &thir.types, max_id + 1);
    thir
}

/// Bare type-check — the deliberate, **labeled** no-stdlib mode: the user source
/// as one module with NO stdlib loaded. For compiler-isolation unit tests and the
/// floor built-ins that legitimately stay. It is the *same* `check_modules`
/// pipeline with an empty stdlib input (module name `""`), not a separate path —
/// so it cannot diverge. See `docs/stdlib-loading-unification.md` §3.
pub fn check_source(source: &str) -> typecheck::Thir {
    check_modules(&[("", source)])
}

/// Whether a module path belongs to the embedded standard library. Delegates to
/// `stdlib::is_stdlib_module` which checks the build-time verified set of
/// known stdlib module paths. See `docs/intrinsic-and-stdlib-identity.md` §2a.
fn is_stdlib_module(name: &str) -> bool {
    stdlib::is_stdlib_module(name)
}

/// Validate `@lang` and `@intrinsic` annotations for one lowered module.
/// Stdlib modules may use both; non-stdlib modules may use neither.
/// Accumulates lang-item bindings for later registry consistency checks.
fn validate_module_annotations(
    items: &[resolver::Item],
    is_stdlib: bool,
    stdlib_bindings: &mut Vec<resolver::LangBinding>,
    diagnostics: &mut Vec<resolver::HirDiagnostic>,
) {
    // ── @lang ────────────────────────────────────────────────────────────
    let lang_bindings = resolver::collect_lang_bindings(items);
    if is_stdlib {
        stdlib_bindings.extend(lang_bindings);
    } else {
        for b in lang_bindings {
            diagnostics.push(resolver::HirDiagnostic::LangItemOutsideStdlib {
                key: b.key,
                span: lexer::Span { lo: 0, hi: 0 },
            });
        }
    }

    // ── @intrinsic ───────────────────────────────────────────────────────
    let intrinsic_bindings = resolver::collect_intrinsic_bindings(items);
    if is_stdlib {
        diagnostics.append(&mut resolver::validate_intrinsic_bindings(
            &intrinsic_bindings,
        ));
    } else {
        for b in intrinsic_bindings {
            diagnostics.push(resolver::HirDiagnostic::IntrinsicOutsideStdlib {
                key: b.key.clone(),
                span: lexer::Span { lo: 0, hi: 0 },
            });
        }
    }
}
