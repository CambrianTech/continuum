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
set -o pipefail  # a failing command in a pipeline must not read as success (card aad30dee)
LABEL="${1:-drill}"
OUT="${TMPDIR:-/tmp}/airc-blackout-drill-${LABEL}-$(date +%s).log"

# POSITIVE-CONTROL VERIFIED probe (2026-08-08, fourth iteration — the history
# IS the lesson, keep it):
#   1. `airc status` is not a stale read, it is an instrument that CHANGES what
#      it measures: run_status calls ensure_daemon_running first, which SPAWNS
#      a daemon if none answers (BigMama, from commands.rs:2183). Asking
#      creates the answer — status cannot report down, and running it during
#      an update window respawns the OLD binary mid-update. Never probe with it.
#   2. `airc peers` reads the address book: 13ms with NO daemon. Blind.
#   3. `timeout` does not exist on macOS; the test that blessed peers measured
#      exit-127, not the daemon.
# `airc ping` is the honest verb: run_ping goes straight to DaemonClient with
# no ensure/spawn. Two-sided control passed 2026-08-08: exit 0 on a live
# daemon, non-zero within ~2s on a stopped one. Never change this probe
# without re-running BOTH sides of that control.
probe() { airc ping >/dev/null 2>&1 && echo up || echo down; }

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
