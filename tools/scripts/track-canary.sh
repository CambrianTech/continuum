#!/usr/bin/env bash
# track-canary.sh — this node follows the canary tip on cadence. No hands.
#
# Law (Joel, 2026-09-05): NEVER HOLD A DEPLOY — a stale binary is the failure. Until
# 2026-09-13 the tracking was an operator's shell chains (merge-on-green watchers,
# deploy-after scripts), which is exactly the "broken system we keep waiting on".
# This script is the owner: every INTERVAL seconds it fetches the tracked branch;
# when the tip differs from the running core's build SHA AND the tip's checks are
# green, it deploys once (`continuum reboot`, whose warm build keeps the core
# serving through the compile) and records a receipt. One deploy at a time, never
# while a build is running, never on a red tip, never while a benchmark round asks
# for quiet (a `hold` file). Everything it decides is in the log.
#
#   track-canary.sh --once          one pass (what cron/launchd/systemd should call)
#   track-canary.sh --loop          run forever with INTERVAL (default 300 s)
#   track-canary.sh --install       install a launchd agent (macOS) or a systemd
#                                   user timer (Linux) calling --once every INTERVAL
#   track-canary.sh --status        print the tracked branch, tip, running SHA, verdict
#
# Env: CONTINUUM_TRACK_BRANCH (default canary), CONTINUUM_TRACK_INTERVAL (300),
#      CONTINUUM_TRACK_HOLD (a file whose presence pauses deploys; default
#      ~/.continuum/state/deploy-hold). The repo is the checkout this script lives in;
#      the GitHub repo is derived from `git remote get-url origin` — nothing is
#      hardcoded (this repo is for other people and their orgs).
set -u
set -o pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
BRANCH="${CONTINUUM_TRACK_BRANCH:-canary}"
INTERVAL="${CONTINUUM_TRACK_INTERVAL:-300}"
HOLD="${CONTINUUM_TRACK_HOLD:-$HOME/.continuum/state/deploy-hold}"
LOG="$HOME/.continuum/logs/track-canary.log"
LOCK="$HOME/.continuum/state/track-canary.lock"
mkdir -p "$(dirname "$LOG")" "$(dirname "$LOCK")"
export PATH="$HOME/.local/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"

say() { printf '%s %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$*" | tee -a "$LOG"; }

gh_repo() {
  git -C "$REPO_DIR" remote get-url origin 2>/dev/null \
    | sed -E 's#(git@github.com:|https://github.com/)##; s#\.git$##'
}

running_sha() {
  continuum ping 2>/dev/null </dev/null | grep -oE '"buildSha": "[0-9a-f]+' | cut -d'"' -f4
}

tip_sha() {
  git -C "$REPO_DIR" fetch -q origin "$BRANCH" 2>/dev/null || return 1
  git -C "$REPO_DIR" rev-parse "origin/$BRANCH"
}

# The tip's check verdict: green | red | pending | unknown. Required contexts are
# whatever the branch protection names; absent protection, every check counts.
tip_checks() {
  local repo="$1" sha="$2" json
  json="$(gh api "repos/$repo/commits/$sha/check-runs?per_page=100" 2>/dev/null)" || { echo unknown; return; }
  python3 - "$json" <<'PY'
import json, sys
d = json.loads(sys.argv[1]); runs = d.get("check_runs", [])
if not runs: print("unknown"); sys.exit()
concl = [r.get("conclusion") for r in runs]
if any(c in ("failure", "timed_out", "cancelled", "action_required") for c in concl): print("red")
elif any(r.get("status") != "completed" for r in runs): print("pending")
else: print("green")
PY
}

build_in_flight() {
  pgrep -f "continuum reboot" >/dev/null 2>&1 || pgrep -f "cargo build --manifest-path $REPO_DIR" >/dev/null 2>&1
}

status() {
  local repo tip run
  repo="$(gh_repo)"; tip="$(tip_sha || echo '?')"; run="$(running_sha || echo '?')"
  echo "repo=$repo branch=$BRANCH tip=${tip:0:9} running=${run:-none} checks=$(tip_checks "$repo" "$tip") hold=$([ -e "$HOLD" ] && echo yes || echo no) building=$(build_in_flight && echo yes || echo no)"
}

once() {
  # A PID-checked lock: a pass killed mid-deploy (2026-09-14 06:44 → 08:53: ten passes skipped
  # on a lock whose holder was gone) must never hold the tracker forever.
  if [ -d "$LOCK" ]; then
    local holder; holder="$(cat "$LOCK/pid" 2>/dev/null)"
    if [ -n "$holder" ] && kill -0 "$holder" 2>/dev/null; then say "skip: pass $holder still running"; return 0; fi
    say "lock: holder ${holder:-?} is gone — clearing the stale lock"; rm -rf "$LOCK"
  fi
  mkdir "$LOCK" 2>/dev/null || { say "skip: could not take $LOCK"; return 0; }
  echo $$ > "$LOCK/pid"
  trap 'rm -rf "$LOCK" 2>/dev/null' RETURN
  local repo tip run verdict
  repo="$(gh_repo)"; [ -z "$repo" ] && { say "skip: no origin remote"; return 0; }
  tip="$(tip_sha)" || { say "skip: fetch failed (offline?) — the running build stands"; return 0; }
  run="$(running_sha)"
  if [ -z "$run" ]; then say "skip: core not answering — not a deploy trigger (start it, then track)"; return 0; fi
  # A silent watcher reads as a dead one (airc law: silent = down). One line per pass,
  # even when there is nothing to do — the receipt that the agent is alive.
  if [ "${tip:0:9}" = "${run:0:9}" ]; then say "ok: running ${run:0:9} == tip"; return 0; fi
  if [ -e "$HOLD" ]; then say "hold: tip ${tip:0:9} ≠ running ${run:0:9} but $HOLD is present ($(cat "$HOLD" 2>/dev/null | head -c 80))"; return 0; fi
  if build_in_flight; then say "wait: a build/reboot is already in flight"; return 0; fi
  verdict="$(tip_checks "$repo" "$tip")"
  case "$verdict" in
    green) ;;
    pending) say "wait: tip ${tip:0:9} checks pending"; return 0 ;;
    red)   say "refuse: tip ${tip:0:9} is RED — a red tip is never deployed"; return 0 ;;
    *)     say "wait: tip ${tip:0:9} checks unknown (gh unreachable or no runs yet)"; return 0 ;;
  esac
  say "deploy: running ${run:0:9} → tip ${tip:0:9} (checks green)"
  if ! git -C "$REPO_DIR" diff --quiet || ! git -C "$REPO_DIR" diff --cached --quiet; then
    say "refuse: the deploy tree has uncommitted changes — the deploy tree is the deploy tree"; return 0
  fi
  ( cd "$REPO_DIR" && git checkout -q "$BRANCH" && git pull -q --ff-only origin "$BRANCH" && git submodule update --init --recursive -q ) \
    || { say "refuse: checkout/pull of $BRANCH failed"; return 0; }
  local started; started="$(date +%s)"
  # FROM THE REPO. Under launchd the cwd is `/`; `continuum reboot` then finds no source
  # tree, skips the build, and bounces the installed artifact — the first self-deploy
  # (2026-09-14 03:32Z) "deployed" the old binary and left the core down on a stale airc
  # socket. The deploy tree is where a deploy runs.
  cd "$REPO_DIR" || { say "refuse: cannot cd $REPO_DIR"; return 0; }
  if continuum reboot >>"$LOG" 2>&1; then
    say "deployed: $(running_sha) in $(( $(date +%s) - started )) s"
    # The core must ANSWER before anything else is believed (2026-09-14 06:53 → 08:58:
    # a core launched by the tracker held its socket for two hours and never answered
    # ping; the persona stores failed to open under the agent's environment). One
    # supervised restart from THIS shell, then the self-check decides.
    local waited=0
    while [ "$waited" -lt 600 ] && [ -z "$(running_sha)" ]; do sleep 30; waited=$((waited+30)); done
    if [ -z "$(running_sha)" ]; then
      say "core silent ${waited}s after the deploy — one supervised restart (continuum start)"
      continuum start >>"$LOG" 2>&1 </dev/null || true
      waited=0; while [ "$waited" -lt 300 ] && [ -z "$(running_sha)" ]; do sleep 30; waited=$((waited+30)); done
    fi
    self_check "$tip"
  else
    say "FAILED: continuum reboot exited non-zero after $(( $(date +%s) - started )) s — read the log above; the tracker will retry next pass"
  fi
}

# POST-DEPLOY SELF-CHECK — catch it now, not an hour later (2026-09-14: a regressed
# server pin, a boot that drained the roster, and cold KV restores each sat unseen for
# 40–60 minutes while the operator read probes by hand). Three receipts, each with a
# named outcome; any failure writes the HOLD file so the tracker stops chasing a broken
# tip until a human reads the log, and posts one line to the project room.
#   1. the running build is the tip (deploy provenance)
#   2. the roster fills: residents ≥ min(seats, 1) within RESIDENCY_WAIT seconds
#   3. the KV restore economy is warm: `serving/cache-probe --roundtrip` says reuses
self_check() {
  local tip="$1" run ok=1 residents verdict
  run="$(running_sha)"
  if [ "${run:0:9}" != "${tip:0:9}" ]; then say "SELF-CHECK FAIL: running ${run:0:9} ≠ deployed tip ${tip:0:9}"; ok=0; fi
  local waited=0
  while [ "$waited" -lt "${RESIDENCY_WAIT:-900}" ]; do
    residents="$(continuum persona/list 2>/dev/null </dev/null | python3 -c 'import json,sys; d=json.load(sys.stdin); print(sum(1 for x in d.get("citizens",[]) if x.get("resident")))' 2>/dev/null || echo 0)"
    [ "${residents:-0}" -ge 1 ] && break
    sleep 30; waited=$((waited+30))
  done
  if [ "${residents:-0}" -lt 1 ]; then say "SELF-CHECK FAIL: no resident citizen ${RESIDENCY_WAIT:-900}s after the deploy (the empty-node class)"; ok=0; else say "self-check: $residents resident after ${waited}s"; fi
  verdict="$(continuum serving/cache-probe --roundtrip 2>/dev/null </dev/null | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("restore_verdict") or d.get("verdict") or "unknown")' 2>/dev/null || echo unknown)"
  case "$verdict" in
    reuses) say "self-check: KV restore economy warm (cache-probe: reuses)" ;;
    *) say "SELF-CHECK FAIL: cache-probe restore verdict '$verdict' — a restored slot is cold (the 09-04 / 09-14 class)"; ok=0 ;;
  esac
  if [ "$ok" = "1" ]; then
    say "self-check PASSED on ${tip:0:9}"
    continuum chat/send --roomId "$(project_room)" --text "deploy self-check PASSED on ${tip:0:9}: build verified, $residents resident, KV restores warm" >/dev/null 2>&1 </dev/null || true
  else
    printf 'self-check failed on %s at %s — read %s, fix, then remove this file
' "${tip:0:9}" "$(date -u +%H:%M:%SZ)" "$LOG" > "$HOLD"
    say "HOLD written ($HOLD): the tracker will not chase the next tip until a human clears it"
    continuum chat/send --roomId "$(project_room)" --text "deploy SELF-CHECK FAILED on ${tip:0:9} — tracker on hold; see $LOG" >/dev/null 2>&1 </dev/null || true
  fi
}

# The project room to post receipts into: the room bound to this checkout's origin
# (never a hardcoded id). Empty = no post.
project_room() {
  continuum room/list 2>/dev/null </dev/null | python3 -c 'import json,sys
d=json.load(sys.stdin); rooms=d.get("rooms") or d.get("items") or []
for r in rooms:
    if (r.get("purpose") or r.get("recipe") or "") == "project": print(r.get("id") or r.get("roomId") or ""); break' 2>/dev/null
}

install_agent() {
  case "$(uname -s)" in
    Darwin)
      local plist="$HOME/Library/LaunchAgents/com.continuum.track-canary.plist"
      cat >"$plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>com.continuum.track-canary</string>
  <key>ProgramArguments</key><array><string>/bin/zsh</string><string>-lc</string><string>exec bash '$SCRIPT_DIR/track-canary.sh' --once</string></array>
  <key>StartInterval</key><integer>$INTERVAL</integer>
  <key>RunAtLoad</key><true/>
  <key>EnvironmentVariables</key><dict><key>PATH</key><string>$HOME/.local/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin</string></dict>
  <key>StandardOutPath</key><string>$LOG.launchd</string>
  <key>StandardErrorPath</key><string>$LOG.launchd</string>
</dict></plist>
EOF
      launchctl unload "$plist" 2>/dev/null; launchctl load "$plist" && say "installed launchd agent com.continuum.track-canary every ${INTERVAL}s ($plist)"
      ;;
    Linux)
      local dir="$HOME/.config/systemd/user"; mkdir -p "$dir"
      cat >"$dir/continuum-track-canary.service" <<EOF
[Unit]
Description=Continuum: this node follows the canary tip
[Service]
Type=oneshot
ExecStart=/bin/bash -lc "exec bash '$SCRIPT_DIR/track-canary.sh' --once"
EOF
      cat >"$dir/continuum-track-canary.timer" <<EOF
[Unit]
Description=Continuum: track canary every ${INTERVAL}s
[Timer]
OnBootSec=120
OnUnitActiveSec=${INTERVAL}
[Install]
WantedBy=timers.target
EOF
      systemctl --user daemon-reload && systemctl --user enable --now continuum-track-canary.timer && say "installed systemd user timer continuum-track-canary every ${INTERVAL}s"
      ;;
    *) say "install: unsupported platform $(uname -s) — run '--loop' under your supervisor"; return 1 ;;
  esac
}

case "${1:---once}" in
  --once) once ;;
  --loop) while true; do once; sleep "$INTERVAL"; done ;;
  --install) install_agent ;;
  --status) status ;;
  *) echo "usage: $0 [--once|--loop|--install|--status]" >&2; exit 2 ;;
esac
