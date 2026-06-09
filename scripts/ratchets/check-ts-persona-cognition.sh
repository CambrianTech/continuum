#!/bin/bash
# check-ts-persona-cognition.sh — Lane F ratchet (PR #1084).
#
# Enforces "TS persona cognition must shrink." Counts current LOC under
# src/system/user/server (excluding *.test.ts / *.spec.ts), compares to
# the baseline in scripts/ratchets/ts-persona-cognition-baseline.json,
# fails (exit 1) if current > baseline, succeeds (exit 0) otherwise.
#
# Per Rust-first alpha contract (PR #1070, ALPHA-GAP-ANALYSIS.md "Rust
# core owns behavior"): every PR touching the persona surface must
# either keep the line count flat or shrink it. New cognition logic
# belongs in Rust (`workers/continuum-core/src/persona/`,
# `workers/continuum-core/src/cognition/`), not in this TS surface.
#
# Modes:
#   ./check-ts-persona-cognition.sh              # check + report; exit 0/1
#   ./check-ts-persona-cognition.sh --update-baseline   # update + commit-ready (use after legitimate shrinks)
#   ./check-ts-persona-cognition.sh --verbose     # print per-file LOC table

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BASELINE_FILE="$SCRIPT_DIR/ts-persona-cognition-baseline.json"
SURFACE_DIR="$REPO_ROOT/src/system/user/server"

YELLOW='\033[1;33m'
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

UPDATE_BASELINE=0
VERBOSE=0
for arg in "$@"; do
  case "$arg" in
    --update-baseline) UPDATE_BASELINE=1 ;;
    --verbose|-v)      VERBOSE=1 ;;
    --help|-h)
      echo "Usage: $0 [--update-baseline] [--verbose]"
      echo "  Default: check current LOC against baseline; exit non-zero on growth."
      echo "  --update-baseline: rewrite baseline to current count (use after a legitimate shrink)."
      echo "  --verbose: print per-file LOC table."
      exit 0
      ;;
    *)
      echo -e "${RED}Unknown arg: $arg${NC}" >&2
      exit 2
      ;;
  esac
done

if [[ ! -d "$SURFACE_DIR" ]]; then
  echo -e "${RED}ERROR: surface directory not found: $SURFACE_DIR${NC}" >&2
  exit 2
fi

if [[ ! -f "$BASELINE_FILE" ]]; then
  echo -e "${RED}ERROR: baseline file not found: $BASELINE_FILE${NC}" >&2
  echo "  Generate one by running this script with --update-baseline (the first time)." >&2
  exit 2
fi

# Count current TS LOC excluding tests. Use find + wc for portability;
# bash glob ** requires shopt globstar which isn't always set in CI.
CURRENT_TOTAL=$(find "$SURFACE_DIR" -type f -name "*.ts" \
  -not -name "*.test.ts" -not -name "*.spec.ts" \
  -exec cat {} + | wc -l | tr -d ' ')

# Read baseline. Use python3 (always present) instead of jq (may not be).
BASELINE=$(python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['total_lines'])" "$BASELINE_FILE")

DELTA=$((CURRENT_TOTAL - BASELINE))

if [[ "$VERBOSE" -eq 1 ]]; then
  echo -e "${YELLOW}━━ TS persona-cognition surface (per-file LOC) ━━${NC}"
  find "$SURFACE_DIR" -type f -name "*.ts" \
    -not -name "*.test.ts" -not -name "*.spec.ts" \
    -exec wc -l {} + | sort -n | tail -20
  echo ""
fi

if [[ "$UPDATE_BASELINE" -eq 1 ]]; then
  CURRENT_SHA=$(git -C "$REPO_ROOT" rev-parse --short HEAD 2>/dev/null || echo "unknown")
  CURRENT_ISO=$(date -u +"%Y-%m-%dT%H:%MZ")
  python3 - "$BASELINE_FILE" "$CURRENT_TOTAL" "$CURRENT_SHA" "$CURRENT_ISO" <<'PYEOF'
import json, sys
path, total, sha, iso = sys.argv[1], int(sys.argv[2]), sys.argv[3], sys.argv[4]
with open(path) as f:
    data = json.load(f)
data["total_lines"] = total
data["_baseline_anchored_at_canary"] = sha
data["_anchored_at_iso"] = iso
with open(path, "w") as f:
    json.dump(data, f, indent=2)
    f.write("\n")
PYEOF
  echo -e "${GREEN}✓ baseline updated to ${CURRENT_TOTAL} (was ${BASELINE}, delta ${DELTA})${NC}"
  echo "  Commit: git add $BASELINE_FILE"
  exit 0
fi

if [[ "$DELTA" -gt 0 ]]; then
  echo -e "${RED}━━ ❌ TS persona-cognition RATCHET FAILED ━━${NC}" >&2
  echo -e "${RED}  Baseline: ${BASELINE} lines${NC}" >&2
  echo -e "${RED}  Current : ${CURRENT_TOTAL} lines${NC}" >&2
  echo -e "${RED}  Delta   : +${DELTA} (growth)${NC}" >&2
  echo "" >&2
  echo "  Per Rust-first alpha contract (PR #1070, docs/planning/ALPHA-GAP-ANALYSIS.md)," >&2
  echo "  the TS persona surface must SHRINK or stay flat. New cognition logic belongs" >&2
  echo "  in Rust:" >&2
  echo "    workers/continuum-core/src/persona/" >&2
  echo "    workers/continuum-core/src/cognition/" >&2
  echo "" >&2
  echo "  Options:" >&2
  echo "    1. Move the new code Rust-side." >&2
  echo "    2. Delete equivalent TS LOC elsewhere in the surface to keep total flat or below." >&2
  echo "    3. If this PR genuinely shrinks net (despite some additions), re-run after the" >&2
  echo "       deletes land in this branch." >&2
  echo "" >&2
  echo "  Current top files (run with --verbose for full table):" >&2
  find "$SURFACE_DIR" -type f -name "*.ts" \
    -not -name "*.test.ts" -not -name "*.spec.ts" \
    -exec wc -l {} + | sort -n | tail -5 >&2
  exit 1
fi

if [[ "$DELTA" -eq 0 ]]; then
  echo -e "${GREEN}✓ TS persona-cognition ratchet held: ${CURRENT_TOTAL} lines (baseline ${BASELINE}, no change)${NC}"
else
  echo -e "${GREEN}✓ TS persona-cognition ratchet shrank: ${CURRENT_TOTAL} lines (baseline ${BASELINE}, delta ${DELTA})${NC}"
  echo "  After merge: run this script with --update-baseline to lower the baseline."
fi
exit 0
