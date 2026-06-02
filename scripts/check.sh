#!/usr/bin/env bash
# Axiom enforcement hook (LAYERS 1-4). Invoked by the Claude Code PostToolUse hook
# (.claude/settings.json) after a file is edited, and usable standalone / in CI.
#
# Behavior:
#   - When run as a hook, reads the tool payload (JSON) on stdin and only acts if a
#     `.rs` file was edited (so editing docs never triggers a compile).
#   - Enforces the file-size cap (RUST_CONVENTIONS.md §10) — runs with or without cargo.
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

# Ensure the Rust toolchain is on PATH even in a non-login / non-interactive shell
# (Claude Code hooks don't source your interactive profile). rustup installs to
# ~/.cargo/bin and writes this env file.
if [ -f "$HOME/.cargo/env" ]; then
    # shellcheck disable=SC1091
    . "$HOME/.cargo/env"
fi

# Nothing to check until there is Rust source in the tree.
if ! find . -path ./target -prune -o -name '*.rs' -print 2>/dev/null | grep -q .; then
    exit 0
fi

fail=0
report=""

# ── LAYER 4: file-size cap (RUST_CONVENTIONS.md §10: target 150–400, split past
# ~500). The enforced ceiling is 600 — the documented "~500" plus headroom, the
# same way §8's "~50 lines" is enforced as clippy's 60. A file over the cap fails
# the build; split it into a folder + `mod.rs`. Pre-existing violations are
# grandfathered below WITH A REASON and only warn — they must still be split
# (tracked). A NEW oversized file is rejected. Needs no cargo, so it runs even
# when the toolchain is absent.
MAX_LINES=600

# Echo a reason if $1 is a sanctioned (tracked) over-cap file; else return 1.
# The list is currently EMPTY — every file is under the cap. Add an entry only
# for a genuine, tracked exception, and remove it the moment the file is split:
#   ./path/to/big.rs) echo "why it's over + the plan (tracked)" ;;
grandfathered_size() {
    case "$1" in
    *) return 1 ;;
    esac
}

while IFS= read -r rs_file; do
    [ -n "$rs_file" ] || continue
    rs_lines="$(wc -l <"$rs_file" | tr -d ' ')"
    [ "$rs_lines" -le "$MAX_LINES" ] && continue
    if size_reason="$(grandfathered_size "$rs_file")"; then
        echo "⚠️  file-size: $rs_file is $rs_lines lines (cap $MAX_LINES) — grandfathered: $size_reason" >&2
    else
        report+=$'\n❌ file-size: '"$rs_file"' is '"$rs_lines"' lines (> '"$MAX_LINES"' cap — RUST_CONVENTIONS.md §10). Split it into a folder + mod.rs.'
        fail=1
    fi
done < <(find . -path ./target -prune -o -name '*.rs' -print 2>/dev/null)

# The Rust toolchain must be available to run fmt/clippy. If `cargo` is not on
# PATH (e.g. this project builds via Docker like Oxy, or the toolchain isn't
# installed on the host), skip those two — but still honor the file-size result.
# To make fmt/clippy actually bite locally: install rustup + clippy on the host,
# OR replace the two `cargo` invocations below with a Docker wrapper
# (e.g. `docker compose run --rm dev bash -c "cargo ..."`). CI always has cargo.
if ! command -v cargo >/dev/null 2>&1; then
    echo "⚠️  axiom/check.sh: cargo not found — skipping fmt/clippy enforcement on this host. See ENFORCEMENT.md." >&2
    if [ "$fail" -ne 0 ]; then
        printf '%s\n' "$report" >&2
        exit 2
    fi
    exit 0
fi

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
