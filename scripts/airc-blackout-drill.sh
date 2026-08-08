#!/usr/bin/env bash
# airc-blackout-drill — measure the daemon-unavailability WINDOW across `airc update`.
#
# The instrument for airc #1332 (update blackout): probes the daemon every 500ms
# while an update runs, records every transition, and reports total downtime.
# Run BEFORE and AFTER a fix lands — only a measured window is worth believing.
# (BigMama, 2026-08-08: "my claim is a shrunken window, and only a measured one
# is worth believing.")
#
# This measures AVAILABILITY, not delivery. The companion loss drill (sequenced
# messages through the window, gaps counted at a real peer) needs a second node
# and runs coordinated. Window first: it is the half a single node can falsify.
#
# Usage: scripts/airc-blackout-drill.sh [label]
set -u
LABEL="${1:-drill}"
OUT="${TMPDIR:-/tmp}/airc-blackout-drill-${LABEL}-$(date +%s).log"

# POSITIVE-CONTROL VERIFIED probe (2026-08-08, third attempt — the first two
# were blind and the story is the lesson):
#   1. `airc status` reads local state: UP a second after `airc stop`. Blind.
#   2. `airc peers` reads the address book: 13ms with NO daemon. Blind.
#      (And `timeout` does not exist on macOS, so the "sensitivity test" that
#      blessed it was measuring exit-127, not the daemon.)
# NO airc CLI read verb is a liveness probe. The only honest probe is the
# daemon's own unix socket: connect fails when the daemon is down, verified
# two-sided against a real stop AND a real running daemon. Never change this
# probe without re-running BOTH sides of that control.
probe() {
  SOCK=$(find "$HOME/.airc/runtime" -name '*.sock' 2>/dev/null | head -1)
  [ -z "$SOCK" ] && { echo down; return; }
  python3 - "$SOCK" <<'PYEOF'
import socket, sys
s = socket.socket(socket.AF_UNIX)
s.settimeout(2)
try:
    s.connect(sys.argv[1])
    print("up")
except Exception:
    print("down")
PYEOF
}

echo "drill '$LABEL' starting $(date -u +%FT%TZ) — probing every 500ms, log: $OUT"
STATE=$(probe)
echo "$(date +%s.%N) $STATE (initial)" >> "$OUT"

# Probe loop in the background for the whole drill.
(
  PREV=$STATE
  while [ ! -f "$OUT.stop" ]; do
    CUR=$(probe)
    if [ "$CUR" != "$PREV" ]; then
      echo "$(date +%s.%N) $CUR" >> "$OUT"
      PREV=$CUR
    fi
    sleep 0.5
  done
) &
PROBE_PID=$!

T0=$(date +%s)
echo "--- airc update starting ---"
airc update 2>&1 | tail -5
T1=$(date +%s)
echo "--- airc update finished (${T1}-${T0}=$((T1 - T0))s wall) ---"

# Let the daemon settle, then stop probing.
sleep 5
touch "$OUT.stop"
wait "$PROBE_PID" 2>/dev/null
rm -f "$OUT.stop"

python3 - "$OUT" "$((T1 - T0))" <<'EOF'
import sys
lines = [l.split() for l in open(sys.argv[1]) if l.strip()]
wall = sys.argv[2]
down_total = 0.0
down_since = None
transitions = 0
for parts in lines:
    t, state = float(parts[0]), parts[1]
    if state == "down" and down_since is None:
        down_since = t; transitions += 1
    elif state == "up" and down_since is not None:
        down_total += t - down_since; down_since = None
print(f"RESULT: update wall={wall}s, daemon DOWN total={down_total:.1f}s across {transitions} outage(s)")
print("(0.0s across 0 outages == the no-op update was free — the #1332 target)")
EOF
