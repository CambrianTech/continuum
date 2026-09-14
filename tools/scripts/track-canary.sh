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
  if ! mkdir "$LOCK" 2>/dev/null; then say "skip: another pass holds $LOCK"; return 0; fi
  trap 'rmdir "$LOCK" 2>/dev/null' RETURN
  local repo tip run verdict
  repo="$(gh_repo)"; [ -z "$repo" ] && { say "skip: no origin remote"; return 0; }
  tip="$(tip_sha)" || { say "skip: fetch failed (offline?) — the running build stands"; return 0; }
  run="$(running_sha)"
  if [ -z "$run" ]; then say "skip: core not answering — not a deploy trigger (start it, then track)"; return 0; fi
  if [ "${tip:0:9}" = "${run:0:9}" ]; then return 0; fi
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
  if continuum reboot >>"$LOG" 2>&1; then
    say "deployed: $(running_sha) in $(( $(date +%s) - started )) s"
  else
    say "FAILED: continuum reboot exited non-zero after $(( $(date +%s) - started )) s — read the log above; the tracker will retry next pass"
  fi
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
  <key>ProgramArguments</key><array><string>/bin/bash</string><string>$SCRIPT_DIR/track-canary.sh</string><string>--once</string></array>
  <key>StartInterval</key><integer>$INTERVAL</integer>
  <key>RunAtLoad</key><true/>
  <key>EnvironmentVariables</key><dict><key>PATH</key><string>$HOME/.local/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin</string></dict>
  <key>StandardOutPath</key><string>$LOG</string>
  <key>StandardErrorPath</key><string>$LOG</string>
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
ExecStart=/bin/bash $SCRIPT_DIR/track-canary.sh --once
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
