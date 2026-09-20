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
# Ratchet semantics: the count of offenders may never RISE. It is NOT zero, and
# the reason is the most important thing in this file.
#
# The first version of this gate swept `set -o pipefail` into 21 scripts at
# once and baselined at 0. That sweep BROKE THE DEPLOY PATH. In a script that
# already runs `set -e`, pipefail turns an expected-empty pipeline into a fatal
# error, and `start-server.sh` has several: the airc-socket derivation returns
# nothing when no daemon is running (a FRESH INSTALL), and the lane-adopt probe
# fails when a pid dies mid-race. Both were followed by `if [ -n "$X" ]` — the
# empty case was already handled, and pipefail killed the script before it got
# there. Measured 2026-09-05; a fleet deploy was stopped minutes before it ran.
#
# A static pass then found ~28 more command substitutions containing pipes
# across the `set -e` scripts, each now a potential silent exit. Two patches
# would not have covered them.
#
# So pipefail has been REVERTED from every script that already sets `-e`, and
# this ratchet is baselined at the resulting real count. Those scripts are the
# debt. The flag goes back one script at a time, with each pipe's empty case
# actually EXERCISED — not parsed, not eyeballed — and the baseline drops by
# one. Ratcheting down deliberately is the whole point; a green 0 bought by a
# sweep was a lie that cost a deploy.
#
# The general lesson, paid for twice in one night: a mechanical sweep of a
# behaviour-changing flag is not hygiene. It is a behaviour change to every
# file it touches, and `bash -n` cannot see it because parsing is syntax and
# this is runtime.

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
#
# GENERATED output is excluded, and that exclusion is load-bearing rather than
# a convenience: `tools/scripts/generated/` is a mechanical projection of
# `install-manifest.toml`, guarded by its own drift detector. Hand-patching a
# file stamped "GENERATED FILE - DO NOT EDIT" is how you get a green hygiene
# gate and a red drift gate at the same time — measured, because the first
# revision of THIS ratchet did exactly that and the drift detector caught it.
# A generated script must inherit pipefail from its GENERATOR
# (tools/manifest-gen/src/main.rs); making it pass here by editing the output
# would be defeating the source of truth to satisfy a lint. Tracked as the
# follow-up on card aad30dee.
mapfile -t OFFENDERS < <(
  find "$REPO_ROOT/tools/scripts" "$REPO_ROOT/scripts" -name '*.sh' -type f 2>/dev/null \
    | grep -v '/generated/' \
    | sort \
    | while read -r f; do
        # The pipefail test is ANCHORED to the start of a line (after optional
        # indentation) so it matches a `set` STATEMENT and not a comment that
        # merely says "set -o pipefail". Unanchored, a file whose comments
        # discuss the flag reads as compliant with the flag absent — measured
        # 2026-09-05, when this ratchet's own mutation step caught it: the fix
        # for #3736 added explanatory comments containing the words `set -o
        # pipefail`, and start-server.sh with the real flag STRIPPED still
        # reported 0 offenders. A checker that cannot tell "the flag is set"
        # from "the word appears" is the exact defect this gate exists to stop.
        if grep -qE '\|[[:space:]]*(tail|head|grep|tee|awk|sed)' "$f" \
           && ! grep -qE '^[[:space:]]*set +-[a-zA-Z]*o[a-zA-Z]* +pipefail|^[[:space:]]*set +-o +pipefail' "$f"; then
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
