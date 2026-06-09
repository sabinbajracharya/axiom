# axiom-stdlib

The **embedded standard library** — the single source of truth for stdlib source.

`build.rs` walks `source/` at compile time and bakes every `.ax` module into the crate as
`(module_path, source)` pairs (module path derived from the relative file path exactly as
`modules::discover` does: `core/platform.ax` → `core::platform`). The compiler carries
its own stdlib *inside* the binary — no hardcoded file list to drift, no runtime disk
dependency.

## API
- `modules() -> &'static [(&'static str, &'static str)]` — all bundled modules,
  sorted by module path.
- `with_main(source) -> Vec<(&str, &str)>` — the embedded modules followed by one unnamed
  (`""`) user module: the standard single-file/test module set to hand to
  `driver::check_modules`.

This crate is a **pure leaf** — it composes the module list; the caller drives the
compile pipeline. It does not depend on `typecheck`, so the type checker stays
stdlib-agnostic and there is no dependency cycle.

## Files
- `build.rs` — walks `source/`, emits `$OUT_DIR/stdlib_manifest.rs` (the `STDLIB` table) and
  `cargo:rerun-if-changed` for every `.ax` file.
- `src/lib.rs` — re-exports `STDLIB` via `modules()`; holds the **drift guard** test
  (`test_embedded_matches_disk`) asserting the embedded set equals `discover_library`'s view
  of disk.
- `source/` — the Axiom stdlib sources, organised as `core/` and `std/` following the
  module path convention.

See `docs/stdlib-loading-unification.md` for the why and the full design.
