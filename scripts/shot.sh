#!/usr/bin/env bash
# shot.sh — headless screenshot of a running URL → PNG. The "never blind" feedback
# loop as one command ([[never-blind-feedback-driven-iteration]]).
#
# I captured the web app by hand-wiring CDP earlier this session. This is the
# leverage: build the factory with the product. Uses Chrome's built-in headless
# --screenshot (no CDP, no deps); --virtual-time-budget lets a JS/SPA app settle
# before the frame is taken, so it works on the real three-panel app, not a blank.
#
# Usage:
#   scripts/shot.sh                                   # shoots http://localhost:5173/
#   scripts/shot.sh 'http://localhost:5173/?me=<uuid>'
#   scripts/shot.sh <url> <out.png>
#   SHOT_SIZE=1920,1200 SHOT_BUDGET_MS=8000 scripts/shot.sh <url>
#
# Env: CHROME (binary path), SHOT_SIZE (WxH default 1600,1000),
#      SHOT_BUDGET_MS (SPA settle budget, default 6000).
set -euo pipefail

CHROME="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
URL="${1:-http://localhost:5173/}"
OUT="${2:-/tmp/continuum-shot.png}"
SIZE="${SHOT_SIZE:-1600,1000}"
BUDGET="${SHOT_BUDGET_MS:-6000}"

if [[ ! -x "$CHROME" ]]; then
  echo "shot: Chrome not found at '$CHROME' — set CHROME=/path/to/chrome." >&2
  exit 1
fi

# Fresh isolated profile each run so a stale session can't wedge the capture.
profile="$(mktemp -d)"
trap 'rm -rf "$profile"' EXIT

"$CHROME" --headless=new --disable-gpu --hide-scrollbars --no-first-run \
  --user-data-dir="$profile" --window-size="$SIZE" \
  --virtual-time-budget="$BUDGET" --screenshot="$OUT" "$URL" >/dev/null 2>&1 || true

if [[ -s "$OUT" ]]; then
  echo "shot: $URL → $OUT ($(du -h "$OUT" | cut -f1), size ${SIZE})"
else
  echo "shot: FAILED — no image written. Is '$URL' reachable/serving?" >&2
  exit 1
fi
