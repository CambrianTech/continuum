#!/bin/bash
# check-pipefail.sh — every shell script that pipes must set `pipefail`.
#
# WHY THIS EXISTS (card aad30dee, 2026-09-05). A build was reported green by
# its own harness while cargo had failed, because the command was
#
#     cargo build ... 2>&1 | tail -12
#
# and the exit code belonged to `tail`, not to cargo. The same shape was then
# found in the shared validation gate: `git-prepush.sh` counted ESLint errors
# by grepping the linter's merged output, so a linter that CRASHED produced
# zero matches, compared `0 -le baseline`, and printed a green checkmark. A
# broken check read as a passing check.
#
# `set -e` does NOT cover this. Without `pipefail`, a pipeline's status is the
# status of its LAST command, so `set -e` sees success and the script looks
# defended while silently swallowing the failure of the command that mattered.
# Thirteen scripts in tools/scripts were in that state, including the deploy
# path and both git hooks.
#
# SCOPE, deliberately: this ratchet checks for `pipefail` ONLY. It does not
# require `-u` (nounset). `start-server.sh` alone carries 193 bare variable
# references against 11 guarded ones, and that script must run on every
# supported host — turning nounset on there is a behaviour change to the
# deploy path, not a hygiene fix, and its failure mode is an install that dies
# instantly on a machine nobody can reproduce. If nounset is wanted it is a
# per-file audit with its own ratchet.
#
# Ratchet semantics: the count of offenders may never RISE. It is currently
# zero and should stay there; the baseline exists so a legitimate exception
# can be recorded deliberately rather than by accident.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BASELINE_FILE="$SCRIPT_DIR/pipefail-baseline.txt"

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

UPDATE_BASELINE=0
[[ "${1:-}" == "--update-baseline" ]] && UPDATE_BASELINE=1

# A script is an OFFENDER when it pipes into a command whose exit status the
# author plainly cares about, and does not set pipefail. Restricted to the
# consumers that actually mask a failure (tail/head/grep/tee/awk/sed) rather
# than every `|` in the tree, so the signal stays about swallowed exit codes.
mapfile -t OFFENDERS < <(
  find "$REPO_ROOT/tools/scripts" "$REPO_ROOT/scripts" -name '*.sh' -type f 2>/dev/null \
    | sort \
    | while read -r f; do
        if grep -qE '\|[[:space:]]*(tail|head|grep|tee|awk|sed)' "$f" \
           && ! grep -qE 'set -[a-zA-Z]*o[a-zA-Z]* +pipefail|set -o +pipefail' "$f"; then
          printf '%s\n' "${f#"$REPO_ROOT"/}"
        fi
      done
)

COUNT=${#OFFENDERS[@]}

if [[ "$UPDATE_BASELINE" == "1" ]]; then
  printf '%s\n' "$COUNT" > "$BASELINE_FILE"
  echo -e "${YELLOW}pipefail baseline updated to $COUNT${NC}"
  exit 0
fi

if [[ ! -f "$BASELINE_FILE" ]]; then
  echo -e "${RED}❌ pipefail baseline missing at $BASELINE_FILE${NC}"
  echo "   Create it once: bash scripts/ratchets/check-pipefail.sh --update-baseline"
  exit 1
fi

BASELINE=$(tr -d '[:space:]' < "$BASELINE_FILE")

if (( COUNT > BASELINE )); then
  echo -e "${RED}❌ pipefail ratchet: $COUNT script(s) pipe without pipefail (baseline $BASELINE).${NC}"
  echo "   These can report a FAILING command as success — the defect this gate exists to stop:"
  printf '     %s\n' "${OFFENDERS[@]}"
  echo ""
  echo "   Fix: add 'set -o pipefail' near the top (alongside any existing 'set -e')."
  echo "   Do NOT add -u mechanically; see the header of this script for why."
  exit 1
fi

if (( COUNT < BASELINE )); then
  echo -e "${YELLOW}✅ pipefail: $COUNT offender(s), below baseline $BASELINE — lock the win:${NC}"
  echo "   bash scripts/ratchets/check-pipefail.sh --update-baseline"
  exit 0
fi

echo -e "${GREEN}✅ pipefail: $COUNT offender(s), at baseline ($BASELINE).${NC}"
