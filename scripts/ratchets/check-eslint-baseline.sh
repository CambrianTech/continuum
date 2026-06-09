#!/bin/bash
# check-eslint-baseline.sh — repo-wide TypeScript ESLint error-count ratchet.
#
# The repo still has historical ESLint debt. This gate makes that debt
# monotonic: fail on growth, and fail on shrink unless the baseline is updated
# in the same branch. That keeps cleanup wins from evaporating between PRs.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
SRC_DIR="$REPO_ROOT/src"
PLATFORM="${ESLINT_BASELINE_PLATFORM:-$(uname -s 2>/dev/null)}"
PLATFORM="$(printf '%s' "$PLATFORM" | tr '[:upper:]' '[:lower:]')"
DEFAULT_BASELINE_FILE="$SRC_DIR/eslint-baseline.txt"
PLATFORM_BASELINE_FILE="$SRC_DIR/eslint-baseline.${PLATFORM}.txt"
if [[ -f "$PLATFORM_BASELINE_FILE" ]]; then
  BASELINE_FILE="$PLATFORM_BASELINE_FILE"
else
  BASELINE_FILE="$DEFAULT_BASELINE_FILE"
fi

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
      echo "  Default: require current ESLint error count to equal the baseline."
      echo "  --update-baseline: rewrite the active platform baseline to the current count."
      echo "  --verbose: print the ESLint error output."
      exit 0
      ;;
    *)
      echo -e "${RED}Unknown arg: $arg${NC}" >&2
      exit 2
      ;;
  esac
done

if [[ ! -d "$SRC_DIR" ]]; then
  echo -e "${RED}ERROR: src directory not found: $SRC_DIR${NC}" >&2
  exit 2
fi

if [[ ! -f "$SRC_DIR/package.json" ]]; then
  echo -e "${RED}ERROR: src/package.json not found${NC}" >&2
  exit 2
fi

if [[ ! -x "$SRC_DIR/node_modules/.bin/eslint" ]]; then
  echo -e "${RED}ERROR: ESLint is not installed in $SRC_DIR/node_modules${NC}" >&2
  echo "  Run: cd src && npm install" >&2
  exit 2
fi

if [[ ! -f "$BASELINE_FILE" ]]; then
  echo -e "${RED}ERROR: baseline file not found: $BASELINE_FILE${NC}" >&2
  echo "  Generate one with: bash scripts/ratchets/check-eslint-baseline.sh --update-baseline" >&2
  exit 2
fi

BASELINE="$(tr -d '[:space:]' < "$BASELINE_FILE")"
if [[ ! "$BASELINE" =~ ^[0-9]+$ ]]; then
  echo -e "${RED}ERROR: $BASELINE_FILE must contain a single integer, got: $BASELINE${NC}" >&2
  exit 2
fi

TMP_OUT="$(mktemp "${TMPDIR:-/tmp}/continuum-eslint-ratchet.XXXXXX")"
trap 'rm -f "$TMP_OUT"' EXIT

set +e
(cd "$SRC_DIR" && npx eslint './**/*.ts' --max-warnings 0 --quiet >"$TMP_OUT" 2>&1)
ESLINT_STATUS=$?
set -e

CURRENT="$(grep -cE 'error\s+' "$TMP_OUT" || true)"
DELTA=$((CURRENT - BASELINE))

if [[ "$VERBOSE" -eq 1 ]]; then
  echo -e "${YELLOW}━━ ESLint output ━━${NC}"
  cat "$TMP_OUT"
  echo ""
fi

if [[ "$UPDATE_BASELINE" -eq 1 ]]; then
  printf '%s\n' "$CURRENT" > "$BASELINE_FILE"
  echo -e "${GREEN}✓ eslint baseline updated to ${CURRENT} (was ${BASELINE}, delta ${DELTA})${NC}"
  echo "  Commit: git add $BASELINE_FILE"
  exit 0
fi

if [[ "$CURRENT" -gt "$BASELINE" ]]; then
  echo -e "${RED}━━ ❌ ESLint baseline ratchet failed ━━${NC}" >&2
  echo -e "${RED}  Baseline: ${BASELINE} errors${NC}" >&2
  echo -e "${RED}  Current : ${CURRENT} errors${NC}" >&2
  echo -e "${RED}  Delta   : +${DELTA} new error(s)${NC}" >&2
  echo "" >&2
  echo "  Run for details:" >&2
  echo "    cd src && npx eslint './**/*.ts' --max-warnings 0 --quiet" >&2
  exit 1
fi

if [[ "$CURRENT" -lt "$BASELINE" ]]; then
  echo -e "${RED}━━ ❌ ESLint baseline can be lowered ━━${NC}" >&2
  echo -e "${RED}  Baseline: ${BASELINE} errors${NC}" >&2
  echo -e "${RED}  Current : ${CURRENT} errors${NC}" >&2
  echo -e "${RED}  Delta   : ${DELTA} fewer error(s)${NC}" >&2
  echo "" >&2
  echo "  Lock the win in this PR:" >&2
  echo "    bash scripts/ratchets/check-eslint-baseline.sh --update-baseline" >&2
  echo "    git add $BASELINE_FILE" >&2
  exit 1
fi

# If ESLint exits non-zero but the count equals baseline, that is expected debt.
# If it exits zero and count is zero, also fine.
if [[ "$ESLINT_STATUS" -ne 0 && "$CURRENT" -eq 0 ]]; then
  echo -e "${RED}ERROR: ESLint exited non-zero but no error count was detected.${NC}" >&2
  cat "$TMP_OUT" >&2
  exit 2
fi

echo -e "${GREEN}✓ ESLint baseline ratchet held: ${CURRENT} errors (${BASELINE_FILE#$REPO_ROOT/})${NC}"
exit 0
