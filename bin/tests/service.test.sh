#!/usr/bin/env bash
# service.test.sh — validates `continuum service` generates a correct, valid,
# logout/reboot-surviving boot-service definition for EACH platform, WITHOUT
# installing anything live.
#
# What this catches: the node-resilience fix (a node tied to a login session
# dies on logoff — BIGMAMA did). Each platform must produce a SYSTEM-scope
# service (LaunchDaemon not LaunchAgent; systemd multi-user not a user unit;
# Windows AtStartup as SYSTEM) — a regression that emitted a user-scope service
# would silently reintroduce the logout-death bug.
#
# Each OS branch is driven by shimming `uname` on PATH; `--dry-run` means
# nothing is written to /Library/LaunchDaemons or /etc/systemd and no sudo runs.
#
#   bash bin/tests/service.test.sh

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BRAIN="$(dirname "$SCRIPT_DIR")/continuum"

PASS=0; FAIL=0
ok(){ echo "  ✓ $1"; PASS=$((PASS+1)); }
no(){ echo "  ✗ $1"; FAIL=$((FAIL+1)); }
has(){ case "$2" in *"$1"*) ok "$3";; *) no "$3 — missing: $1";; esac; }
lacks(){ case "$2" in *"$1"*) no "$3 — should NOT contain: $1";; *) ok "$3";; esac; }

SHIM="$(mktemp -d "${TMPDIR:-/tmp}/continuum-svc-test.XXXXXX")"
trap 'rm -rf "$SHIM"' EXIT
make_uname(){ printf '#!/bin/sh\necho "%s"\n' "$1" > "$SHIM/uname"; chmod +x "$SHIM/uname"; }
run_service(){ PATH="$SHIM:$PATH" bash "$BRAIN" service "$@" 2>&1; }

echo "macOS — LaunchDaemon (system scope, survives logout):"
make_uname Darwin
OUT="$(run_service install --dry-run)"
has "/Library/LaunchDaemons/homes.continuum.node.plist" "$OUT" "targets system LaunchDaemons (NOT a per-user LaunchAgent)"
has "<key>RunAtLoad</key><true/>" "$OUT" "RunAtLoad (starts at boot)"
has "<key>KeepAlive</key><true/>" "$OUT" "KeepAlive (restarts if it dies)"
has "<string>start</string>" "$OUT" "runs the node start"
lacks "<string>--headless</string>" "$OUT" "default pins NO mode — honors CONTINUUM_LAUNCH_MODE"
if command -v plutil >/dev/null 2>&1; then
  printf '%s\n' "$OUT" | sed -n '/<?xml/,/<\/plist>/p' > "$SHIM/test.plist"
  plutil -lint "$SHIM/test.plist" >/dev/null 2>&1 && ok "generated plist passes plutil -lint" || no "plist failed plutil -lint"
fi

echo "Linux — systemd system unit (survives logout + reboot):"
make_uname Linux
OUT="$(run_service install --dry-run)"
has "/etc/systemd/system/continuum-node.service" "$OUT" "targets the SYSTEM unit dir"
has "WantedBy=multi-user.target" "$OUT" "boots at multi-user (independent of any login)"
has "Restart=always" "$OUT" "auto-restarts"
has "ExecStart=" "$OUT" "has an ExecStart"
lacks "start --headless" "$OUT" "default ExecStart pins NO mode — honors CONTINUUM_LAUNCH_MODE"
has "systemctl enable docker" "$OUT" "enables the docker engine at boot too"

echo "Windows — startup task as SYSTEM (validate on BIGMAMA):"
make_uname MINGW64_NT-10.0
OUT="$(run_service install --dry-run)"
has "Register-ScheduledTask" "$OUT" "prints the startup-task recipe"
has "AtStartup" "$OUT" "runs at startup, before any login"
lacks "start --headless" "$OUT" "default pins NO mode — honors CONTINUUM_LAUNCH_MODE"

echo "override — 'service install --headless' pins core-only regardless of the setting:"
make_uname Darwin
OUT="$(run_service install --headless --dry-run)"
has "<string>--headless</string>" "$OUT" "macOS: --headless param baked into the service"
make_uname Linux
OUT="$(run_service install --headless --dry-run)"
has "start --headless" "$OUT" "Linux: --headless param baked into ExecStart"
make_uname MINGW64_NT-10.0
OUT="$(run_service install --headless --dry-run)"
has "start --headless" "$OUT" "Windows: --headless param baked into the task"

echo "status (unprivileged) + bad action:"
make_uname Darwin
run_service status >/dev/null 2>&1 && ok "status exits clean with nothing installed" || no "status errored"
make_uname Darwin
if run_service bogus >/dev/null 2>&1; then no "unknown action should exit non-zero"; else ok "unknown action exits non-zero"; fi

echo ""
echo "── $PASS passed, $FAIL failed ──"
[ "$FAIL" -eq 0 ]
