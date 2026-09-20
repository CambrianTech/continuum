#!/bin/bash
# check-ts-persona-forbidden-strings.sh — Lane F PR-2 ratchet (PR #1091 followup).
#
# Per-pattern monotonic-decrease ratchet for anti-patterns in the TS
# persona surface (src/system/user/server/). Mirrors PR #1091's LOC
# ratchet shape but counts grep matches per regex instead of total
# lines.
#
# Per Joel's no-fallbacks rule + the Rust-first alpha contract (PR #1070,
# ALPHA-GAP-ANALYSIS.md): the TS surface must shed cloud-key env reads,
# direct adapter instantiation, and the WORD `fallback` over time. The
# Rust provider registry + resolver own these concerns (#1066, #1074,
# #1077, #1089).
#
# Modes:
#   ./check-ts-persona-forbidden-strings.sh              # check + report; exit 0/1
#   ./check-ts-persona-forbidden-strings.sh --update-baseline   # update + commit-ready
#   ./check-ts-persona-forbidden-strings.sh --verbose     # print per-pattern occurrences

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BASELINE_FILE="$SCRIPT_DIR/ts-persona-forbidden-strings-baseline.json"
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
      echo "  Default: check current per-pattern counts against baseline; exit non-zero on any growth."
      echo "  --update-baseline: rewrite baseline_count for each pattern to current (use after legitimate removal)."
      echo "  --verbose: print first 5 occurrences per pattern."
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
  exit 2
fi

# Count occurrences of one pattern across the surface (excluding tests).
count_pattern() {
  local regex="$1"
  local case_insensitive="$2"
  local grep_flags="-rEoI --include=*.ts --exclude=*.test.ts --exclude=*.spec.ts"
  if [[ "$case_insensitive" == "true" ]]; then
    grep_flags="$grep_flags -i"
  fi
  # `|| true` — grep returns 1 on zero matches, which is a valid count.
  grep $grep_flags "$regex" "$SURFACE_DIR" 2>/dev/null | wc -l | tr -d ' ' || true
}

# Read pattern config from JSON in shell-friendly tabular form.
PATTERN_DATA=$(python3 - "$BASELINE_FILE" <<'PYEOF'
import json, sys
with open(sys.argv[1]) as f:
    data = json.load(f)
for p in data["patterns"]:
    print("\t".join([
        p["id"],
        p["regex"],
        "true" if p.get("case_insensitive", False) else "false",
        str(p["baseline_count"]),
    ]))
PYEOF
)

ANY_GROWTH=0
RESULTS=()
while IFS=$'\t' read -r id regex ci baseline; do
  current=$(count_pattern "$regex" "$ci")
  delta=$((current - baseline))
  RESULTS+=("$id|$baseline|$current|$delta")
  if [[ "$delta" -gt 0 ]]; then
    ANY_GROWTH=1
  fi
done <<< "$PATTERN_DATA"

if [[ "$VERBOSE" -eq 1 ]]; then
  echo -e "${YELLOW}━━ TS persona-forbidden-strings (per-pattern occurrences, top 5) ━━${NC}"
  while IFS=$'\t' read -r id regex ci baseline; do
    echo -e "${YELLOW}# $id  baseline=$baseline${NC}"
    grep_flags="-rEnI --include=*.ts --exclude=*.test.ts --exclude=*.spec.ts"
    if [[ "$ci" == "true" ]]; then grep_flags="$grep_flags -i"; fi
    grep $grep_flags "$regex" "$SURFACE_DIR" 2>/dev/null | head -5 || echo "  (no matches)"
    echo ""
  done <<< "$PATTERN_DATA"
fi

if [[ "$UPDATE_BASELINE" -eq 1 ]]; then
  CURRENT_SHA=$(git -C "$REPO_ROOT" rev-parse --short HEAD 2>/dev/null || echo "unknown")
  CURRENT_ISO=$(date -u +"%Y-%m-%dT%H:%MZ")
  python3 - "$BASELINE_FILE" "$CURRENT_SHA" "$CURRENT_ISO" "${RESULTS[@]}" <<'PYEOF'
import json, sys
path, sha, iso = sys.argv[1], sys.argv[2], sys.argv[3]
results = {}
for entry in sys.argv[4:]:
    pid, baseline, current, delta = entry.split("|")
    results[pid] = int(current)
with open(path) as f:
    data = json.load(f)
for p in data["patterns"]:
    if p["id"] in results:
        p["baseline_count"] = results[p["id"]]
data["_baseline_anchored_at_canary"] = sha
data["_anchored_at_iso"] = iso
with open(path, "w") as f:
    json.dump(data, f, indent=2)
    f.write("\n")
PYEOF
  echo -e "${GREEN}✓ baseline updated to current counts:${NC}"
  for r in "${RESULTS[@]}"; do
    IFS='|' read -r id baseline current delta <<< "$r"
    echo "  $id: $baseline → $current (delta $delta)"
  done
  echo "  Commit: git add $BASELINE_FILE"
  exit 0
fi

if [[ "$ANY_GROWTH" -eq 1 ]]; then
  echo -e "${RED}━━ ❌ TS persona-forbidden-strings RATCHET FAILED ━━${NC}" >&2
  echo "" >&2
  for r in "${RESULTS[@]}"; do
    IFS='|' read -r id baseline current delta <<< "$r"
    if [[ "$delta" -gt 0 ]]; then
      echo -e "${RED}  ❌ $id: baseline=$baseline current=$current delta=+$delta${NC}" >&2
    elif [[ "$delta" -lt 0 ]]; then
      echo -e "${GREEN}  ✓ $id: baseline=$baseline current=$current delta=$delta (shrunk)${NC}" >&2
    else
      echo -e "${YELLOW}  · $id: baseline=$baseline current=$current (held)${NC}" >&2
    fi
  done
  echo "" >&2
  echo "  Per Joel's no-fallbacks rule + Rust-first alpha contract (PR #1070)," >&2
  echo "  the TS persona surface must shed these patterns over time. Provider" >&2
  echo "  resolution + admission belong in Rust (core/continuum-core/src/cognition/," >&2
  echo "  core/continuum-core/src/persona/), NOT in TS." >&2
  echo "" >&2
  echo "  Options:" >&2
  echo "    1. Move the pattern occurrence Rust-side." >&2
  echo "    2. Refactor it out (rename, restructure) so the TS surface stops mentioning it." >&2
  echo "    3. If your PR also REMOVES occurrences elsewhere AND net is flat-or-down for" >&2
  echo "       this pattern, the ratchet should already be passing for that pattern. Run" >&2
  echo "       this script with --verbose to see what's left." >&2
  exit 1
fi

echo -e "${GREEN}✓ TS persona-forbidden-strings ratchet held:${NC}"
for r in "${RESULTS[@]}"; do
  IFS='|' read -r id baseline current delta <<< "$r"
  if [[ "$delta" -lt 0 ]]; then
    echo -e "${GREEN}  ✓ $id: baseline=$baseline current=$current delta=$delta (shrunk — run --update-baseline post-merge to lock in)${NC}"
  else
    echo "  · $id: baseline=$baseline current=$current"
  fi
done
exit 0
