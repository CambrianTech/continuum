#!/usr/bin/env bash
# install-service.sh — install the Continuum core as a REBOOT-SURVIVING OS service.
#
# Reliability spine #2 (upstart): a grid node must come back on its own after a
# reboot, a crash, OR a user logout — never "the persona died because someone
# logged off the machine" (the literal BIGMAMA failure, [[grid-node-resilience]]).
#
# Two scopes:
#   user (default)   macOS LaunchAgent (~/Library/LaunchAgents) / Linux systemd --user.
#                    No sudo. Survives CRASH (KeepAlive) + reboot-then-LOGIN
#                    (RunAtLoad). Dies on logout / boot-without-login.
#   --system          macOS LaunchDaemon (/Library/LaunchDaemons, runs as you at boot)
#                    / Linux systemd system unit. Needs sudo. Survives LOGOUT +
#                    boot-without-login — the real unattended grid-node answer.
#
# airc is the nervous system, not optional: this REQUIRES airc on PATH (ensured,
# never disabled) so the core finds it instantly at boot (`which airc`) and never
# does a blocking boot-time network install.
#
# Usage:
#   bash tools/scripts/install-service.sh [install|uninstall|status] [--system]
set -euo pipefail

# ── args ──
ACTION="install"; SCOPE="user"
for a in "$@"; do
  case "$a" in
    install|uninstall|status) ACTION="$a" ;;
    --system) SCOPE="system" ;;
    *) echo "usage: $0 [install|uninstall|status] [--system]" >&2; exit 2 ;;
  esac
done

LABEL="com.continuum.core"
SOCKET="${CONTINUUM_SOCKET:-/tmp/continuum-core.sock}"
DATA="$HOME/.continuum"
LOG_DIR="$DATA/logs"
SVC_USER="$(id -un)"

# ── airc is REQUIRED (the nervous system): ensure present, never disable ──
require_airc() {
  command -v airc >/dev/null 2>&1 && return 0
  echo "✗ airc not found on PATH. airc is the identity/rooms/event substrate — the core needs it." >&2
  echo "  Install it first, then re-run:  curl -fsSL https://raw.githubusercontent.com/CambrianTech/airc/main/install.sh | bash" >&2
  exit 1
}

# ── Resolve the core binary a service can exec (fail loud — never guess, never build-on-boot) ──
resolve_core_bin() {
  if [ -n "${CONTINUUM_CORE_BIN:-}" ]; then
    [ -x "$CONTINUUM_CORE_BIN" ] || { echo "CONTINUUM_CORE_BIN set but not executable: $CONTINUUM_CORE_BIN" >&2; return 1; }
    echo "$CONTINUUM_CORE_BIN"; return 0
  fi
  local tgt="${CARGO_TARGET_DIR:-$DATA/cache/cargo-target}" p
  for p in /usr/local/bin/continuum-core-server "$DATA/bin/continuum-core-server" \
           "$tgt/release/continuum-core-server" "$tgt/debug/continuum-core-server"; do
    [ -x "$p" ] && { echo "$p"; return 0; }
  done
  return 1
}

ort_dylib() { case "$(uname -s)" in Darwin) echo "/opt/homebrew/lib/libonnxruntime.dylib";; *) echo "$DATA/lib/libonnxruntime.so";; esac; }

# The wrapper command the supervisor runs: airc + toolchain on PATH so the core
# finds airc at boot (no blocking install), config.env sourced, ORT set, core
# exec'd in the FOREGROUND so the supervisor owns its lifecycle.
core_wrapper() {
  # if/then/fi (not `&&`) so the string is valid XML unescaped inside the plist.
  echo "export PATH=\"$HOME/.local/bin:/opt/homebrew/bin:/usr/local/bin:$HOME/.cargo/bin:\$PATH\"; if [ -f \"$DATA/config.env\" ]; then set -a; . \"$DATA/config.env\"; set +a; fi; export ORT_DYLIB_PATH=\"$(ort_dylib)\"; exec \"$1\" \"$SOCKET\""
}

# ════════════════════════ macOS ════════════════════════
mac_agent_plist() { echo "$HOME/Library/LaunchAgents/$LABEL.plist"; }
mac_daemon_plist() { echo "/Library/LaunchDaemons/$LABEL.plist"; }

mac_write_plist() { # $1=path  $2=bin  $3=extra <dict> entries
  local path="$1" bin="$2" extra="${3:-}"
  cat <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>$LABEL</string>
  <key>ProgramArguments</key>
  <array><string>/bin/bash</string><string>-lc</string><string>$(core_wrapper "$bin")</string></array>
  <key>RunAtLoad</key><true/>
  <!-- Crash-only relaunch: survive a CRASH (abnormal exit) + a reboot (RunAtLoad),
       but honor an explicit `continuum stop` (clean exit 0) so it stays down for dev
       ([[take-the-core-down-freely]]). Unconditional KeepAlive would instantly
       relaunch a deliberate stop, fighting the developer AND an operator draining a
       grid node. Crash + RunAtLoad still satisfies the reliability spine. -->
  <key>KeepAlive</key><dict><key>Crashed</key><true/></dict>
  <key>WorkingDirectory</key><string>$DATA</string>
  <key>StandardOutPath</key><string>$LOG_DIR/service.out.log</string>
  <key>StandardErrorPath</key><string>$LOG_DIR/service.err.log</string>
  <key>ProcessType</key><string>Background</string>$extra
</dict>
</plist>
PLIST
}

mac_install() {
  local bin; bin="$(resolve_core_bin)" || { echo "✗ no continuum-core-server binary — build it (npm start) or set CONTINUUM_CORE_BIN." >&2; exit 1; }
  mkdir -p "$LOG_DIR"
  if [ "$SCOPE" = "system" ]; then
    # LaunchDaemon: system scope (survives logout), but runs AS the installing
    # user with their HOME so it uses ~/.continuum (never root's).
    local extra="
  <key>UserName</key><string>$SVC_USER</string>
  <key>EnvironmentVariables</key><dict><key>HOME</key><string>$HOME</string></dict>"
    local tmp; tmp="$(mktemp)"
    mac_write_plist "$(mac_daemon_plist)" "$bin" "$extra" > "$tmp"
    plutil -lint "$tmp" >/dev/null
    sudo launchctl bootout "system/$LABEL" 2>/dev/null || true
    sudo install -m 0644 -o root -g wheel "$tmp" "$(mac_daemon_plist)"; rm -f "$tmp"
    sudo launchctl bootstrap system "$(mac_daemon_plist)"
    echo "✓ installed system LaunchDaemon '$LABEL' (survives logout) → $bin $SOCKET"
  else
    mkdir -p "$(dirname "$(mac_agent_plist)")"
    mac_write_plist "$(mac_agent_plist)" "$bin" > "$(mac_agent_plist)"
    plutil -lint "$(mac_agent_plist)" >/dev/null
    launchctl bootout "gui/$(id -u)/$LABEL" 2>/dev/null || true
    launchctl bootstrap "gui/$(id -u)" "$(mac_agent_plist)"
    launchctl enable "gui/$(id -u)/$LABEL" 2>/dev/null || true
    echo "✓ installed user LaunchAgent '$LABEL' (crash + reboot-login) → $bin $SOCKET"
  fi
  echo "  logs: $LOG_DIR/service.{out,err}.log"
}

mac_uninstall() {
  sudo launchctl bootout "system/$LABEL" 2>/dev/null || true; sudo rm -f "$(mac_daemon_plist)" 2>/dev/null || true
  launchctl bootout "gui/$(id -u)/$LABEL" 2>/dev/null || true; rm -f "$(mac_agent_plist)"
  echo "✓ removed '$LABEL' (both scopes)"
}

mac_status() {
  launchctl print "system/$LABEL" >/dev/null 2>&1 && { echo "✓ '$LABEL' loaded (system/LaunchDaemon)"; return; }
  launchctl print "gui/$(id -u)/$LABEL" >/dev/null 2>&1 && echo "✓ '$LABEL' loaded (user/LaunchAgent)" || echo "✗ '$LABEL' not loaded"
}

# ════════════════════════ Linux ════════════════════════
linux_user_unit() { echo "$HOME/.config/systemd/user/continuum-core.service"; }
linux_system_unit() { echo "/etc/systemd/system/continuum-core.service"; }

linux_unit_body() { # $1=bin  $2=User line (system only)
  cat <<UNIT
[Unit]
Description=Continuum headless core (reboot-surviving)
After=network.target

[Service]
Type=simple
${2:-}EnvironmentFile=-$DATA/config.env
Environment=ORT_DYLIB_PATH=$(ort_dylib)
Environment=PATH=$HOME/.local/bin:/usr/local/bin:/usr/bin:/bin:$HOME/.cargo/bin
WorkingDirectory=$DATA
ExecStart=$1 $SOCKET
# Crash-only relaunch (mirror the macOS KeepAlive.Crashed policy): restart on a
# CRASH (non-zero exit / signal), but honor a clean `continuum stop` (exit 0) so a
# deliberate take-down stays down ([[take-the-core-down-freely]]). Boot recovery is
# the [Install] WantedBy, not Restart. `systemctl stop` also never triggers a relaunch.
Restart=on-failure
RestartSec=2

[Install]
WantedBy=${3:-default.target}
UNIT
}

linux_install() {
  local bin; bin="$(resolve_core_bin)" || { echo "✗ no continuum-core-server binary — build/install it or set CONTINUUM_CORE_BIN." >&2; exit 1; }
  mkdir -p "$LOG_DIR"
  if [ "$SCOPE" = "system" ]; then
    linux_unit_body "$bin" "User=$SVC_USER
" "multi-user.target" | sudo tee "$(linux_system_unit)" >/dev/null
    sudo systemctl daemon-reload; sudo systemctl enable --now continuum-core.service
    echo "✓ installed systemd SYSTEM service (survives logout) → $bin $SOCKET"
  else
    mkdir -p "$(dirname "$(linux_user_unit)")"
    linux_unit_body "$bin" "" "default.target" > "$(linux_user_unit)"
    systemctl --user daemon-reload; systemctl --user enable --now continuum-core.service
    echo "✓ installed systemd --user service (crash + reboot-login) → $bin $SOCKET"
    echo "  (boot-without-login: 'sudo loginctl enable-linger $SVC_USER', or use --system)"
  fi
}

linux_uninstall() {
  sudo systemctl disable --now continuum-core.service 2>/dev/null || true; sudo rm -f "$(linux_system_unit)" 2>/dev/null || true; sudo systemctl daemon-reload 2>/dev/null || true
  systemctl --user disable --now continuum-core.service 2>/dev/null || true; rm -f "$(linux_user_unit)"; systemctl --user daemon-reload 2>/dev/null || true
  echo "✓ removed continuum-core service (both scopes)"
}

linux_status() { systemctl --user status continuum-core.service --no-pager 2>&1 | head -4; systemctl status continuum-core.service --no-pager 2>&1 | head -4; }

# ── Dispatch ──
[ "$ACTION" = "install" ] && require_airc
case "$(uname -s)" in
  Darwin) case "$ACTION" in install) mac_install;; uninstall) mac_uninstall;; status) mac_status;; esac ;;
  Linux)  case "$ACTION" in install) linux_install;; uninstall) linux_uninstall;; status) linux_status;; esac ;;
  *) echo "✗ No supervised-service path for $(uname -s) yet. Add a Windows Service: sc.exe create ContinuumCore binPath= \"…continuum-core-server $SOCKET\" start= auto" >&2; exit 1 ;;
esac
