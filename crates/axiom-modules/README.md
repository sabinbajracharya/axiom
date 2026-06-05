# axiom-modules

Module graph construction and file discovery for multi-file Axiom projects.

## What this does

Given a source directory, `discover()` scans for `.ax` files, builds a
`ModuleGraph` capturing parent/child relationships, and validates the structure:

- `main.ax` at the root is the entry point (required).
- `foo.ax` + `foo/` siblings → children attach to the `foo` module.
- `foo/mod.ax` inside `foo/` → the directory IS the module.
- Error on `foo.ax` + `foo/mod.ax` conflicts, name collisions, missing `main.ax`.

## What this does NOT do (yet)

- Cross-module name resolution (Phase 2).
- Multi-file compilation pipeline (Phase 3).
- Prelude auto-import (Phase 4).

## Key types

- `ModuleGraph` — the full graph; `modules` list + `root` id.
- `ModuleEntry` — one module: path, name, source, parent, children, visibility.
- `ModuleId` — opaque handle into the graph.
- `ModuleError` — all error variants from discovery.
