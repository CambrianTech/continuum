#!/usr/bin/env bash
# canary-smoke-airc-queue.sh — AIRC + queue-lifecycle slice of the canary
# end-to-end smoke matrix (continuum#1132 PR-1).
#
# WHY THIS GATE EXISTS
#
# Alpha confidence requires more than compile checks. cmd_queue.sh shipped
# six verbs in seven days (airc#566/#568/#573/#574/#583/#581) — the dispatch
# table, help text, dry-run paths, and envelope shapes drift the moment
# nobody re-exercises the CLI surface. This script is the canary check that
# catches drift early instead of letting it land in a peer's bash session.
#
# WHAT IT VALIDATES (PR-1 SCOPE — AIRC + queue subset only)
#
#   1. `airc` is on PATH and answers --version (binary present).
#   2. `airc queue --help` lists every documented verb the dispatch table
#      claims (catches: dispatcher and help drift apart, e.g. PR-2 forgot
#      to register `claim` in --help).
#   3. `airc queue add owner/repo --title X --dry-run` emits a card body
#      with `kind: "airc-queue-card-v1"` (catches: envelope schema drift).
#   4. `airc queue claim owner/repo#1 --dry-run` emits a status-log entry
#      (catches: mutate-card path silently drops log entries).
#   5. `airc queue set-status owner/repo#1 review --dry-run` shows the
#      enum-validated state transition (catches: enum guard regresses).
#   6. `airc queue close-merged <fake-pr-url> --dry-run` parses the PR ref
#      shape and emits the would-close summary (catches: airc#576 ref
#      parser regresses).
#
# OTHER SLICES OUT OF SCOPE — handed to peers in their territory:
#   - Cargo + features parity (sibling/codex)
#   - JTAG ping/screenshot (anyone with a running stack)
#   - Persona/chat path proof (anyone with personas seeded)
#   - ts-rs export sync ratchet (sibling tab #1, continuum#1132 PR-2)
#   - Docker/Carl install gate (already lives at carl-install-smoke.sh)
#
# RUNNING
#
#   bash scripts/ci/canary-smoke-airc-queue.sh
#
# Optional env:
#   AIRC_BIN=/path/to/airc      override which airc binary to test
#   SMOKE_VERBOSE=1             show per-step output (default: only failures)
#
# EXIT CODES
#
#   0  every check passed
#   1  airc binary not present (skip — gate is opt-in for repos w/o airc)
#   2  one or more checks failed (script reports which)
#
# DESIGN CHOICES
#
#  - Dry-run only. No actual GitHub writes, no actual AIRC mesh traffic.
#    Live-mode roundtrips need a test room/repo; deferred to PR-3+ when
#    the canary smoke matrix has a budget for ephemeral test fixtures.
#  - Fake-gh shim under a temp PATH so `airc queue close-merged` can
#    exercise its envelope-fetch path without needing real gh auth.
#  - Isolated AIRC_HOME so we don't pollute the operator's real scope.

set -uo pipefail

AIRC_BIN="${AIRC_BIN:-airc}"
SMOKE_VERBOSE="${SMOKE_VERBOSE:-0}"

# Resolve airc to an absolute path BEFORE we override PATH below — the
# fake-gh PATH narrowing would otherwise hide a perfectly-installed airc
# binary that lives in ~/.local/bin or wherever the user installed it.
if command -v "$AIRC_BIN" >/dev/null 2>&1; then
  AIRC_BIN=$(command -v "$AIRC_BIN")
fi

PASS_COUNT=0
FAIL_COUNT=0
FAILED_STEPS=()

# Isolated temp dir for state + fake gh.
TMPDIR_SMOKE=$(mktemp -d -t airc-queue-smoke.XXXXXX) || {
  printf 'FATAL: mktemp failed\n' >&2
  exit 2
}
trap 'rm -rf "$TMPDIR_SMOKE"' EXIT

FAKE_GH_DIR="$TMPDIR_SMOKE/bin"
mkdir -p "$FAKE_GH_DIR"

# Fake gh: returns a synthetic airc-queue card body for `gh issue view`,
# accepts `gh pr view` with a canned merged-PR JSON, no-ops on edits/closes.
# Lets `airc queue claim --dry-run` and `airc queue close-merged --dry-run`
# exercise their full code path without real GitHub.
cat > "$FAKE_GH_DIR/gh" <<'GH_FAKE'
#!/bin/sh
# Fake gh for canary-smoke-airc-queue.sh.
verb1="${1:-}"; verb2="${2:-}"
case "$verb1 $verb2" in
  "issue view")
    # Return a synthetic card body. Honor --jq .body unwrap.
    use_jq=0
    while [ $# -gt 0 ]; do
      case "$1" in
        --jq) use_jq=1; shift; shift ;;
        *) shift ;;
      esac
    done
    body='**airc-queue card**

```json
{
  "kind": "airc-queue-card-v1",
  "id": "smoke-fixture",
  "branch": "feat/x",
  "owner": "previous-owner",
  "status": "in-progress"
}
```
'
    if [ "$use_jq" -eq 1 ]; then
      printf '%s' "$body"
    else
      printf '{"body":'
      python3 -c "import json,sys; print(json.dumps(sys.stdin.read()))" <<< "$body"
      printf '}'
    fi
    ;;
  "pr view")
    cat <<'PR_JSON'
{"body":"Closes #100.\n","mergedAt":"2026-05-13T20:00:00Z","mergeCommit":{"oid":"smokesha0123456789abcdef"},"baseRefName":"canary","url":"https://github.com/CambrianTech/airc/pull/9999"}
PR_JSON
    ;;
  "issue edit"|"issue close")
    # No-op. Real edits/closes are out of scope for dry-run smoke.
    :
    ;;
  *)
    printf '[]'
    ;;
esac
exit 0
GH_FAKE
chmod +x "$FAKE_GH_DIR/gh"

# Isolate airc state. AIRC_NO_IDENTITY_PROMPT prevents the first-run
# identity wizard from blocking on stdin.
export HOME="$TMPDIR_SMOKE"
export AIRC_HOME="$TMPDIR_SMOKE/.airc"
export AIRC_NO_IDENTITY_PROMPT=1
mkdir -p "$AIRC_HOME"

# Put fake gh first on PATH. Keep system bins for python3 etc.
export PATH="$FAKE_GH_DIR:/usr/bin:/bin:/usr/local/bin:/opt/homebrew/bin"

# CRITICAL: airc wraps every `gh` call through `airc_core.gh_backoff` (a
# Python adapter that adds rate-limit budget + audit logging — see
# airc/airc:425). The adapter resolves the gh binary via the
# `AIRC_GH_BIN` env var FIRST, then falls back to PATH. PATH alone
# isn't enough to redirect to fake gh — the adapter overrides PATH with
# its own resolution. Setting AIRC_GH_BIN forces every gh call inside
# airc to use the fake.
export AIRC_GH_BIN="$FAKE_GH_DIR/gh"

# ── helpers ──────────────────────────────────────────────────────────

step() {
  # Run a check; report pass/fail with the step name.
  # Args: <step-name> <command...>
  # Verifies command exits 0 AND stdout contains every required-substring
  # passed via STEP_REQUIRES (newline-separated). STEP_REQUIRES_NOT is the
  # negative — output must NOT contain those substrings.
  local name="$1"
  shift

  local out rc
  out=$("$@" 2>&1)
  rc=$?

  local fail_reason=""
  if [ "$rc" -ne 0 ]; then
    fail_reason="exit=$rc"
  fi

  if [ -n "${STEP_REQUIRES:-}" ]; then
    while IFS= read -r needle; do
      [ -z "$needle" ] && continue
      if ! printf '%s' "$out" | grep -qF "$needle"; then
        fail_reason="${fail_reason}${fail_reason:+ + }missing: $needle"
      fi
    done <<< "$STEP_REQUIRES"
  fi
  if [ -n "${STEP_REQUIRES_NOT:-}" ]; then
    while IFS= read -r needle; do
      [ -z "$needle" ] && continue
      if printf '%s' "$out" | grep -qF "$needle"; then
        fail_reason="${fail_reason}${fail_reason:+ + }unexpected: $needle"
      fi
    done <<< "$STEP_REQUIRES_NOT"
  fi

  if [ -z "$fail_reason" ]; then
    PASS_COUNT=$((PASS_COUNT + 1))
    printf '  ✓ %s\n' "$name"
    if [ "$SMOKE_VERBOSE" -eq 1 ]; then
      printf '%s\n' "$out" | sed 's/^/      /'
    fi
  else
    FAIL_COUNT=$((FAIL_COUNT + 1))
    FAILED_STEPS+=("$name: $fail_reason")
    printf '  ✗ %s — %s\n' "$name" "$fail_reason"
    printf '%s\n' "$out" | sed 's/^/      /'
  fi

  unset STEP_REQUIRES STEP_REQUIRES_NOT
}

# ── preflight ────────────────────────────────────────────────────────

printf '━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n'
printf '  canary-smoke-airc-queue (continuum#1132 PR-1)\n'
printf '  AIRC_BIN=%s\n' "$AIRC_BIN"
printf '  AIRC_HOME=%s (isolated)\n' "$AIRC_HOME"
printf '  fake gh=%s/gh\n' "$FAKE_GH_DIR"
printf '━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n'

if ! command -v "$AIRC_BIN" >/dev/null 2>&1; then
  printf 'SKIP: %s not on PATH. AIRC + queue smoke is opt-in for repos\n' "$AIRC_BIN" >&2
  printf '      that have airc installed. Install via:\n' >&2
  printf '        curl -fsSL https://raw.githubusercontent.com/CambrianTech/airc/main/install.sh | bash\n' >&2
  exit 1
fi

# ── checks ───────────────────────────────────────────────────────────

# 1. Binary present + answers --help (proxies for "the dispatcher loaded
#    every cmd_*.sh module without parse error" — catches a sourced-file
#    syntax error pre-dispatch).
STEP_REQUIRES="airc"
step "airc --help works" \
  "$AIRC_BIN" --help

# 2. queue --help advertises every CORE verb. Core = present on canary
#    today (PR-1/2/3, plus adopt). close-merged is the in-flight airc#581
#    PR; it's checked in step 6 below with a soft-skip path. If a future
#    PR adds a verb to dispatch but forgets to update --help (or vice
#    versa), this catches the asymmetry.
STEP_REQUIRES="add
list
claim
release
set-status
nudge
adopt"
step "queue --help lists every documented core verb" \
  "$AIRC_BIN" queue --help

# 3. queue add --dry-run emits an envelope. Catches: card body shape
#    regresses, kind constant changes, JSON construction breaks.
STEP_REQUIRES='kind
airc-queue-card-v1'
step "queue add --dry-run emits airc-queue-card-v1 envelope" \
  "$AIRC_BIN" queue add CambrianTech/airc \
    --title "smoke fixture" --owner smoke --status claimed --dry-run

# 4. queue claim --dry-run produces a status-log entry. Catches:
#    _airc_queue_mutate_card status-log path regresses.
STEP_REQUIRES='Status log
claim by smoke'
step "queue claim --dry-run writes a status-log entry" \
  "$AIRC_BIN" queue claim CambrianTech/airc#1 \
    --owner smoke --status in-progress --dry-run

# 5. queue set-status enum guard. The dry-run produces a body with the
#    new status; bad status would have died on the enum check.
STEP_REQUIRES='status=review
Status log'
step "queue set-status review --dry-run mutates status field" \
  "$AIRC_BIN" queue set-status CambrianTech/airc#1 review --dry-run

# 5b. Bad status REJECTED with the canonical list. Catches: enum guard
#     regression where a typo would silently coerce.
STEP_REQUIRES_NOT='status=in-flight'
step "queue set-status rejects unknown state with canonical list" \
  bash -c "
    out=\$(\"$AIRC_BIN\" queue set-status CambrianTech/airc#1 in-flight 2>&1)
    rc=\$?
    if [ \"\$rc\" -eq 0 ]; then
      echo 'FAIL: bad status accepted (rc=0)'
      echo \"\$out\"
      exit 1
    fi
    echo \"\$out\"
    if ! echo \"\$out\" | grep -q 'review'; then
      echo 'FAIL: error must list canonical states'
      exit 1
    fi
    exit 0
  "

# 6. queue close-merged --dry-run parses a PR URL + emits the would-close
#    summary. Exercises the airc#576 ref parser end-to-end against the
#    fake-gh fixture (PR body Closes #100; envelope card body).
#
# Soft-skip when close-merged isn't in this airc build — airc#581 is the
# in-flight PR; smoke runs against whatever airc is on canary. Once #581
# merges, this step starts running automatically.
if "$AIRC_BIN" queue close-merged --help >/dev/null 2>&1; then
  # Note: airc#587 (post-#576) extended the parser to scan PR title AND
  # body. Older airc says "scanned N body refs"; current airc says
  # "scanned N title/body refs". Match the per-card lines + summary
  # which are stable across both formats.
  STEP_REQUIRES='[dry-run]
CambrianTech/airc#100
1 closed'
  step "queue close-merged --dry-run parses PR refs + would-close summary" \
    "$AIRC_BIN" queue close-merged \
      https://github.com/CambrianTech/airc/pull/9999 --dry-run
else
  printf '  ⊘ queue close-merged — verb not in this airc build (airc#581 pending)\n'
fi

# ── summary ──────────────────────────────────────────────────────────

printf '━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n'
printf '  canary-smoke-airc-queue: %d passed, %d failed\n' "$PASS_COUNT" "$FAIL_COUNT"
printf '━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n'

if [ "$FAIL_COUNT" -gt 0 ]; then
  printf 'Failed steps:\n'
  for s in "${FAILED_STEPS[@]}"; do
    printf '  ✗ %s\n' "$s"
  done
  exit 2
fi

exit 0
