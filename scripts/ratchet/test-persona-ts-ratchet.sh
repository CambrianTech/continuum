#!/usr/bin/env bash
#
# Tests for scripts/ratchet/persona-ts-ratchet.sh — Lane F PR-1.
#
# Each test sets up a temp tree with a mocked persona-cognition layout
# and a controlled baseline + allowlist, then asserts the script's exit
# code and (where useful) a substring of its output. No mocks of bash
# itself — these are real subprocess invocations of the real script.
#
# Run: scripts/ratchet/test-persona-ts-ratchet.sh
# Run a single case: scripts/ratchet/test-persona-ts-ratchet.sh case_clean_baseline

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
RATCHET="$SCRIPT_DIR/persona-ts-ratchet.sh"

PASS=0
FAIL=0
FAILURES=()

# Each test case sets up a temp dir representing a mock repo root with
# only the watched cognition dirs populated, plus a baseline + allowlist
# file at known temp paths.
new_fixture_root() {
    local root
    root="$(mktemp -d -t lane-f-fixture.XXXX)"
    mkdir -p "$root/src/system/user/server/modules/cognition"
    mkdir -p "$root/src/system/user/server/modules/cognitive"
    mkdir -p "$root/src/system/user/server/modules/consciousness"
    mkdir -p "$root/src/system/user/server/modules/being"
    mkdir -p "$root/src/system/user/server/modules/central-nervous-system"
    mkdir -p "$root/src/system/user/server/attention"
    mkdir -p "$root/src/system/ai/server"
    echo "$root"
}

write_ts() {
    local path="$1"
    local lines="$2"
    mkdir -p "$(dirname "$path")"
    {
        for ((i = 1; i <= lines; i++)); do
            echo "// line $i"
        done
    } > "$path"
}

# Generate a baseline file from a root by invoking the script's refresh mode.
gen_baseline() {
    local root="$1"
    local baseline="$2"
    local allowlist="$3"
    PERSONA_RATCHET_BASELINE="$baseline" \
    PERSONA_RATCHET_ALLOWLIST="$allowlist" \
        "$RATCHET" --root "$root" refresh > /dev/null
}

run_check() {
    local root="$1"
    local baseline="$2"
    local allowlist="$3"
    PERSONA_RATCHET_BASELINE="$baseline" \
    PERSONA_RATCHET_ALLOWLIST="$allowlist" \
        "$RATCHET" --root "$root" check
}

# Asserts $1 (test name) by running $2 (callable) — pass if exit 0.
assert() {
    local name="$1"; shift
    if "$@"; then
        PASS=$((PASS + 1))
        echo "PASS  $name"
    else
        FAIL=$((FAIL + 1))
        FAILURES+=("$name")
        echo "FAIL  $name"
    fi
}

# Tiny helper: assert a command exits with a specific code.
assert_exit() {
    local expected="$1"; shift
    local actual=0
    "$@" > /dev/null 2>&1 || actual=$?
    [[ "$actual" -eq "$expected" ]]
}

# --- Cases --------------------------------------------------------------

case_clean_baseline_passes() {
    local root; root="$(new_fixture_root)"
    write_ts "$root/src/system/user/server/modules/cognition/A.ts" 10
    write_ts "$root/src/system/user/server/modules/being/B.ts" 5
    local baseline; baseline="$(mktemp)"
    local allowlist; allowlist="$(mktemp)"
    : > "$allowlist"
    gen_baseline "$root" "$baseline" "$allowlist"
    assert "clean_baseline_passes" assert_exit 0 \
        env PERSONA_RATCHET_BASELINE="$baseline" PERSONA_RATCHET_ALLOWLIST="$allowlist" \
        "$RATCHET" --root "$root" check
    rm -rf "$root" "$baseline" "$allowlist"
}

case_loc_growth_in_existing_file_fails() {
    local root; root="$(new_fixture_root)"
    write_ts "$root/src/system/user/server/modules/cognition/A.ts" 10
    local baseline; baseline="$(mktemp)"
    local allowlist; allowlist="$(mktemp)"
    : > "$allowlist"
    gen_baseline "$root" "$baseline" "$allowlist"
    # Now grow the file — same file, more lines. Baseline LOC was 10; now 30.
    write_ts "$root/src/system/user/server/modules/cognition/A.ts" 30
    assert "loc_growth_in_existing_file_fails" assert_exit 1 \
        env PERSONA_RATCHET_BASELINE="$baseline" PERSONA_RATCHET_ALLOWLIST="$allowlist" \
        "$RATCHET" --root "$root" check
    rm -rf "$root" "$baseline" "$allowlist"
}

case_new_unallowed_ts_file_fails() {
    local root; root="$(new_fixture_root)"
    write_ts "$root/src/system/user/server/modules/cognition/A.ts" 10
    local baseline; baseline="$(mktemp)"
    local allowlist; allowlist="$(mktemp)"
    : > "$allowlist"
    gen_baseline "$root" "$baseline" "$allowlist"
    # New verb-shaped file appearing after baseline — must fail.
    write_ts "$root/src/system/user/server/modules/cognition/NewCognitionController.ts" 20
    assert "new_unallowed_ts_file_fails" assert_exit 1 \
        env PERSONA_RATCHET_BASELINE="$baseline" PERSONA_RATCHET_ALLOWLIST="$allowlist" \
        "$RATCHET" --root "$root" check
    rm -rf "$root" "$baseline" "$allowlist"
}

case_new_allowlisted_generated_passes() {
    local root; root="$(new_fixture_root)"
    write_ts "$root/src/system/user/server/modules/cognition/A.ts" 10
    local baseline; baseline="$(mktemp)"
    local allowlist; allowlist="$(mktemp)"
    cat > "$allowlist" <<'EOF'
**/*.generated.ts
**/*.gen.ts
**/generated/**/*.ts
EOF
    gen_baseline "$root" "$baseline" "$allowlist"
    # New generated file appearing post-baseline — matches allowlist, passes.
    # NOTE: LOC must NOT exceed baseline either. Generated file goes into the
    # generated/ subdir whose LOC IS counted; bumping LOC must also pass
    # baseline. We deliberately grow zero lines in the watched dir's *non-
    # generated* paths but the generated file DOES bump the LOC count for
    # the parent dir. Allowlist-passing files still count toward LOC.
    # So: shrink the existing file by the same number of lines we add.
    write_ts "$root/src/system/user/server/modules/cognition/A.ts" 5
    write_ts "$root/src/system/user/server/modules/cognition/generated/Foo.gen.ts" 5
    assert "new_allowlisted_generated_passes" assert_exit 0 \
        env PERSONA_RATCHET_BASELINE="$baseline" PERSONA_RATCHET_ALLOWLIST="$allowlist" \
        "$RATCHET" --root "$root" check
    rm -rf "$root" "$baseline" "$allowlist"
}

case_new_types_file_passes() {
    local root; root="$(new_fixture_root)"
    write_ts "$root/src/system/user/server/modules/cognition/A.ts" 10
    local baseline; baseline="$(mktemp)"
    local allowlist; allowlist="$(mktemp)"
    cat > "$allowlist" <<'EOF'
**/*.types.ts
EOF
    gen_baseline "$root" "$baseline" "$allowlist"
    # Same LOC trade — shrink A by what we add as types.
    write_ts "$root/src/system/user/server/modules/cognition/A.ts" 5
    write_ts "$root/src/system/user/server/modules/cognition/Decision.types.ts" 5
    assert "new_types_file_passes" assert_exit 0 \
        env PERSONA_RATCHET_BASELINE="$baseline" PERSONA_RATCHET_ALLOWLIST="$allowlist" \
        "$RATCHET" --root "$root" check
    rm -rf "$root" "$baseline" "$allowlist"
}

case_deletion_after_refresh_passes() {
    local root; root="$(new_fixture_root)"
    write_ts "$root/src/system/user/server/modules/cognition/A.ts" 100
    write_ts "$root/src/system/user/server/modules/cognition/B.ts" 100
    local baseline; baseline="$(mktemp)"
    local allowlist; allowlist="$(mktemp)"
    : > "$allowlist"
    gen_baseline "$root" "$baseline" "$allowlist"
    # Delete B entirely. LOC shrinks (100 -> 0 for B). Still passes.
    rm "$root/src/system/user/server/modules/cognition/B.ts"
    assert "deletion_after_refresh_passes" assert_exit 0 \
        env PERSONA_RATCHET_BASELINE="$baseline" PERSONA_RATCHET_ALLOWLIST="$allowlist" \
        "$RATCHET" --root "$root" check
    rm -rf "$root" "$baseline" "$allowlist"
}

case_missing_baseline_returns_2() {
    local root; root="$(new_fixture_root)"
    local baseline="$root/nonexistent-baseline.txt"
    local allowlist; allowlist="$(mktemp)"
    : > "$allowlist"
    assert "missing_baseline_returns_2" assert_exit 2 \
        env PERSONA_RATCHET_BASELINE="$baseline" PERSONA_RATCHET_ALLOWLIST="$allowlist" \
        "$RATCHET" --root "$root" check
    rm -rf "$root" "$allowlist"
}

case_ai_server_shim_growth_fails() {
    local root; root="$(new_fixture_root)"
    write_ts "$root/src/system/ai/server/AIDecisionService.ts" 10
    local baseline; baseline="$(mktemp)"
    local allowlist; allowlist="$(mktemp)"
    : > "$allowlist"
    gen_baseline "$root" "$baseline" "$allowlist"
    write_ts "$root/src/system/ai/server/AIDecisionService.ts" 25
    assert "ai_server_shim_growth_fails" assert_exit 1 \
        env PERSONA_RATCHET_BASELINE="$baseline" PERSONA_RATCHET_ALLOWLIST="$allowlist" \
        "$RATCHET" --root "$root" check
    rm -rf "$root" "$baseline" "$allowlist"
}

case_refresh_writes_baseline_idempotently() {
    local root; root="$(new_fixture_root)"
    write_ts "$root/src/system/user/server/modules/cognition/A.ts" 12
    write_ts "$root/src/system/user/server/modules/being/B.ts" 7
    local baseline; baseline="$(mktemp)"
    local allowlist; allowlist="$(mktemp)"
    : > "$allowlist"
    PERSONA_RATCHET_BASELINE="$baseline" PERSONA_RATCHET_ALLOWLIST="$allowlist" \
        "$RATCHET" --root "$root" refresh > /dev/null
    local first; first="$(grep -v '^# Refreshed' "$baseline")"
    PERSONA_RATCHET_BASELINE="$baseline" PERSONA_RATCHET_ALLOWLIST="$allowlist" \
        "$RATCHET" --root "$root" refresh > /dev/null
    local second; second="$(grep -v '^# Refreshed' "$baseline")"
    assert "refresh_writes_baseline_idempotently" test "$first" = "$second"
    rm -rf "$root" "$baseline" "$allowlist"
}

# Selective run: argument names a specific case_*.
if [[ $# -gt 0 ]]; then
    "$1"
else
    case_clean_baseline_passes
    case_loc_growth_in_existing_file_fails
    case_new_unallowed_ts_file_fails
    case_new_allowlisted_generated_passes
    case_new_types_file_passes
    case_deletion_after_refresh_passes
    case_missing_baseline_returns_2
    case_ai_server_shim_growth_fails
    case_refresh_writes_baseline_idempotently
fi

echo
echo "================================"
echo "Pass: $PASS    Fail: $FAIL"
echo "================================"

if [[ $FAIL -gt 0 ]]; then
    for n in "${FAILURES[@]}"; do
        echo "  fail: $n" >&2
    done
    exit 1
fi
exit 0
