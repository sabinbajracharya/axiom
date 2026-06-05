# axiom-stdlib

The **embedded standard library** — the single source of truth for stdlib source.

`build.rs` walks `stdlib/` at compile time and bakes every `.ax` module into the crate as
`(module_path, source)` pairs (module path derived from the relative file path exactly as
`axiom_modules::discover` does: `core/platform.ax` → `core::platform`). The compiler carries
its own stdlib *inside* the binary — no hardcoded file list to drift, no runtime disk
dependency.

## API
- `modules() -> &'static [(&'static str, &'static str)]` — all bundled modules,
  sorted by module path.

## Files
- `build.rs` — walks `stdlib/`, emits `$OUT_DIR/stdlib_manifest.rs` (the `STDLIB` table) and
  `cargo:rerun-if-changed` for every `.ax` file.
- `src/lib.rs` — re-exports `STDLIB` via `modules()`; holds the **drift guard** test
  (`test_embedded_matches_disk`) asserting the embedded set equals `discover_library`'s view
  of disk.

See `docs/stdlib-loading-unification.md` for the why and the full plan. Later steps add a
`check_with_stdlib` convenience here (depends on `axiom-typeck`).
