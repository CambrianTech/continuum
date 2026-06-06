#!/usr/bin/env bash
# airc-shared-target-opt-in.sh
#
# Per build-time doctrine slice 3 (src/workers/.cargo/config.toml):
# collapse N per-worktree target/ dirs into ONE shared
# `~/.airc/cargo-target`. Run once on this machine; the policy then
# travels with every airc worktree this peer claims.
#
# WHY: each airc worktree carries its own 5-10 GB target/ dir; with
# 10 active worktrees the dev-artifact sprawl can saturate the disk
# (observed: 2026-06-05 disk-full incident blocked a mid-build PR
# fix). Shared target collapses that to ONE tree.
#
# TRADE-OFFS (cargo target lock is real):
#   - Sequential builds: concurrent `cargo build` calls across
#     worktrees serialize on the target lock. Invisible for the
#     typical one-PR-at-a-time flow.
#   - Branch-switch rebuilds when Cargo.lock differs.
#   - Composes with slice 1 (sccache) + slice 2 (linker selection).
#
# WHAT THIS SCRIPT DOES:
#   1. Creates ~/.airc/cargo-target/ if missing.
#   2. Writes a one-line CARGO_TARGET_DIR export into the user's
#      shell init (zshrc / bashrc — auto-detected).
#   3. Reports current usage (before/after).
#
# WHAT THIS SCRIPT DOES NOT DO:
#   - Edit src/workers/.cargo/config.toml (that's checked-in opt-in
#     guidance; not a per-machine action).
#   - Delete existing per-worktree target/ dirs (operator decision;
#     see the optional cleanup step below).
#   - Sync sccache settings (slice 1 has its own opt-in).
#
# USAGE:
#   ./scripts/airc-shared-target-opt-in.sh           # interactive
#   ./scripts/airc-shared-target-opt-in.sh --no-prompt   # CI/scripts

set -euo pipefail

SHARED_TARGET="${SHARED_TARGET_OVERRIDE:-$HOME/.airc/cargo-target}"
NO_PROMPT=false
for arg in "$@"; do
    case "$arg" in
        --no-prompt) NO_PROMPT=true ;;
        --help|-h)
            sed -n '1,/^set -euo/p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *) echo "unknown arg: $arg (use --help)" >&2; exit 2 ;;
    esac
done

confirm() {
    if "$NO_PROMPT"; then return 0; fi
    read -r -p "$1 [y/N] " ans
    [[ "$ans" =~ ^[Yy]$ ]]
}

echo "airc shared CARGO_TARGET_DIR opt-in"
echo "==================================="
echo "shared target dir : $SHARED_TARGET"
echo

# Step 1 — create shared target dir
if [[ -d "$SHARED_TARGET" ]]; then
    echo "[1/3] shared target already exists at $SHARED_TARGET"
else
    echo "[1/3] creating $SHARED_TARGET"
    mkdir -p "$SHARED_TARGET"
fi
echo

# Step 2 — write shell init export
SHELL_RC=""
case "${SHELL##*/}" in
    zsh)  SHELL_RC="$HOME/.zshrc" ;;
    bash) SHELL_RC="$HOME/.bashrc" ;;
    *)    SHELL_RC="$HOME/.profile" ;;
esac

EXPORT_LINE="export CARGO_TARGET_DIR=\"$SHARED_TARGET\"  # airc shared target (slice 3)"

if grep -Fq "airc shared target (slice 3)" "$SHELL_RC" 2>/dev/null; then
    echo "[2/3] $SHELL_RC already has the export line — skipping"
else
    if confirm "[2/3] append CARGO_TARGET_DIR export to $SHELL_RC?"; then
        printf '\n%s\n' "$EXPORT_LINE" >> "$SHELL_RC"
        echo "      added. source it now or open a new shell:"
        echo "      source $SHELL_RC"
    else
        echo "[2/3] skipped — to apply manually, append this line to your shell init:"
        echo "      $EXPORT_LINE"
    fi
fi
echo

# Step 3 — report current per-worktree target sprawl
WORKTREE_ROOT="$HOME/.airc/worktrees"
if [[ -d "$WORKTREE_ROOT" ]]; then
    echo "[3/3] current per-worktree target/ sprawl (reclaimable on next clean build):"
    found_any=false
    for wt_target in "$WORKTREE_ROOT"/*/src/workers/target; do
        if [[ -d "$wt_target" ]]; then
            found_any=true
            size=$(du -sh "$wt_target" 2>/dev/null | cut -f1)
            printf "      %s  %s\n" "$size" "$wt_target"
        fi
    done
    if ! "$found_any"; then
        echo "      none — all worktrees are already without their own target/ dir."
    else
        echo
        echo "      to reclaim now: rm -rf those paths. Subsequent builds will"
        echo "      compile into $SHARED_TARGET instead."
    fi
else
    echo "[3/3] no airc worktrees at $WORKTREE_ROOT — nothing to report."
fi

echo
echo "done. verify after opening a new shell:"
echo "  echo \$CARGO_TARGET_DIR    # should print $SHARED_TARGET"
echo "  cd \$(any airc worktree)/src && cargo build -v 2>&1 | head -5"
