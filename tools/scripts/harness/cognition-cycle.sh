#!/usr/bin/env bash
#
# cognition-cycle.sh — ONE measured iteration of the persona-cognition harness.
#
# This is the executable companion to docs/architecture/HARNESS-RUNBOOK.md (the
# amnesiac entry point) and docs/architecture/PERFORMANCE-HARNESS-FRAMEWORK.md
# (the design / VDD-record schema). If you are a fresh instance with no context:
# read the runbook, then run THIS. It does, in order, idempotently and failing
# loud at the first missing precondition:
#
#   1. resolve the `cu` client (build hint if absent)
#   2. confirm the headless core is up on its IPC socket   (cu ping)
#   3. resolve the target persona (by name or uuid)        (cognition/personas)
#   4. snapshot every glass-box capture stream's length BEFORE the run
#   5. run `cu cognition/eval` (single-pass, or A/B with --gene)
#   6. delta the capture streams; collect THIS run's new lines
#   7. write a timestamped report dir + print the headline VDD-shaped record
#
# It measures a COPY of the persona (humane snapshot-eval, #59) — running it never
# degrades the living citizen. Safe to run against a live, working core.
#
# Doctrine honored: no fallbacks — every missing precondition fails loud naming the
# cause + the fix (`cu` returns rc=0 even on substrate refusal, so we parse output,
# never trust $?). Pure-Rust core + cu only; never npm/jtag.
#
# Usage:
#   cognition-cycle.sh [--persona NAME|UUID] [--eval-set PATH] [--note LABEL]
#                      [--gene JSON] [--max-acts N] [--dry-run]
#
#   --persona    persona to measure (name like "Asha" or a UUID). Default: Asha.
#   --eval-set   JSONL gym corpus. Default: docs/genome/coder-eval.jsonl
#                (navigation/knowledge, 13 tasks, substring-graded). The write set
#                docs/genome/coder-write-eval.jsonl is rustc-graded, 30 tasks.
#   --note       trend-line label written into the progress ledger + report.
#   --gene       A/B mode: JSON {"name":"...","path":"...","scale":1.0}. Omit for
#                single-pass baseline on the persona's live lane.
#   --max-acts   cap on agent-loop act→observe iterations per task.
#   --dry-run    do all preconditions + print the exact eval command, but DON'T run
#                it (validates plumbing without spending inference).
#
set -euo pipefail

# ---- repo root (this script lives at tools/scripts/harness/) -----------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

# ---- defaults ----------------------------------------------------------------
PERSONA="Asha"
EVAL_SET="docs/genome/coder-eval.jsonl"
NOTE=""
GENE=""
MAX_ACTS=""
DRY_RUN=0
SOCKET="${CONTINUUM_CORE_SOCKET:-/tmp/continuum-core.sock}"
TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/.continuum/cache/cargo-target}"
FIXTURES="$HOME/.continuum/fixtures"

# ---- arg parse ---------------------------------------------------------------
while [[ $# -gt 0 ]]; do
  case "$1" in
    --persona)   PERSONA="$2"; shift 2 ;;
    --eval-set)  EVAL_SET="$2"; shift 2 ;;
    --note)      NOTE="$2"; shift 2 ;;
    --gene)      GENE="$2"; shift 2 ;;
    --max-acts)  MAX_ACTS="$2"; shift 2 ;;
    --dry-run)   DRY_RUN=1; shift ;;
    -h|--help)   sed -n '2,40p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "harness: unknown arg '$1' (see --help)" >&2; exit 2 ;;
  esac
done

die() { echo "harness: FAIL — $1" >&2; exit 1; }
need() { command -v "$1" >/dev/null || die "missing '$1' on PATH — $2"; }

need jq "install with: brew install jq"

# ---- 1. resolve cu -----------------------------------------------------------
CU="$TARGET_DIR/debug/cu"
[[ -x "$CU" ]] || CU="$TARGET_DIR/release/cu"
[[ -x "$CU" ]] || die "cu client not built. Build it:
    export CARGO_TARGET_DIR=\"$TARGET_DIR\"
    cargo build --manifest-path core/continuum-core/Cargo.toml --bin cu --features metal,accelerate"
export CONTINUUM_CORE_SOCKET="$SOCKET"

# cu_json CMD [ARGS...] — run a cu command, return stdout. Fail loud if the
# substrate refused (cu prints to stderr + still exits 0, so we sniff the text).
# A genuine result is always valid JSON; a refusal is plain stderr text. So the
# refusal sniff runs ONLY when the output does not parse as JSON — otherwise a
# legitimate eval payload whose answer text happens to contain "FAIL"/"error:"
# would false-positive into a die() (intermittent, answer-content-dependent).
cu_json() {
  local out
  out="$("$CU" "$@" 2>&1)" || true
  if ! jq -e . >/dev/null 2>&1 <<<"$out"; then
    if grep -qiE "substrate refused|Unknown command|FAIL|error:" <<<"$out"; then
      die "cu $* refused:
$out"
    fi
  fi
  printf '%s' "$out"
}

# ---- 2. core up? -------------------------------------------------------------
[[ -S "$SOCKET" ]] || die "no core socket at $SOCKET. Start the headless core:
    bash tools/scripts/start-server.sh
  (or: cargo run --manifest-path core/continuum-core/Cargo.toml --bin continuum-core-server --features metal,accelerate -- $SOCKET)"
PING="$(cu_json ping)"
[[ "$(jq -r '.ok' <<<"$PING")" == "true" ]] || die "core did not answer ping on $SOCKET:
$PING"

# ---- 3. resolve persona ------------------------------------------------------
PERSONAS="$(cu_json cognition/personas)"
# match by exact UUID first, else by case-insensitive name.
PID="$(jq -r --arg p "$PERSONA" '.personas[] | select(.persona_id==$p) | .persona_id' <<<"$PERSONAS")"
PNAME="$PERSONA"
if [[ -z "$PID" ]]; then
  PID="$(jq -r --arg p "$PERSONA" '.personas[] | select((.name|ascii_downcase)==($p|ascii_downcase)) | .persona_id' <<<"$PERSONAS")"
  PNAME="$(jq -r --arg p "$PERSONA" '.personas[] | select((.name|ascii_downcase)==($p|ascii_downcase)) | .name' <<<"$PERSONAS")"
fi
[[ -n "$PID" ]] || die "no live persona '$PERSONA'. Online now:
$(jq -r '.personas[] | "    \(.name)  \(.persona_id)"' <<<"$PERSONAS")
  Spawn one with:  $CU persona/spawn"

EVAL_PATH="$REPO_ROOT/$EVAL_SET"
[[ -f "$EVAL_PATH" ]] || die "eval set not found: $EVAL_PATH"

# ---- 4. snapshot capture streams BEFORE --------------------------------------
linecount() { [[ -f "$1" ]] && wc -l <"$1" | tr -d ' ' || echo 0; }
CAP_PROMPT="$FIXTURES/prompt-captures/$PID.jsonl"
CAP_WS="$FIXTURES/workspace-traces/$PID.jsonl"
CAP_PLACE="$FIXTURES/placement-decisions/decisions.jsonl"
LEDGER="$HOME/.continuum/progress/$PID.jsonl"
PRE_PROMPT=$(linecount "$CAP_PROMPT"); PRE_WS=$(linecount "$CAP_WS")
PRE_PLACE=$(linecount "$CAP_PLACE");   PRE_LEDGER=$(linecount "$LEDGER")

# ---- 5. build + run the eval -------------------------------------------------
STAMP="$(date +%Y%m%d-%H%M%S)"
RUN_DIR="$HOME/.continuum/harness-runs/$STAMP-${PNAME}"

EVAL_ARGS=(cognition/eval --persona_id "$PID" --eval_set "$EVAL_PATH")
[[ -n "$NOTE" ]]     && EVAL_ARGS+=(--note "$NOTE")
[[ -n "$GENE" ]]     && EVAL_ARGS+=(--gene "$GENE")
[[ -n "$MAX_ACTS" ]] && EVAL_ARGS+=(--max_acts "$MAX_ACTS")

echo "harness: persona=$PNAME ($PID)  eval_set=$EVAL_SET  note='${NOTE:-}'"
echo "harness: report → $RUN_DIR"
echo "harness: command → $CU ${EVAL_ARGS[*]}"

if [[ "$DRY_RUN" == "1" ]]; then
  echo "harness: --dry-run, not executing the eval."
  exit 0
fi

mkdir -p "$RUN_DIR"
echo "harness: running gym (forks a measurement copy; the living $PNAME is untouched)…"
START_MS=$(($(date +%s%N)/1000000))
RESULT="$(cu_json "${EVAL_ARGS[@]}")"
END_MS=$(($(date +%s%N)/1000000))
WALL_MS=$((END_MS - START_MS))
printf '%s\n' "$RESULT" >"$RUN_DIR/eval.json"

# ---- 6. delta the capture streams --------------------------------------------
POST_PLACE=$(linecount "$CAP_PLACE")
# the placement verdict(s) emitted by THIS run:
if [[ $POST_PLACE -gt $PRE_PLACE ]]; then
  tail -n $((POST_PLACE - PRE_PLACE)) "$CAP_PLACE" >"$RUN_DIR/placement-decisions.jsonl"
fi
NEW_PROMPT=$(( $(linecount "$CAP_PROMPT") - PRE_PROMPT ))
NEW_WS=$(( $(linecount "$CAP_WS") - PRE_WS ))
NEW_LEDGER=$(( $(linecount "$LEDGER") - PRE_LEDGER ))

# ---- 7. headline VDD-shaped record + report.md -------------------------------
j() { jq -r "$1 // \"null\"" <<<"$RESULT"; }
# prefill_share — fraction of LANE time (prefill+decode) spent prefilling. This is
# the headline "where the time goes" number the instrumentation exists to surface;
# the lever (KV-prefix reuse) is what drives it down. "n/a" when the lane gave no
# timings (cloud / older endpoint).
prefill_share() {
  jq -r '(.total_prefill_ms // 0) as $p | (.total_decode_ms // 0) as $d
         | if ($p + $d) > 0 then "\((($p*100)/($p+$d))|floor)%" else "n/a" end' <<<"$RESULT"
}
REPORT="$RUN_DIR/report.md"
{
  echo "# Cognition harness cycle — $PNAME — $STAMP"
  echo
  echo "- persona:        $PNAME ($PID)"
  echo "- eval_set:       $EVAL_SET"
  echo "- note:           ${NOTE:-（none）}"
  echo "- gene:           ${GENE:-（single-pass baseline）}"
  echo "- git_sha:        $(git -C "$REPO_ROOT" rev-parse --short HEAD)"
  echo "- wall_clock_ms:  $WALL_MS"
  echo
  echo "## Result (the numbers that move)"
  echo
  echo "| metric | value |"
  echo "|---|---|"
  echo "| pass_rate | $(j .pass_rate) ( $(j .score)/$(j .total) ) |"
  echo "| lift (vs base) | $(j .lift) |"
  echo "| self_verify_rate | $(j .self_verify_rate) |"
  echo "| mean_latency_ms | $(j .mean_latency_ms) |"
  echo "| p95_latency_ms | $(j .p95_latency_ms) |"
  echo "| mean_tokens_per_second (wall-clock, diluted) | $(j .mean_tokens_per_second) |"
  echo "| **mean_decode_tokens_per_second (real lane rate)** | **$(j .mean_decode_tokens_per_second)** |"
  echo "| **mean_cache_hit_rate** (→1.0 = prefix stayed resident) | **$(j .mean_cache_hit_rate)** |"
  echo "| total_prefill_ms (the re-rasterization tax) | $(j .total_prefill_ms) |"
  echo "| total_decode_ms (actual generation) | $(j .total_decode_ms) |"
  echo "| prefill share of lane time | $(prefill_share) |"
  echo "| total_output_tokens | $(j .total_output_tokens) |"
  echo "| **lane_placement** | **$(j .lane_placement)** — $(j .lane_placement_reason) |"
  echo "| lane_free_vram_bytes | $(j .lane_free_vram_bytes) |"
  echo "| lane_estimated_footprint_bytes | $(j .lane_estimated_footprint_bytes) |"
  echo
  echo "## Glass-box streams (new lines this run)"
  echo
  echo "- prompt-captures:    +$NEW_PROMPT   ($CAP_PROMPT)"
  echo "- workspace-traces:   +$NEW_WS   ($CAP_WS)"
  echo "- placement-decisions:+$((POST_PLACE - PRE_PLACE))   → $RUN_DIR/placement-decisions.jsonl"
  echo "- progress-ledger:    +$NEW_LEDGER   ($LEDGER)"
  echo
  echo "## Per-task"
  echo
  echo "| task | ok | acts | latency_ms | wall tok/s | decode tok/s | cache_hit | grade |"
  echo "|---|---|---|---|---|---|---|---|"
  jq -r '.results[]? | "| \(.id) | \(if .ok then "✅" else "❌" end) | \(.acts) | \(.latency_ms) | \(.tokens_per_second|floor) | \((.decode_tokens_per_second // 0)|floor) | \(((.cache_hit_rate // 0)*100|floor))% | \(.grade) |"' <<<"$RESULT"
} >"$REPORT"

echo
echo "════════════════════════════════════════════════════════════════"
echo " pass_rate $(j .pass_rate)  ($(j .score)/$(j .total))   lift $(j .lift)"
echo " device    $(j .lane_placement)  —  $(j .lane_placement_reason)"
echo " latency   mean $(j .mean_latency_ms)ms  p95 $(j .p95_latency_ms)ms   wall $(j .mean_tokens_per_second) tok/s"
echo " speed     decode $(j .mean_decode_tokens_per_second) tok/s (real)   cache_hit $(j .mean_cache_hit_rate)   prefill $(prefill_share) of lane time"
echo "════════════════════════════════════════════════════════════════"
echo "harness: full report → $REPORT"
