#!/bin/bash
# Git pre-push hook — compilation + test gate
# Runs before code reaches the remote. Fast enough to not block workflow,
# thorough enough to catch real problems.
set -e
set -o pipefail  # a failing command in a pipeline must not read as success (card aad30dee)

START_TIME=$(date +%s)
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# Script lives at <repo>/tools/scripts/ (substrate-first layout) — resolve
# the repo root from there, then anchor src/ and the Rust workspace off it.
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
SRC_DIR="$REPO_ROOT/src"
RUST_DIR="$REPO_ROOT/core/continuum-core"

require_node_deps() {
    if [ -x "$SRC_DIR/node_modules/.bin/tsx" ] \
        && [ -x "$SRC_DIR/node_modules/.bin/eslint" ] \
        && [ -d "$SRC_DIR/node_modules/typescript" ]; then
        return 0
    fi

    echo "❌ Node dependencies are not installed in this worktree."
    echo "   Expected: $SRC_DIR/node_modules with tsx, eslint, and typescript."
    echo "   Run:"
    echo "     cd $SRC_DIR && npm install"
    echo "   Then retry the push."
    echo ""
    echo "   This is a worktree setup failure, not a TypeScript/Rust failure."
    exit 1
}

changed_files_for_push() {
    local input="${PREPUSH_STDIN:-}"
    if [ -z "$input" ]; then
        input="$(cat 2>/dev/null || true)"
    fi

    local zero_sha="0000000000000000000000000000000000000000"
    if [ -n "$input" ]; then
        while IFS=' ' read -r local_ref local_sha remote_ref remote_sha; do
            [ -z "$local_sha" ] && continue
            [ "$local_sha" = "$zero_sha" ] && continue
            local range base
            if [ "$remote_sha" = "$zero_sha" ]; then
                base="$(git merge-base "$local_sha" origin/canary 2>/dev/null \
                    || git merge-base "$local_sha" origin/main 2>/dev/null \
                    || echo "$local_sha")"
                range="$base..$local_sha"
            else
                range="$remote_sha..$local_sha"
            fi
            git diff --name-only "$range" 2>/dev/null || true
        done <<< "$input"
    else
        git diff --name-only HEAD 2>/dev/null || true
        git diff --cached --name-only 2>/dev/null || true
    fi
}

echo "🚀 PRE-PUSH: Compilation + test gate"
echo "====================================="

FAILED=0
CHANGED_FILES="$(changed_files_for_push | sort -u)"
RUST_RELEVANT=0
if echo "$CHANGED_FILES" | grep -qE "^(core/|docker/|protocol/typescript/|Cargo\.(toml|lock)$|core/.*/Cargo\.(toml|lock)$)"; then
    RUST_RELEVANT=1
fi

# Phase 1: TypeScript compilation (<15s)
echo ""
echo "📋 Phase 1: TypeScript compilation"
echo "-----------------------------------"
TS_START=$(date +%s)
require_node_deps
if cd "$SRC_DIR" && npm run build:ts > /dev/null 2>&1; then
    echo "✅ TypeScript: clean ($(( $(date +%s) - TS_START ))s)"
else
    echo "❌ TypeScript compilation FAILED"
    echo "   Run: cd src && npm run build:ts"
    FAILED=1
fi

# Phase 1b: ESLint — baseline-tolerant.
#
# Rationale: the repo has thousands of pre-existing ESLint violations
# accumulated over time (see eslint-baseline.txt for the count). Strict
# `--max-warnings 0` would block every push regardless of whether the
# pusher introduced anything new. We still want the gate — just one
# that catches REGRESSIONS, not historical state.
#
# How this works:
#   1. Run ESLint, count errors against the explicit glob (`.` is
#      "all ignored" in ESLint 9 with the current eslint.config.js).
#   2. Read eslint-baseline.txt — the recorded "acceptable" count.
#   3. Pass if current <= baseline. Fail if current > baseline (means
#      this push added new violations).
#   4. Suggest updating the baseline if current dropped substantially
#      (cleanup is welcome, but the baseline should track real state).
#
# Update baseline after a real cleanup pass:
#   bash scripts/ratchets/check-eslint-baseline.sh --update-baseline
echo ""
echo "📋 Phase 1b: ESLint (baseline-tolerant)"
echo "----------------------------------------"
LINT_START=$(date +%s)
BASELINE_FILE="$SRC_DIR/eslint-baseline.txt"
ESLINT_RATCHET="$REPO_ROOT/scripts/ratchets/check-eslint-baseline.sh"
if [ ! -f "$BASELINE_FILE" ]; then
    echo "⚠️  eslint-baseline.txt not present at $BASELINE_FILE — skipping ESLint gate."
    echo "   Generate it once with: bash scripts/ratchets/check-eslint-baseline.sh --update-baseline"
elif [ -x "$ESLINT_RATCHET" ]; then
    if "$ESLINT_RATCHET"; then
        LINT_DUR=$(( $(date +%s) - LINT_START ))
        echo "✅ ESLint ratchet passed (${LINT_DUR}s)"
    else
        FAILED=1
    fi
else
    BASELINE=$(cat "$BASELINE_FILE" | tr -d '[:space:]')
    # Card aad30dee: COUNT the errors, but only after establishing that eslint
    # actually RAN. The previous form piped eslint's merged stdout+stderr into
    # `grep -cE "error\s+" || true` and used the count directly — so a linter
    # that CRASHED (missing module, bad config, wrong cwd) emitted diagnostics
    # the regex does not match ("Error: Cannot find module" has a capital E and
    # no trailing whitespace class), yielding 0. Zero is `-le` any baseline, so
    # a linter that never ran printed a green checkmark and let the push
    # through. `|| true` guaranteed the pipeline could not fail on its own, and
    # `set -e` cannot see through a pipe — the script LOOKED defended.
    #
    # eslint's exit codes: 0 = clean, 1 = lint errors found, 2 = eslint itself
    # failed. Only 0 and 1 mean "the linter ran and has an opinion"; anything
    # else is a broken checker and must be loud, never a silent zero.
    # `|| LINT_RC=$?` is load-bearing, not defensive noise: `set -e` (line 5)
    # aborts on a failing command substitution, and eslint exits 1 for the
    # ORDINARY case of "found lint errors". Without this the script would die
    # on the exact path it exists to report. The old `|| true` was doing this
    # job too — it just discarded the code we now need.
    LINT_RC=0
    LINT_RAW=$(cd "$SRC_DIR" && npx eslint './**/*.ts' --max-warnings 0 --quiet 2>&1) || LINT_RC=$?
    if [ "$LINT_RC" -gt 1 ]; then
        echo "❌ ESLint FAILED TO RUN (exit $LINT_RC) — this is a broken checker, not a clean tree."
        echo "   Refusing to report a lint result. Its output:"
        echo "$LINT_RAW" | tail -20 | sed 's/^/      /'
        FAILED=1
        CURRENT=""
    else
        CURRENT=$(printf '%s\n' "$LINT_RAW" | grep -cE "error\s+" || true)
    fi
fi
if [ -n "${CURRENT:-}" ]; then
    LINT_DUR=$(( $(date +%s) - LINT_START ))
    if [ "$CURRENT" -le "$BASELINE" ]; then
        if [ "$CURRENT" -lt "$BASELINE" ]; then
            DROPPED=$(( BASELINE - CURRENT ))
            echo "✅ ESLint: $CURRENT errors (baseline $BASELINE, dropped $DROPPED — update eslint-baseline.txt to lock the win) (${LINT_DUR}s)"
        else
            echo "✅ ESLint: $CURRENT errors at baseline ($BASELINE) (${LINT_DUR}s)"
        fi
    else
        DELTA=$(( CURRENT - BASELINE ))
        echo "❌ ESLint: $CURRENT errors — baseline is $BASELINE, this push added $DELTA new violation(s)."
        echo "   Run to see what's new:"
        echo "   cd src && npx eslint './**/*.ts' --max-warnings 0 --quiet"
        FAILED=1
    fi
fi

# Phase 2: Rust compilation check (<20s cached)
#
# Source cargo-features.sh to select the right GPU features per platform —
# Mac MUST pass `--features metal` after the 2026-04-23 compile_error guard
# in llama/src/lib.rs (a Mac build without --features metal produces a
# silent CPU-only binary, so the guard makes that case impossible). Without
# this source, cargo check on Mac trips the guard and pre-push fails.
# Same path npm start uses — single source of truth for which features go
# with which uname -s.
echo ""
echo "📋 Phase 2: Rust compilation"
echo "----------------------------"
RUST_START=$(date +%s)
if [ "$RUST_RELEVANT" -eq 0 ]; then
    echo "⏭️  No Rust-relevant changes in this push — skipping cargo check."
elif [ -d "$RUST_DIR" ]; then
    # shellcheck source=shared/cargo-features.sh
    source "$(dirname "$0")/shared/cargo-features.sh"
    # --message-format=short: rustc 1.94+'s default annotate-snippets diagnostic
    # renderer ICEs ("StyledBuffer::replace ... slice index starts at N but ends
    # at N-1", rust-lang/rust#157460 / #157148) while rendering some warnings in
    # this large crate. The short format bypasses that renderer entirely and the
    # hook discards diagnostic output anyway — exit code semantics are unchanged.
    if (cd "$RUST_DIR" && cargo check --message-format=short $CARGO_GPU_FEATURES 2>/dev/null); then
        echo "✅ Rust: clean ($(( $(date +%s) - RUST_START ))s) ${CARGO_GPU_FEATURES:-[cpu-only]}"
    else
        echo "❌ Rust compilation FAILED"
        echo "   Run: cd core/continuum-core && cargo check $CARGO_GPU_FEATURES"
        FAILED=1
    fi
else
    echo "⚠️  Rust directory not found (skipping)"
fi

# Phase 3: Rust tests (<30s cached)
# Use cargo's exit code as the canonical pass/fail signal — the
# previous `tail -1 | grep "test result: ok"` failed because cargo
# emits a trailing newline, so tail -1 saw an empty line and grep
# always returned no match. Exit code is the reliable test gate.
#
# Same --features rule as Phase 2 — Mac without metal trips the
# llama-crate compile_error guard.
echo ""
echo "📋 Phase 3: Rust tests"
echo "----------------------"
TEST_START=$(date +%s)
if [ "$RUST_RELEVANT" -eq 0 ]; then
    echo "⏭️  No Rust-relevant changes in this push — skipping cargo test."
elif [ -d "$RUST_DIR" ]; then
    # --message-format=short for the same annotate-snippets ICE reason as Phase 2.
    if (cd "$RUST_DIR" && cargo test --lib --message-format=short $CARGO_GPU_FEATURES > /tmp/git-prepush-cargo.log 2>&1); then
        echo "✅ Rust tests: passed ($(( $(date +%s) - TEST_START ))s) ${CARGO_GPU_FEATURES:-[cpu-only]}"
    else
        echo "❌ Rust tests FAILED"
        echo "   Run: cd core/continuum-core && cargo test --lib $CARGO_GPU_FEATURES"
        echo "   Last output:"
        tail -10 /tmp/git-prepush-cargo.log | sed 's/^/      /'
        FAILED=1
    fi
else
    echo "⚠️  Rust directory not found (skipping)"
fi

# Phase 4: Native-arch Docker images (conditional)
# Fires only when the push touches Rust or Docker files. TS/docs/widget-
# only pushes skip — they don't affect the continuum-core/vulkan/cuda
# image binaries, so there's no point paying the ~20 min build cost.
#
# Background: CI's multi-arch QEMU builds (docker-images.yml) hit 5-6hr
# timeouts on PR #950 because linux/arm64 emulation on linux/amd64 GHA
# runners is pathologically slow. New strategy: each dev machine pushes
# its NATIVE arch, CI verifies coverage. See docs/architecture/
# PERSONA-AS-RUST-LIBRARY-PLAN.md and scripts/push-current-arch.sh.
echo ""
echo "📋 Phase 4: Native-arch Docker images (if Rust/docker changed)"
echo "---------------------------------------------------------------"

DOCKER_PUSH_START=$(date +%s)
DOCKER_RELEVANT="$RUST_RELEVANT"
DOCKER_PUSH_MODE="${CONTINUUM_PREPUSH_DOCKER:-manual}"

if [ "$DOCKER_RELEVANT" -eq 0 ]; then
    echo "⏭️  No Rust/docker changes in this push — skipping native-arch build."
elif [ "$DOCKER_PUSH_MODE" != "1" ] && [ "$DOCKER_PUSH_MODE" != "true" ]; then
    echo "⏭️  Native-arch Docker publish skipped for pre-push."
    echo "   Canary iteration is gated by local TS/Rust proof above."
    echo "   Run explicitly for canary→main promotion:"
    echo "     CONTINUUM_PREPUSH_DOCKER=1 scripts/git-prepush.sh"
    echo "   Or run:"
    echo "     scripts/push-current-arch.sh"
elif [ ! -x "$REPO_ROOT/scripts/push-current-arch.sh" ]; then
    echo "⚠️  scripts/push-current-arch.sh not found or not executable — skipping."
    echo "   CI will still gate via verify-architectures, but this machine's native"
    echo "   arch won't be pushed. Investigate the missing script."
else
    echo "→ Rust/docker changes detected. Building + pushing native-arch slices."
    echo "  This takes ~20 min per image (native, not QEMU)."
    echo "  If this fails, fix Docker/auth/worktree state or push images manually with scripts/push-current-arch.sh."
    echo ""
    if "$REPO_ROOT/scripts/push-current-arch.sh"; then
        echo "✅ Native-arch Docker push: done ($(( $(date +%s) - DOCKER_PUSH_START ))s)"
    else
        # Don't block the git push on docker push failure — verify-architectures
        # in CI gates the merge, so the user sees the miss at PR time. Better
        # to let the commit propagate with a loud warning than block on a
        # transient registry auth issue or Docker daemon hiccup.
        echo "⚠️  Native-arch Docker push FAILED — continuing with git push."
        echo "   CI's verify-architectures will block merge until resolved."
        echo "   Re-run manually: scripts/push-current-arch.sh"
    fi
fi

# Result
echo ""
echo "====================================="
TOTAL_TIME=$(( $(date +%s) - START_TIME ))

if [ $FAILED -ne 0 ]; then
    echo "❌ PRE-PUSH FAILED (${TOTAL_TIME}s)"
    echo "   Fix the errors above, then push again."
    echo "   Do not bypass this with --no-verify; fix the worktree, dependencies, submodules, or hook."
    exit 1
fi

echo "✅ PRE-PUSH PASSED (${TOTAL_TIME}s)"
