# axiom-cli

The **`axiom`** compiler-driver binary — the first crate downstream of the
parser, and the plumbing every later pipeline stage plugs into. Built for
**M0** of [`docs/v0-roadmap.md`](../../docs/v0-roadmap.md).

Two jobs:
- **The command surface.** `axiom <command> [file.ax]`, hand-rolled (the v0
  surface is small, so no CLI dependency). The surface is stable now so the
  stages behind it can land one milestone at a time.
- **The `.ax` feature-test harness** (`harness`) — discovers the
  `examples/features/**` corpus so tests run every program through the pipeline.
  The *pattern* is harvested from Oxy; re-implemented here with no dependencies.

## Commands

| Command | Status | Does |
|---|---|---|
| `axiom check <file>` | **working (M0)** | Lex + parse; print the CST to stdout, diagnostics to stderr |
| `axiom run <file>` | stubbed | Run a program — arrives in **M4** (the IR interpreter) |
| `axiom build <file>` | stubbed | Build a native executable — arrives in **M5** (Cranelift) |
| `axiom help` / `-h` / `--help` | working | Usage |
| `axiom version` / `-V` / `--version` | working | Version |

`check` adds no analysis of its own yet — it reuses `axiom_parser::parse`,
`serialize`, and `ParseError::render` verbatim and just surfaces what lex+parse
produce. (The package manager / build tool `forge` is a separate **v2** concern,
deliberately not built here.)

### Exit codes

| Code | Meaning |
|---|---|
| `0` | Success (clean check) |
| `1` | The source had diagnostics — a *clean* failure, not a crash |
| `2` | Usage error (bad args) or I/O error reading the file |
| `3` | Recognized-but-unimplemented command (`run` / `build`) |

## Files

| File | Responsibility | Key items |
|---|---|---|
| `src/main.rs` | Binary shell: args → `run` → exit code | `main` |
| `src/lib.rs` | Driver entry; dispatch + stdout/stderr/exit wiring; help text | `run`, re-exports |
| `src/cli.rs` | Argument parsing (pure, total) | `Command`, `CliError`, `parse_args` |
| `src/check.rs` | The `check` core: source → CST dump + rendered diagnostics (pure) | `check_source`, `CheckReport` |
| `src/harness.rs` | `.ax` corpus discovery for the feature tests | `features_dir`, `discover` |
| `tests/features.rs` | Every corpus file lex+parses clean (input → output loop) | — |

## Commands (dev)

```bash
cargo test -p axiom-cli                          # full suite incl. the corpus harness
cargo run -p axiom-cli -- check examples/features/hello.ax   # the debug check
cargo run -p axiom-cli -- help
```

## When you change this crate

- **Add a subcommand:** one `Command` variant + arm in `cli::parse_args`, one arm
  in `run`, a row in the tables above. Keep `cli` pure and `lib` the only place
  that touches stdout/stderr/exit codes.
- **Add a corpus program:** drop a `*.ax` under `examples/features/`. The harness
  discovers it automatically; `tests/features.rs` will check it clean (so it must
  parse with zero diagnostics until later stages give it more meaning).
