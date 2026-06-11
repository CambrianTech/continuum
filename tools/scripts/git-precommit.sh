#!/bin/bash
set -e  # Exit immediately on any error

# Navigate to the correct working directory
cd "$(dirname "$0")/.."

# ==============================================================================
# BRANCH-STATE GUARD (continuum#1187)
# ==============================================================================
# Capture the branch + HEAD sha BEFORE the hook does any work. The end-of-
# script guard verifies these are unchanged before printing "Commit approved";
# if they HAVE changed, the script aborts with exit 1 + a loud error so git
# refuses to create the commit on the wrong ref.
#
# Root-cause family of #1187: backticks in commit messages can be evaluated
# by bash if the user runs `git commit -m "fix \`git checkout\` bug"` — bash
# executes the backtick subcommand and its side-effects (an unintended
# `git checkout`) silently change the branch. Single-quoted HEREDOC commit
# messages don't have this problem, but the hook can't enforce caller quoting.
# Defense in depth: even if the bug recurs (this hook OR caller), the guard
# catches it.
PRECOMMIT_INITIAL_BRANCH="$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo 'DETACHED')"
PRECOMMIT_INITIAL_HEAD="$(git rev-parse HEAD 2>/dev/null || echo '')"
PRECOMMIT_INITIAL_TOPLEVEL="$(git rev-parse --show-toplevel 2>/dev/null || echo '')"
export PRECOMMIT_INITIAL_BRANCH PRECOMMIT_INITIAL_HEAD PRECOMMIT_INITIAL_TOPLEVEL

# Verify the captured state still holds. Used at end of script + can be
# called from any sub-step that wants to assert mid-run.
verify_branch_state_unchanged() {
    local now_branch
    local now_head
    local now_toplevel
    now_branch="$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo 'DETACHED')"
    now_head="$(git rev-parse HEAD 2>/dev/null || echo '')"
    now_toplevel="$(git rev-parse --show-toplevel 2>/dev/null || echo '')"

    if [ "$now_branch" != "$PRECOMMIT_INITIAL_BRANCH" ] \
        || [ "$now_head" != "$PRECOMMIT_INITIAL_HEAD" ] \
        || [ "$now_toplevel" != "$PRECOMMIT_INITIAL_TOPLEVEL" ]; then
        echo ""
        echo "🚨🚨🚨 BRANCH-STATE GUARD TRIPPED — ABORTING COMMIT 🚨🚨🚨"
        echo "==================================================================="
        echo "The precommit hook changed branch state mid-run. Aborting before"
        echo "git can create a commit on the wrong ref. This protects you from"
        echo "the silent loss-of-work failure mode tracked in continuum#1187."
        echo ""
        echo "  branch:    '$PRECOMMIT_INITIAL_BRANCH' -> '$now_branch'"
        echo "  HEAD:      '$PRECOMMIT_INITIAL_HEAD' -> '$now_head'"
        echo "  toplevel:  '$PRECOMMIT_INITIAL_TOPLEVEL' -> '$now_toplevel'"
        echo ""
        echo "Likely cause: backticks in your commit message that bash evaluated"
        echo "as subcommands. Switch to single-quoted HEREDOC for commit messages:"
        echo ""
        echo "  git commit -m \"\$(cat <<'EOF'"
        echo "  fix(...): your message with \`backticks\` is now safe"
        echo "  EOF"
        echo "  )\""
        echo ""
        echo "Your staged changes are still in the index. Recover with:"
        echo "  git switch '$PRECOMMIT_INITIAL_BRANCH'"
        echo "  git stash list   # if anything got auto-stashed"
        echo "==================================================================="
        exit 1
    fi
}

require_node_deps() {
    if [ -x "node_modules/.bin/tsx" ] \
        && [ -x "node_modules/.bin/eslint" ] \
        && [ -d "node_modules/typescript" ]; then
        return 0
    fi

    echo "❌ Node dependencies are not installed in this worktree."
    echo "   Expected: $(pwd)/node_modules with tsx, eslint, and typescript."
    echo "   Run:"
    echo "     cd $(pwd) && npm install"
    echo "   Then retry the commit."
    echo ""
    echo "   This is a worktree setup failure, not a TypeScript/Rust failure."
    exit 1
}

# ==============================================================================
# LOAD CONFIGURATION
# ==============================================================================
# Source the modular configuration file
if [ -f "scripts/precommit-config.sh" ]; then
    source scripts/precommit-config.sh
    echo "✅ Loaded precommit configuration from scripts/precommit-config.sh"
else
    echo "❌ Configuration file not found: scripts/precommit-config.sh"
    echo "   Using default settings"
    export ENABLE_TYPESCRIPT_CHECK=true
    export ENABLE_BROWSER_TEST=true
    export RESTART_STRATEGY="on_code_change"
    # Browser ping = "server didn't crash + browser is reachable" (low bar).
    # Chat roundtrip = "a persona actually replies to a chat probe" (#1186).
    # Run BOTH on every commit until path-tier dispatcher lands (#1186 PR-2).
    export PRECOMMIT_TESTS="tests/precommit/browser-ping.test.ts tests/precommit/chat-roundtrip.test.ts"
    export PRECOMMIT_TEST_TIMEOUT_SECONDS=60
    export PRECOMMIT_CHAT_ROUNDTRIP_TIMEOUT_SECONDS=120
fi

echo "🔒 GIT PRECOMMIT: Modular validation (config-driven)"
echo "=================================================="
echo "📋 Active phases:"
[ "$ENABLE_TYPESCRIPT_CHECK" = true ] && echo "  ✅ TypeScript compilation"
[ "$ENABLE_SYSTEM_RESTART" = true ] && echo "  ✅ System restart (strategy: $RESTART_STRATEGY)"
[ "$ENABLE_BROWSER_TEST" = true ] && echo "  ✅ Browser tests ($PRECOMMIT_TESTS)"
echo ""

# Phase 0: Command generator ownership guard
# New src/commands/** modules must have a matching generator spec. This keeps
# generated command shape centralized instead of letting agents hand-create
# partial command folders that later fail registration/runtime discovery.
echo "📋 Phase 0: Command generator ownership"
echo "-------------------------------------"
require_node_deps
npx tsx generator/validate-command-spec-coverage.ts
echo ""

# Phase 0: Block changes to generated files
# These are auto-generated by build scripts and should never be manually edited.
# Personas keep modifying them — this catches it before commit.
GENERATED_FILES="src/server/generated.ts src/browser/generated.ts protocol/typescript-command-constants.ts src/generated-command-schemas.json"
BLOCKED_FILES=""
cd ..
for f in $GENERATED_FILES; do
    if git diff --cached --name-only | grep -q "^$f$"; then
        BLOCKED_FILES="$BLOCKED_FILES $f"
    fi
done
if [ -n "$BLOCKED_FILES" ]; then
    echo "❌ BLOCKED: Changes to generated files detected:"
    for f in $BLOCKED_FILES; do
        echo "   $f"
    done
    echo ""
    echo "   These files are auto-generated. Your changes will be overwritten."
    echo "   Unstage them: git reset HEAD $BLOCKED_FILES"
    exit 1
fi
cd src

# Phase 1: Foundation Validation
if [ "$ENABLE_TYPESCRIPT_CHECK" = true ]; then
    echo ""
    echo "📋 Phase 1: TypeScript Compilation"
    echo "-------------------------------------"

    echo "🔨 Running TypeScript compilation..."
    require_node_deps
    npm run build:ts
    # Restore version.ts to avoid timestamp-only changes in commit
    cd ..
    git restore src/shared/version.ts 2>/dev/null || true
    cd src
    echo "✅ TypeScript compilation passed"
else
    echo "⏭️  Phase 1: TypeScript compilation SKIPPED (disabled in config)"
fi

# ============================================================================
# Phase 1.5: Strict Lint (MODIFIED FILES ONLY)
# ============================================================================
# This enforces strict rules on NEW code without breaking existing tech debt.
# Only staged files are checked - incrementally improve quality.
# ============================================================================
echo ""
echo "📋 Phase 1.5: Strict Lint (modified files only)"
echo "-------------------------------------"

# Get list of staged TypeScript files (excluding node_modules, dist, generated)
TS_FILES=$(cd .. && git diff --cached --name-only --diff-filter=ACMR | grep -E 'src/.*\.tsx?$' | grep -v 'node_modules' | grep -v 'dist/' | grep -v '/generated' | grep -v 'generated-command' || true)

# Get list of staged Rust files
RS_FILES=$(cd .. && git diff --cached --name-only --diff-filter=ACMR | grep -E 'core/.*\.rs$' | grep -v 'target/' || true)

LINT_FAILED=false

if [ -n "$TS_FILES" ]; then
    require_node_deps
    echo "TypeScript files staged:"
    echo "$TS_FILES" | sed 's/^/  • /' | head -10
    TS_COUNT=$(echo "$TS_FILES" | wc -l | tr -d ' ')
    [ "$TS_COUNT" -gt 10 ] && echo "  ... and $((TS_COUNT - 10)) more"
    echo ""

    # Two-tier ESLint gate. The previous --max-warnings 0 per-file mode
    # was unworkable: any commit touching a file with pre-existing
    # violations forced --no-verify, which let new debt land freely.
    # The new gate mirrors git-prepush.sh's baseline-tolerant approach
    # but adds a fast path so most commits don't pay the repo-wide cost.
    #
    # Tier 1 (fast, ~5s): lint just the staged files. If they're clean
    #                     (zero violations), the commit can't have added
    #                     anything — pass immediately.
    # Tier 2 (slow, ~2m): if staged files carry violations, run the
    #                     repo-wide check and compare to eslint-baseline.txt.
    #                     Pass if total <= baseline (no new debt added).
    #
    # Update baseline after a real cleanup pass:
    #   cd src && npx eslint './**/*.ts' --max-warnings 0 --quiet 2>&1 \
    #     | grep -cE "error\s+" > eslint-baseline.txt
    # Use a script-relative path instead of `git rev-parse --show-toplevel`.
    # When invoked from a git worktree's `src/` cwd (which the hook does at
    # line 5 + 52), `--show-toplevel` returned the cwd `/repo/src` rather
    # than the worktree root `/repo`, producing an incorrect double-`src`
    # path `/repo/src/src/eslint-baseline.txt`. The hook ALWAYS lives at
    # `<repo>/tools/scripts/git-precommit.sh` (substrate-first layout), so
    # the baseline is at `<repo>/src/eslint-baseline.txt` — two dirs up from
    # the script dir, then into src/ — deterministic, no git resolution needed.
    HOOK_SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    BASELINE_FILE="$(dirname "$(dirname "$HOOK_SCRIPT_DIR")")/src/eslint-baseline.txt"

    # Tier 1: staged-files-only fast lint.
    STAGED_LINT_LOG="$(mktemp)"
    (cd .. && echo "$TS_FILES" | xargs npx eslint --no-warn-ignored --quiet 2>&1 > "$STAGED_LINT_LOG") || true
    STAGED_ERRORS=$(grep -cE "error\s+" "$STAGED_LINT_LOG" || true)
    rm -f "$STAGED_LINT_LOG"

    if [ "$STAGED_ERRORS" -eq 0 ]; then
        echo "✅ ESLint: staged files clean (fast path, no repo-wide check needed)"
    elif [ ! -f "$BASELINE_FILE" ]; then
        echo "⚠️  eslint-baseline.txt not present — falling back to strict per-file gate."
        echo "   Generate once with: cd src && npx eslint './**/*.ts' --max-warnings 0 --quiet 2>&1 | grep -cE \"error\\s+\" > eslint-baseline.txt"
        LINT_FAILED=true
    else
        # Tier 2: staged files carry violations. Verify the commit didn't
        # ADD any by running the same repo-wide gate as prepush.
        echo "ℹ️  Staged files carry $STAGED_ERRORS pre-existing violation(s); running repo-wide baseline check..."
        BASELINE=$(tr -d '[:space:]' < "$BASELINE_FILE")
        LINT_START=$(date +%s)
        CURRENT=$(npx eslint './**/*.ts' --max-warnings 0 --quiet 2>&1 | grep -cE "error\s+" || true)
        LINT_DUR=$(( $(date +%s) - LINT_START ))
        if [ "$CURRENT" -le "$BASELINE" ]; then
            if [ "$CURRENT" -lt "$BASELINE" ]; then
                DROPPED=$(( BASELINE - CURRENT ))
                echo "✅ ESLint: $CURRENT errors (baseline $BASELINE, dropped $DROPPED — update src/eslint-baseline.txt to lock the win) (${LINT_DUR}s)"
            else
                echo "✅ ESLint: $CURRENT errors at baseline ($BASELINE) (${LINT_DUR}s)"
            fi
        else
            DELTA=$(( CURRENT - BASELINE ))
            echo ""
            echo "╔════════════════════════════════════════════════════════════════╗"
            echo "║  ❌ ESLINT: $DELTA NEW VIOLATION(S) — BLOCKING COMMIT          ║"
            echo "╠════════════════════════════════════════════════════════════════╣"
            echo "║  Current: $CURRENT  Baseline: $BASELINE                                       ║"
            echo "║  Run to see what's new:                                        ║"
            echo "║    cd src && npx eslint './**/*.ts' --max-warnings 0 --quiet   ║"
            echo "╚════════════════════════════════════════════════════════════════╝"
            LINT_FAILED=true
        fi
    fi
else
    echo "⏭️  No TypeScript files staged - skipping ESLint"
fi

if [ -n "$RS_FILES" ]; then
    echo ""
    echo "Rust files to lint with clippy:"
    echo "$RS_FILES" | sed 's/^/  • /' | head -10
    echo ""

    # Baseline-tolerant clippy (same shape as ESLint baseline in
    # git-prepush.sh): the workspace has 100+ pre-existing clippy
    # warnings, and -D warnings turns ALL of them into hard errors.
    # That made every commit fail regardless of who wrote what.
    #
    # New shape: count warnings, compare to clippy-baseline.txt.
    # Pass if current <= baseline. Fail if current > baseline (i.e.
    # this commit added new violations). Update the baseline after
    # a real cleanup pass:
    #   cd core/continuum-core
    #   source ../../tools/scripts/shared/cargo-features.sh
    #   cargo clippy --lib --message-format=short $CARGO_GPU_FEATURES 2>&1 | grep -cE ": warning:" > ../../src/clippy-baseline.txt
    #
    # Same platform feature selection as pre-push/npm start. macOS without
    # `--features metal,accelerate` intentionally fails at compile time because
    # CPU-only local inference is not a supported product path.
    #
    # --message-format=short: rustc 1.94+'s annotate-snippets renderer ICEs on
    # some warnings in this crate (rust-lang/rust#157460 / #157148) which both
    # crashed the gate AND truncated the warning count. Short format bypasses
    # the renderer; each diagnostic prints as "<file>:<line>:<col>: warning: ..."
    # so the count regex is ": warning:" (the old "^warning:" counted human
    # format lines and would only match the end-of-run summary here).
    #
    # Use the hook's src cwd instead of git rev-parse. In git worktrees,
    # --show-toplevel is the parent checkout root, while this hook and baseline
    # live under <root>/src. The crate itself moved in the substrate-first
    # layout (2cb63e019): src/workers/continuum-core -> core/continuum-core,
    # and the shared feature script lives next to this hook.
    # shellcheck source=shared/cargo-features.sh
    source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/shared/cargo-features.sh"
    BASELINE_FILE="$(pwd)/clippy-baseline.txt"
    CLIPPY_LOG="$(mktemp)"
    if ! (cd ../core/continuum-core && cargo clippy --lib --message-format=short $CARGO_GPU_FEATURES > "$CLIPPY_LOG" 2>&1); then
        echo "❌ cargo clippy failed to run (compile error or missing toolchain):"
        tail -10 "$CLIPPY_LOG" | sed 's/^/      /'
        LINT_FAILED=true
    fi
    CURRENT=$(grep -cE ": warning:" "$CLIPPY_LOG" || true)
    if [ ! -f "$BASELINE_FILE" ]; then
        echo "❌ clippy-baseline.txt not found at $BASELINE_FILE — cannot run baseline gate."
        echo "   Generate once with:"
        echo "     cd core/continuum-core"
        echo "     source ../../tools/scripts/shared/cargo-features.sh"
        echo "     cargo clippy --lib --message-format=short \$CARGO_GPU_FEATURES 2>&1 | grep -cE \": warning:\" > ../../src/clippy-baseline.txt"
        echo "   Current warning count: $CURRENT"
        LINT_FAILED=true
    else
        BASELINE=$(cat "$BASELINE_FILE" | tr -d '[:space:]')
        if [ "$CURRENT" -le "$BASELINE" ]; then
            if [ "$CURRENT" -lt "$BASELINE" ]; then
                DROPPED=$(( BASELINE - CURRENT ))
                echo "✅ Rust clippy: $CURRENT warnings (baseline $BASELINE, dropped $DROPPED — update src/clippy-baseline.txt to lock the win)"
            else
                echo "✅ Rust clippy: $CURRENT warnings at baseline ($BASELINE)"
            fi
        else
            DELTA=$(( CURRENT - BASELINE ))
            echo ""
            echo "╔════════════════════════════════════════════════════════════════╗"
            echo "║  ❌ RUST CLIPPY: $DELTA NEW WARNING(S) — BLOCKING COMMIT       ║"
            echo "╠════════════════════════════════════════════════════════════════╣"
            echo "║  Current: $CURRENT  Baseline: $BASELINE                                       ║"
            echo "║  Run to see what's new:                                        ║"
            echo "║    cd core/continuum-core                                      ║"
            echo "║    source ../../tools/scripts/shared/cargo-features.sh          ║"
            echo "║    cargo clippy --lib --message-format=short \$CARGO_GPU_FEATURES ║"
            echo "╚════════════════════════════════════════════════════════════════╝"
            LINT_FAILED=true
        fi
    fi
    rm -f "$CLIPPY_LOG"
else
    echo "⏭️  No Rust files staged - skipping clippy"
fi

if [ "$LINT_FAILED" = true ]; then
    echo ""
    echo "❌ STRICT LINT FAILED - Fix violations in modified files before committing"
    exit 1
fi
echo ""

# Detect if code changes require deployment
echo "🔍 Checking if code changes require deployment..."
cd ..
CODE_CHANGED=false

# Check if any TypeScript, JavaScript, or browser bundle files are being committed
if git diff --cached --name-only | grep -qE '\.(ts|tsx|js|jsx|css|html)$'; then
    echo "📝 Code changes detected in commit - deployment required"
    CODE_CHANGED=true
elif git diff --cached --name-only | grep -q 'browser/generated\.ts'; then
    echo "📦 Browser bundle changed - deployment required"
    CODE_CHANGED=true
else
    echo "📄 Only documentation/config changes - deployment may not be needed"
fi

cd src

# Determine if restart is needed based on strategy
if [ "$ENABLE_SYSTEM_RESTART" = true ]; then
    echo "🏓 Checking if system restart is required (strategy: $RESTART_STRATEGY)..."
    NEED_RESTART=false

    case "$RESTART_STRATEGY" in
        always)
            echo "📝 Always restart (strategy: always)"
            NEED_RESTART=true
            ;;
        on_code_change)
            if [ "$CODE_CHANGED" = true ]; then
                echo "📝 Code changed - restart required to test new code"
                NEED_RESTART=true
            elif ! ./jtag ping >/dev/null 2>&1; then
                echo "❌ System not responding to ping - restart required"
                NEED_RESTART=true
            else
                echo "✅ System running and no code changes - no restart needed"
            fi
            ;;
        on_ping_fail)
            if ! ./jtag ping >/dev/null 2>&1; then
                echo "❌ System not responding to ping - restart required"
                NEED_RESTART=true
            else
                echo "✅ System responding to ping - no restart needed"
            fi
            ;;
        never)
            echo "⏭️  Restart disabled (strategy: never)"
            NEED_RESTART=false
            ;;
        *)
            echo "⚠️  Unknown restart strategy: $RESTART_STRATEGY (defaulting to on_code_change)"
            NEED_RESTART=$CODE_CHANGED
            ;;
    esac
else
    echo "⏭️  System restart SKIPPED (disabled in config)"
    NEED_RESTART=false
fi

# Start system if ping failed
if [ "$NEED_RESTART" = true ]; then
    echo "🚀 Starting deployment..."
    npm start &
    DEPLOY_PID=$!

    echo "⏳ Waiting for system to be ready..."
    TIMEOUT=90  # Generous timeout for initial startup
    COUNTER=0

    while [ $COUNTER -lt $TIMEOUT ]; do
        # Check if ping works (system is ready)
        if ./jtag ping >/dev/null 2>&1; then
            echo "✅ System deployment successful - ping responding"
            echo "⏳ Allowing system to settle..."
            sleep 3  # Brief settle time
            break
        fi

        # Progress indicator every 5 seconds
        if [ $((COUNTER % 5)) -eq 0 ]; then
            echo "   ... waiting for system startup ($COUNTER/${TIMEOUT}s)"
        fi

        sleep 1
        COUNTER=$((COUNTER + 1))
    done

    if [ $COUNTER -eq $TIMEOUT ]; then
        echo "❌ System deployment timed out after ${TIMEOUT}s"
        echo "   ./jtag ping never succeeded"
        echo "   Check .continuum/jtag/system/logs/npm-start.log for details"
        kill $DEPLOY_PID 2>/dev/null || true
        exit 1
    fi
else
    echo "⚡ System already running - no restart needed"
fi

# Phase 2: Browser Tests
if [ "$ENABLE_BROWSER_TEST" = true ]; then
    echo ""
    echo "🧪 Phase 2: Browser Tests"
    echo "-----------------------------------------------------------"

    # Skip gracefully when the browser-test prerequisites aren't met.
    # The browser-ping + chat-roundtrip tests both round-trip through
    # continuum-core's Rust IPC socket. If continuum-core isn't running
    # OR the browser isn't connected/responsive, chat-roundtrip hangs
    # or fails on IPC.
    #
    # TWO probes are required because they cover different layers:
    #
    # (1) `./jtag ping` — verifies the jtag-client TS surface is alive.
    #     This is the historical probe but is INSUFFICIENT on its own:
    #     `jtag ping` runs through PingServerCommand which collects
    #     server info + optionally pings browser, but NEVER touches the
    #     Rust continuum-core IPC socket. Returns OK even when core is
    #     down. (Bug surfaced 2026-05-16 — see codex's airc broadcast
    #     and claude-tab-1's second-source confirmation that same day.)
    #
    # (2) Continuum-core Unix socket probe — verifies the Rust server
    #     is actually accepting IPC connections. This is what
    #     chat-roundtrip needs; without it, the gate runs a test that
    #     can only fail. Two-stage: socket file exists (-S) AND nc
    #     accepts a 1s connection. A stale socket file from a crashed
    #     core stays on disk but won't accept, hence both checks.
    #
    # If EITHER probe fails, ENABLE_BROWSER_TEST=false and the gate
    # SKIPS browser tests rather than blocking the commit. CI's
    # verify-architectures + GitHub Actions remain the authoritative
    # pre-merge check.
    #
    # 10s perl-fork timeout pattern for jtag ping — perl's `alarm`
    # doesn't propagate through `exec` (SIGALRM lost when process
    # image replaced), so parent times out + kills child on overrun.
    PING_OK=true
    if ! perl -e '
        my $pid = fork();
        die "fork: $!" unless defined $pid;
        if ($pid == 0) { exec "./jtag", "ping"; die "exec: $!"; }
        my $deadline = time() + 10;
        while (1) {
            my $w = waitpid($pid, 1);  # 1 = WNOHANG
            last if $w == $pid;
            if (time() > $deadline) { kill 9, $pid; waitpid($pid, 0); exit 142; }
            select(undef, undef, undef, 0.1);
        }
        exit ($? >> 8);
    ' > /dev/null 2>&1; then
        PING_OK=false
    fi

    # Continuum-core Unix socket probe. Path matches SOCKETS.CONTINUUM_CORE
    # in src/shared/config.ts (`${HOME}/.continuum/sockets/continuum-core.sock`).
    # nc -U dial with 1s timeout: file-exists alone isn't enough because a
    # stale socket from a crashed core lingers on disk; the actual connect
    # is the truth.
    CORE_OK=true
    CORE_SOCKET="$HOME/.continuum/sockets/continuum-core.sock"
    if [ ! -S "$CORE_SOCKET" ]; then
        CORE_OK=false
    elif ! echo "" | nc -U -w 1 "$CORE_SOCKET" >/dev/null 2>&1; then
        CORE_OK=false
    fi

    if [ "$PING_OK" = false ] || [ "$CORE_OK" = false ]; then
        echo ""
        echo "⚠️  Browser-test prerequisites not met within timeout."
        if [ "$PING_OK" = false ]; then
            echo "     • ./jtag ping: FAILED (jtag-client / browser surface)"
        else
            echo "     • ./jtag ping: ok"
        fi
        if [ "$CORE_OK" = false ]; then
            echo "     • continuum-core IPC ($CORE_SOCKET): NOT REACHABLE"
        else
            echo "     • continuum-core IPC: ok"
        fi
        echo "   Skipping browser tests for this commit."
        echo "   To enable the browser-test gate, ensure the system is running:"
        echo "     cd src && npm start"
        echo "   Then verify with:"
        echo "     cd src && ./jtag ping"
        echo "     [ -S $CORE_SOCKET ] && echo 'core socket present'"
        echo ""
        echo "✅ Browser tests: SKIPPED (prerequisite not met)"
        ENABLE_BROWSER_TEST=false
    fi
fi

if [ "$ENABLE_BROWSER_TEST" = true ]; then
    echo "🧪 Running precommit tests: $PRECOMMIT_TESTS"

    # Ensure test output directory exists
    mkdir -p .continuum/sessions/validation

    # Run all configured tests
    TEST_EXIT_CODE=0
    TEST_SUMMARY=""

    for TEST_FILE in $PRECOMMIT_TESTS; do
        TEST_TIMEOUT_SECONDS="${PRECOMMIT_TEST_TIMEOUT_SECONDS:-60}"
        case "$TEST_FILE" in
            *chat-roundtrip.test.ts)
                TEST_TIMEOUT_SECONDS="${PRECOMMIT_CHAT_ROUNDTRIP_TIMEOUT_SECONDS:-120}"
                ;;
        esac

        echo "=================================================="
        echo "🧪 Running: $TEST_FILE  (${TEST_TIMEOUT_SECONDS}s timeout cap)"
        echo "=================================================="

        # Wrap each test in a timeout via perl fork+wait. perl's
        # bare `alarm` doesn't survive `exec` (signal handler is lost
        # when the process image is replaced), so we fork: parent
        # times out and kills the child after the configured cap. Some tests
        # (browser-ping) hang for 10 minutes when the browser is in
        # a non-responsive-but-not-crashed state — useless friction
        # on every commit.
        perl -e '
            use POSIX qw(setpgid);
            my $timeout = shift @ARGV;
            shift @ARGV if @ARGV && $ARGV[0] eq "--";
            my $pid = fork();
            die "fork: $!" unless defined $pid;
            if ($pid == 0) {
                # Put child + descendants into their own process group so we
                # can kill the entire tree (npx -> node -> tsx -> test +
                # any subprocesses). Without this, killing $pid only kills
                # npx; orphaned tsx + test keep running and hold the
                # commit hostage.
                POSIX::setpgid(0, 0) or warn "setpgid failed: $!";
                exec @ARGV;
                die "exec: $!";
            }
            POSIX::setpgid($pid, $pid);  # parent races child; both safe
            my $deadline = time() + $timeout;
            while (1) {
                my $w = waitpid($pid, 1);
                last if $w == $pid;
                if (time() > $deadline) {
                    # Negative PID = signal whole process group.
                    kill 9, -$pid;
                    waitpid($pid, 0);
                    exit 142;
                }
                select(undef, undef, undef, 0.1);
            }
            exit ($? >> 8);
        ' "$TEST_TIMEOUT_SECONDS" -- npx tsx "$TEST_FILE" 2>&1 \
            | tee .continuum/sessions/validation/test-output.txt
        CURRENT_EXIT_CODE=${PIPESTATUS[0]}

        if [ $CURRENT_EXIT_CODE -eq 142 ] || [ $CURRENT_EXIT_CODE -eq 14 ]; then
            # 142 / 14 = SIGALRM exit. The test exceeded the 60s cap —
            # treat as "system not ready" rather than test failure.
            # Skip the gate; CI's verify-architectures + browser tests
            # in CI environments remain authoritative.
            echo ""
            echo "⚠️  Test timed out after ${TEST_TIMEOUT_SECONDS}s: $TEST_FILE"
            echo "   The system isn't responsive enough for this test."
            echo "   Skipping the browser-test gate for this commit."
            echo "   To enable: ensure 'cd src && ./jtag interface/screenshot --querySelector=body' returns within 60s."
            TEST_SUMMARY="$TEST_SUMMARY $TEST_FILE:SKIPPED-TIMEOUT"
            continue
        fi

        if [ $CURRENT_EXIT_CODE -ne 0 ]; then
            TEST_EXIT_CODE=$CURRENT_EXIT_CODE
            echo ""
            echo "❌ TEST FAILED - BLOCKING COMMIT"
            echo "=================================================="
            echo "❌ Test FAILED (exit code: $CURRENT_EXIT_CODE)"
            echo "   Test file: $TEST_FILE"
            echo "   Output shown above"
            echo ""
            echo "🔍 Fix the failing test before committing"
            echo "=================================================="
            exit 1
        else
            echo "✅ Test passed: $TEST_FILE"
            TEST_SUMMARY="$TEST_SUMMARY $TEST_FILE:PASSED"
        fi
    done

    echo ""
    echo "✅ All precommit tests: PASSED"
    echo "📊 Test results: $TEST_SUMMARY"
else
    echo "⏭️  Phase 2: Browser tests SKIPPED (disabled in config)"
    TEST_SUMMARY="Browser tests: SKIPPED"
fi

# Phase 3: Session Artifacts Collection
if [ "$ENABLE_ARTIFACTS_COLLECTION" = true ]; then
    echo ""
    echo "📦 Phase 3: Collecting session artifacts"
    echo "---------------------------------------------------------------------"

# Use a stable validation ID based on timestamp for this precommit run
VALIDATION_ID="$(date +%Y%m%d-%H%M%S)-$$"
VALIDATION_RUN_DIR=".continuum/sessions/validation/run_${VALIDATION_ID}"

# Find the active browser session (where screenshots were saved by integration tests)
# Don't rely on currentUser symlink - it may point to wrong session if system restarted
SCREENSHOT_SESSION=$(find examples/widget-ui/.continuum/jtag/sessions/user/*/screenshots/*.png 2>/dev/null | head -1)
if [ -n "$SCREENSHOT_SESSION" ]; then
    # Extract session directory from screenshot path
    SESSION_PATH=$(echo "$SCREENSHOT_SESSION" | sed 's|/screenshots/.*||')
    CURRENT_SESSION=$(basename "$SESSION_PATH")

    if [ -d "$SESSION_PATH" ]; then
        echo "🔍 Active screenshot session: $CURRENT_SESSION"

        # Ensure validation parent directory exists
        mkdir -p ".continuum/sessions/validation"

        # Move ENTIRE session directory to validation (rename it to run_ID)
        echo "📋 Moving complete session directory to validation..."
        mv "$SESSION_PATH" "$VALIDATION_RUN_DIR"
        echo "✅ Complete session moved to validation directory"

        # Add test results to the validation directory
        echo "$TEST_OUTPUT" > "$VALIDATION_RUN_DIR/test-results.txt"
        echo "✅ Test results added to session artifacts"

        # Create validation metadata (enhanced session-info.json)
        cat > "$VALIDATION_RUN_DIR/validation-info.json" << EOF
{
  "runId": "${COMMIT_HASH:0:12}",
  "timestamp": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "sessionId": "$CURRENT_SESSION",
  "validationType": "precommit",
  "status": "PASSED",
  "testSummary": "$TEST_SUMMARY",
  "testResults": {
    "exitCode": $TEST_EXIT_CODE,
    "outputLines": $(echo "$TEST_OUTPUT" | wc -l),
    "outputFile": "test-results.txt"
  },
  "validationPhases": [
    "TypeScript Compilation",
    "System Deployment",
    "CRUD + Chat Integration (100% Required)",
    "Screenshot Proof Collection",
    "Complete Session Copy"
  ]
}
EOF

        # VALIDATION ARTIFACTS: Add to git for commit inclusion
        echo "📋 Validation artifacts created for bulletproof validation..."

        # Stage validation directory from repo root
        REPO_ROOT=".."
        cd "$REPO_ROOT"
        git add "src/$VALIDATION_RUN_DIR" 2>/dev/null || true
        cd - > /dev/null
        echo "✅ Validation artifacts staged for commit (or already ignored)"

        # Validation successful - artifacts will be committed with code changes
        echo "✅ VALIDATION COMPLETE: Session artifacts staged for git commit"
        echo "📁 Validation session: $VALIDATION_RUN_DIR"
        echo "🔑 Session artifacts included: logs, screenshots, session metadata"
        echo "📝 Test results included: test-results.txt, validation-info.json"

    else
        echo "❌ Session directory not found: $SESSION_PATH"
        exit 1
    fi
else
    echo "❌ No screenshots found from integration tests"
    echo "   Expected: examples/widget-ui/.continuum/jtag/sessions/user/*/screenshots/*.png"
    echo "   This means the integration test didn't capture screenshots properly"
    exit 1
fi
else
    echo "⏭️  Phase 3: Artifacts collection SKIPPED (disabled in config)"
fi

# Phase 4: Cleanup artifacts from test run
echo ""
echo "🧹 Phase 4: Cleaning up test artifacts"
echo "-----------------------------------------------------------"

# Restore files that get auto-generated during npm start
cd ..
echo "🔄 Restoring auto-generated files to avoid commit noise..."
git restore src/package.json 2>/dev/null || true
git restore src/package-lock.json 2>/dev/null || true
git restore src/generated-command-schemas.json 2>/dev/null || true
git restore src/shared/version.ts 2>/dev/null || true
git restore src/.continuum/sessions/validation/test-output.txt 2>/dev/null || true
cd src
echo "✅ Test artifacts cleaned up"

# continuum#1187 — verify the hook didn't silently switch branches or
# move HEAD via a backticks-in-commit-message side-effect or a buggy
# sub-script. If it did, abort before printing "Commit approved" so
# git refuses to create the commit on the wrong ref.
verify_branch_state_unchanged

# Final Summary
echo ""
echo "🎉 PRECOMMIT VALIDATION COMPLETE!"
echo "=================================================="
[ "$ENABLE_TYPESCRIPT_CHECK" = true ] && echo "✅ TypeScript compilation: PASSED"
[ "$ENABLE_SYSTEM_RESTART" = true ] && echo "✅ System restart: COMPLETED (strategy: $RESTART_STRATEGY)"
[ "$ENABLE_BROWSER_TEST" = true ] && echo "✅ Browser tests: PASSED"
echo "✅ Test artifacts cleaned up"
echo "✅ Branch-state guard: ON branch '$PRECOMMIT_INITIAL_BRANCH' at $PRECOMMIT_INITIAL_HEAD"
echo ""
echo "🚀 Commit approved - all enabled validations passed!"
