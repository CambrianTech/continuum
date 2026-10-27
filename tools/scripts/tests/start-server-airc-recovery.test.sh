#!/usr/bin/env bash
# What this catches (review of continuum #4675): boot treated any failed `airc ping`
# (denied, timed out, protocol mismatch) as a wedged daemon, globbed the newest
# socket, and `kill -9`ed every lsof holder, clients included. Recovery is airc's:
# a failed ping must never signal a process, the start is airc's gated autostart,
# and airc's refusal reaches the operator instead of being swallowed.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
scratch="$(mktemp -d "${TMPDIR:-/tmp}/continuum-airc-recovery.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }

# The functions under test, from the real launcher.
# machine_daemon_pids existed before #4675; extracting it when present lets this fixture run
# against an older launcher and fail there, on behavior.
for fn in machine_airc machine_airc_capture machine_daemon_pids ensure_airc_daemon; do
  eval "$(sed -n "/^${fn}()/,/^}/p" "$script_dir/../start-server.sh")"
done
declare -F ensure_airc_daemon >/dev/null || fail "ensure_airc_daemon not found in start-server.sh"

export HOME="$scratch/home" TRACE="$scratch/trace" STATE="$scratch/state"
mkdir -p "$HOME/.airc/runtime" "$scratch/bin"
# A machine socket exists, as on a real box: the old reap globbed it and lsof'd it.
: > "$HOME/.airc/runtime/airc-machine-fixture-v5.sock"
cat > "$scratch/bin/airc" <<'AIRC'
#!/usr/bin/env bash
echo "airc $*" >> "$TRACE"
case "$1" in
  ping) [ -f "$STATE/up" ] ;;
  events) if [ -f "$STATE/refuse" ]; then echo "daemon intentionally stopped by the operator" >&2; exit 1; fi
          touch "$STATE/up" ;;
  *) exit 0 ;;
esac
AIRC
chmod +x "$scratch/bin/airc"
export PATH="$scratch/bin:$PATH"
bounded_run() { shift; "$@"; }
bounded_capture() { shift; "$@"; }
kill() { echo "kill $*" >> "$TRACE"; }
pkill() { echo "pkill $*" >> "$TRACE"; }
lsof() { echo "lsof $*" >> "$TRACE"; }

# A: nothing answers; airc's ensure brings its daemon up.
mkdir -p "$STATE"; : > "$TRACE"
ensure_airc_daemon > "$scratch/out" 2>&1 || fail "a recoverable start failed: $(cat "$scratch/out")"
grep -q '^airc events list --limit 0 --json$' "$TRACE" || fail "boot did not ask airc's own autostart"
! grep -qE '^(kill|pkill|lsof) ' "$TRACE" || fail "a failed ping signalled or probed processes: $(grep -E '^(kill|pkill|lsof)' "$TRACE")"
! grep -q '^airc stop' "$TRACE" || fail "boot ran the public airc stop"
! grep -q '^airc daemon' "$TRACE" || fail "boot ran a raw airc daemon"

# B: airc refuses (an operator stopped it); boot fails with airc's words, kills nothing.
rm -rf "$STATE"; mkdir -p "$STATE"; touch "$STATE/refuse"; : > "$TRACE"
if ensure_airc_daemon > "$scratch/out" 2>&1; then fail "boot reported success while airc refused"; fi
grep -q 'intentionally stopped by the operator' "$scratch/out" || fail "airc's refusal was swallowed: $(cat "$scratch/out")"
! grep -qE '^(kill|pkill|lsof) ' "$TRACE" || fail "a refused start signalled processes"

echo "✓ boot recovers airc only through airc, never signals a process, and shows airc's refusal"
