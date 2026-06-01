#!/usr/bin/env bash
# Axiom enforcement hook (LAYERS 1-3). Invoked by the Claude Code PostToolUse hook
# (.claude/settings.json) after a file is edited, and usable standalone / in CI.
#
# Behavior:
#   - When run as a hook, reads the tool payload (JSON) on stdin and only acts if a
#     `.rs` file was edited (so editing docs never triggers a compile).
#   - Runs `cargo fmt --all --check` and `cargo clippy --all-targets -- -D warnings`.
#   - On any failure, prints the reason to stderr and exits 2, which Claude Code
#     feeds back to the model as a problem it must fix before continuing.
#
# See ENFORCEMENT.md and RUST_CONVENTIONS.md.
set -uo pipefail

PROJECT_DIR="${CLAUDE_PROJECT_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"

# If a payload is piped in (hook mode), only act on Rust-file edits.
# If stdin is a terminal (manual/CI run), skip this gate and always check.
if [ ! -t 0 ]; then
    payload="$(cat 2>/dev/null || true)"
    if [ -n "$payload" ] &&
        ! printf '%s' "$payload" | grep -qE '"file_path"[[:space:]]*:[[:space:]]*"[^"]*\.rs"'; then
        exit 0
    fi
fi

cd "$PROJECT_DIR" || exit 0

# Nothing to check until there is Rust source in the tree.
if ! find . -path ./target -prune -o -name '*.rs' -print 2>/dev/null | grep -q .; then
    exit 0
fi

# The Rust toolchain must be available to run the checks. If `cargo` is not on
# PATH (e.g. this project builds via Docker like Oxy, or the toolchain isn't
# installed on the host), skip rather than spuriously block every edit.
# To make enforcement actually bite locally: install rustup + clippy on the host,
# OR replace the two `cargo` invocations below with a Docker wrapper
# (e.g. `docker compose run --rm dev bash -c "cargo ..."`). CI always has cargo.
if ! command -v cargo >/dev/null 2>&1; then
    echo "⚠️  axiom/check.sh: cargo not found — skipping fmt/clippy enforcement on this host. See ENFORCEMENT.md." >&2
    exit 0
fi

fail=0
report=""

if ! fmt_out="$(cargo fmt --all -- --check 2>&1)"; then
    report+=$'\n❌ rustfmt: code is not formatted. Run `cargo fmt --all`.\n'"$fmt_out"
    fail=1
fi

if ! clippy_out="$(cargo clippy --all-targets --all-features -- -D warnings 2>&1)"; then
    report+=$'\n❌ clippy: lint violations (warnings are errors). Fix per RUST_CONVENTIONS.md.\n'"$clippy_out"
    fail=1
fi

if [ "$fail" -ne 0 ]; then
    printf '%s\n' "$report" >&2
    exit 2
fi

exit 0
