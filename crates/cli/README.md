# axiom-cli

The **`axiom`** compiler-driver binary — the last crate in the pipeline, and the
plumbing every pipeline stage plugs into.

Two jobs:
- **The command surface.** `axiom <command> [path]`, hand-rolled (no CLI dependency).
  The surface is stable so stages behind it can land one milestone at a time.
  Accepts single `.ax` files or source directories (with `src/main.ax`).
- **The `.ax` feature-test harness** (`harness`) — discovers the `corpus/**`
  programs so tests run every one through the pipeline, classified by expected
  outcome (`valid/` vs `errors/`). The *pattern* is harvested from Oxy;
  re-implemented here with no dependencies.

## Commands

| Command | Status | Does |
|---|---|---|
| `axiom check <path>` | **working** | Full pipeline: lex → parse → lower → resolve → type-check. Prints CST, HIR, THIR canonical dumps to stdout; diagnostics to stderr. |
| `axiom run <path>` | **working** | Full pipeline + monomorphize + IR lowering + VM execution. Prints diagnostics to stderr; program output to stdout. |
| `axiom build <path>` | stubbed | Build a native executable — arrives in **M5** (Cranelift) |
| `axiom help` / `-h` / `--help` | working | Usage |
| `axiom version` / `-V` / `--version` | working | Version |

Both `check` and `run` funnel through `driver::check_modules`, the single
multi-module pipeline. (The package manager / build tool `forge` is a separate
**v2** concern, deliberately not built here.)

### Exit codes

| Code | Meaning |
|---|---|
| `0` | Success (clean compilation / execution) |
| `1` | The source had diagnostics — a *clean* failure, not a crash |
| `2` | Usage error (bad args) or I/O error reading the file |
| `3` | Recognized-but-unimplemented command (`build`) |

## Files

| File | Responsibility | Key items |
|---|---|---|
| `src/main.rs` | Binary shell: args → `run` → exit code | `main` |
| `src/lib.rs` | Driver entry; dispatch + stdout/stderr/exit wiring; help text | `run`, `run_check`, `run_run` |
| `src/cli.rs` | Argument parsing (pure, total) | `Command`, `CliError`, `parse_args` |
| `src/check.rs` | The `check` core: source → CST/HIR/THIR dumps + rendered diagnostics (pure) | `compile_source`, `check_source`, `CheckReport`, `CompileResult` |
| `src/harness.rs` | `.ax` corpus discovery + outcome classification | `corpus_dir`, `discover`, `expects_errors` |
| `tests/features.rs` | Every corpus file matches its expected outcome (input → output loop) | — |
| `tests/fixture_coverage.rs` | Assert every corpus file is exercised by the test harness | — |

## Commands (dev)

```bash
cargo test -p cli                                # full suite incl. the corpus harness
cargo run -p cli -- check corpus/valid/hello.ax   # the debug check
cargo run -p cli -- run   corpus/valid/hello.ax   # compile + execute
cargo run -p cli -- help
```

## When you change this crate

- **Add a subcommand:** one `Command` variant + arm in `cli::parse_args`, one arm
  in `run`, a row in the tables above. Keep `cli` pure and `lib` the only place
  that touches stdout/stderr/exit codes.
- **Add a corpus program:** drop a `*.ax` under `corpus/valid/` or `corpus/errors/`.
  The harness discovers it automatically and `tests/features.rs` asserts the
  outcome for its directory.
