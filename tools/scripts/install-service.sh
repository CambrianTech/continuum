#!/usr/bin/env bash
# install-service.sh — install the Continuum core as a REBOOT-SURVIVING OS service.
#
# Reliability spine #2 (upstart): a grid node must come back on its own after a
# reboot or a crash — never "the persona died because the Mac slept and nobody
# ran `cu start` again" ([[grid-node-resilience]]). This installs the headless
# core under the platform's supervisor so it (a) starts at login/boot and (b) is
# restarted if it dies.
#
# Outlier-validated: launchd (macOS) is the fully-built + on-this-machine-tested
# path; systemd (Linux) is written to the same contract for the grid nodes; other
# platforms fail loud with the manual command rather than pretend.
#
# Usage:
#   bash tools/scripts/install-service.sh            # install + start
#   bash tools/scripts/install-service.sh uninstall  # stop + remove
#   bash tools/scripts/install-service.sh status      # is it loaded / running?
#
# Env:
#   CONTINUUM_CORE_BIN   explicit path to continuum-core-server (else auto-resolved)
#   CONTINUUM_SOCKET     IPC socket (default /tmp/continuum-core.sock)
set -euo pipefail

ACTION="${1:-install}"
LABEL="com.continuum.core"
SOCKET="${CONTINUUM_SOCKET:-/tmp/continuum-core.sock}"
DATA="$HOME/.continuum"
LOG_DIR="$DATA/logs"

# ── Resolve the core binary a service can exec (fail loud — never guess) ──
# Order: explicit override → installed system paths → dev cargo target. A service
# points at a BUILT binary (it must not `cargo build` on every boot); if none
# exists we refuse and tell the user to build/install first.
resolve_core_bin() {
  if [ -n "${CONTINUUM_CORE_BIN:-}" ]; then
    [ -x "$CONTINUUM_CORE_BIN" ] || { echo "CONTINUUM_CORE_BIN set but not executable: $CONTINUUM_CORE_BIN" >&2; return 1; }
    echo "$CONTINUUM_CORE_BIN"; return 0
  fi
  local tgt="${CARGO_TARGET_DIR:-$DATA/cache/cargo-target}"
  local p
  for p in \
    /usr/local/bin/continuum-core-server \
    "$DATA/bin/continuum-core-server" \
    "$tgt/release/continuum-core-server" \
    "$tgt/debug/continuum-core-server"; do
    [ -x "$p" ] && { echo "$p"; return 0; }
  done
  return 1
}

# ── The one legal ORT dylib path (the core links onnxruntime dynamically) ──
ort_dylib() {
  case "$(uname -s)" in
    Darwin) echo "/opt/homebrew/lib/libonnxruntime.dylib" ;;
    *)      echo "$DATA/lib/libonnxruntime.so" ;;
  esac
}

# ── macOS: LaunchAgent (per-user; survives logout→login + reboot) ──
mac_plist() { echo "$HOME/Library/LaunchAgents/$LABEL.plist"; }

mac_install() {
  local bin; bin="$(resolve_core_bin)" || {
    echo "✗ no continuum-core-server binary found. Build it first (npm start / cu start), or set CONTINUUM_CORE_BIN." >&2
    exit 1
  }
  mkdir -p "$LOG_DIR" "$(dirname "$(mac_plist)")"
  # A tiny bash wrapper sources config.env (launchd can't) + sets ORT, then execs
  # the core in the FOREGROUND so launchd owns its lifecycle (KeepAlive restarts).
  cat > "$(mac_plist)" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>$LABEL</string>
  <key>ProgramArguments</key>
  <array>
    <string>/bin/bash</string>
    <string>-lc</string>
    <string>export PATH="$HOME/.local/bin:/opt/homebrew/bin:/usr/local/bin:$HOME/.cargo/bin:\$PATH"; [ -f "$DATA/config.env" ] &amp;&amp; { set -a; . "$DATA/config.env"; set +a; }; export ORT_DYLIB_PATH="$(ort_dylib)"; export AIRC_DISABLE_AUTOINSTALL=1; exec "$bin" "$SOCKET"</string>
  </array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>WorkingDirectory</key><string>$DATA</string>
  <key>StandardOutPath</key><string>$LOG_DIR/service.out.log</string>
  <key>StandardErrorPath</key><string>$LOG_DIR/service.err.log</string>
  <key>ProcessType</key><string>Background</string>
</dict>
</plist>
PLIST
  # bootout any prior instance, then bootstrap into the per-user GUI domain.
  launchctl bootout "gui/$(id -u)/$LABEL" 2>/dev/null || true
  launchctl bootstrap "gui/$(id -u)" "$(mac_plist)"
  launchctl enable "gui/$(id -u)/$LABEL" 2>/dev/null || true
  echo "✓ installed launchd service '$LABEL' → $bin $SOCKET"
  echo "  logs: $LOG_DIR/service.{out,err}.log   plist: $(mac_plist)"
}

mac_uninstall() {
  launchctl bootout "gui/$(id -u)/$LABEL" 2>/dev/null || true
  rm -f "$(mac_plist)"
  echo "✓ removed launchd service '$LABEL'"
}

mac_status() {
  if launchctl print "gui/$(id -u)/$LABEL" >/dev/null 2>&1; then
    echo "✓ '$LABEL' is loaded"; launchctl print "gui/$(id -u)/$LABEL" 2>/dev/null | grep -iE "state|pid" | head -3
  else
    echo "✗ '$LABEL' not loaded"
  fi
}

# ── Linux: systemd user unit (grid nodes — written to contract, verify on Linux) ──
linux_unit() { echo "$HOME/.config/systemd/user/continuum-core.service"; }

linux_install() {
  local bin; bin="$(resolve_core_bin)" || {
    echo "✗ no continuum-core-server binary found. Build/install it first, or set CONTINUUM_CORE_BIN." >&2
    exit 1
  }
  mkdir -p "$LOG_DIR" "$(dirname "$(linux_unit)")"
  cat > "$(linux_unit)" <<UNIT
[Unit]
Description=Continuum headless core (reboot-surviving)
After=network.target

[Service]
Type=simple
EnvironmentFile=-$DATA/config.env
Environment=ORT_DYLIB_PATH=$(ort_dylib)
Environment=AIRC_DISABLE_AUTOINSTALL=1
Environment=PATH=%h/.local/bin:/usr/local/bin:/usr/bin:/bin:%h/.cargo/bin
WorkingDirectory=$DATA
ExecStart=$bin $SOCKET
Restart=always
RestartSec=2

[Install]
WantedBy=default.target
UNIT
  systemctl --user daemon-reload
  systemctl --user enable --now continuum-core.service
  echo "✓ installed systemd --user service → $bin $SOCKET"
  echo "  (enable lingering for boot-without-login: 'sudo loginctl enable-linger $USER')"
}

linux_uninstall() {
  systemctl --user disable --now continuum-core.service 2>/dev/null || true
  rm -f "$(linux_unit)"; systemctl --user daemon-reload 2>/dev/null || true
  echo "✓ removed systemd --user service"
}

linux_status() { systemctl --user status continuum-core.service --no-pager 2>&1 | head -6 || echo "✗ not installed"; }

# ── Dispatch ──
case "$(uname -s)" in
  Darwin) case "$ACTION" in install) mac_install;; uninstall) mac_uninstall;; status) mac_status;; *) echo "usage: $0 [install|uninstall|status]" >&2; exit 2;; esac ;;
  Linux)  case "$ACTION" in install) linux_install;; uninstall) linux_uninstall;; status) linux_status;; *) echo "usage: $0 [install|uninstall|status]" >&2; exit 2;; esac ;;
  *) echo "✗ No supervised-service path for $(uname -s) yet. Run the core manually or add a Windows Service (sc.exe create ContinuumCore binPath= \"…continuum-core-server $SOCKET\" start= auto)." >&2; exit 1 ;;
esac
