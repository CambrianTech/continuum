#!/bin/bash
# Continuum Setup — interactive guided setup for Mac, Windows (Git Bash/WSL), Linux
set -e

echo ""
echo "  ╔═══════════════════════════════════════╗"
echo "  ║        continuum — setup              ║"
echo "  ╚═══════════════════════════════════════╝"
echo ""

# ── Detect platform ───────────────────────────────
if [[ "$OSTYPE" == "msys" ]] || [[ "$OSTYPE" == "cygwin" ]]; then
  echo "✅ Windows (Git Bash) detected"
  PLATFORM="windows"
elif [[ "$(uname -r 2>/dev/null)" == *microsoft* ]] || [[ "$(uname -r 2>/dev/null)" == *WSL* ]]; then
  echo "✅ WSL2 detected"
  PLATFORM="wsl"
elif [[ "$OSTYPE" == "darwin"* ]]; then
  echo "✅ macOS detected"
  PLATFORM="mac"
else
  echo "✅ Linux detected"
  PLATFORM="linux"
fi

# ── Check Docker ──────────────────────────────────
if ! command -v docker &>/dev/null; then
  echo "❌ Docker not found."
  echo "   Install Docker Desktop: https://www.docker.com/products/docker-desktop/"
  echo ""
  # Try to open the download page
  open "https://www.docker.com/products/docker-desktop/" 2>/dev/null || \
    xdg-open "https://www.docker.com/products/docker-desktop/" 2>/dev/null || \
    cmd.exe /c start "https://www.docker.com/products/docker-desktop/" 2>/dev/null || true
  exit 1
fi
echo "✅ Docker found"

# ── Ensure ~/.continuum exists ────────────────────
mkdir -p "$HOME/.continuum/grid"
# config.env MUST exist as a file before docker compose — bind mounts create
# directories for missing paths, which then fails with "not a directory".
touch "$HOME/.continuum/config.env"

# ── Check if Docker is running ────────────────────
if ! docker info &>/dev/null; then
  echo "❌ Docker is installed but not running. Start Docker Desktop and try again."
  exit 1
fi
echo "✅ Docker is running"

# ── Auto-configure Docker VM resources ────────────
# Docker Desktop and Rancher Desktop run a VM with fixed RAM/CPU.
# Default allocations (2-6GB) are too small for continuum.
# Auto-detect system resources and configure the VM to use half.
DOCKER_MEM=$(docker info --format '{{.MemTotal}}' 2>/dev/null || echo "0")
DOCKER_MEM_GB=$((DOCKER_MEM / 1073741824))
MIN_MEM_GB=8

if [ "$DOCKER_MEM_GB" -lt "$MIN_MEM_GB" ] && [ "$DOCKER_MEM_GB" -gt 0 ]; then
  echo ""
  echo "⚠️  Docker VM has ${DOCKER_MEM_GB}GB RAM (minimum ${MIN_MEM_GB}GB needed)."
  echo ""

  # Detect system RAM and offer to fix
  if [[ "$PLATFORM" == "mac" ]]; then
    SYS_MEM_GB=$(sysctl -n hw.memsize 2>/dev/null | awk '{printf "%d", $1/1073741824}')
    SYS_CPUS=$(sysctl -n hw.ncpu 2>/dev/null || echo "4")
  else
    SYS_MEM_GB=$(grep MemTotal /proc/meminfo 2>/dev/null | awk '{printf "%d", $2/1048576}')
    SYS_CPUS=$(nproc 2>/dev/null || echo "4")
  fi
  TARGET_MEM=$((SYS_MEM_GB / 2))
  TARGET_CPUS=$((SYS_CPUS / 2))
  [ "$TARGET_MEM" -lt "$MIN_MEM_GB" ] && TARGET_MEM="$MIN_MEM_GB"
  [ "$TARGET_CPUS" -lt 2 ] && TARGET_CPUS=2

  DOCKER_UPDATED=false

  # Try Rancher Desktop (macOS)
  RANCHER_SETTINGS="$HOME/Library/Preferences/rancher-desktop/settings.json"
  if [ -f "$RANCHER_SETTINGS" ]; then
    echo "   Rancher Desktop detected."
    echo "   Your system has ${SYS_MEM_GB}GB RAM / ${SYS_CPUS} CPUs."
    echo "   Recommended: ${TARGET_MEM}GB RAM / ${TARGET_CPUS} CPUs for Docker VM."
    echo ""
    read -p "   Update Rancher Desktop VM to ${TARGET_MEM}GB/${TARGET_CPUS}CPUs? [Y/n] " -n 1 -r
    echo ""
    if [[ $REPLY =~ ^[Nn]$ ]]; then
      echo "   Skipped. You can change this manually in Rancher Desktop → Preferences → Virtual Machine."
    else
    echo "   Updating VM: ${TARGET_MEM}GB RAM, ${TARGET_CPUS} CPUs..."
    # Use python3 (available on macOS) to patch JSON safely
    python3 -c "
import json, sys
d = json.load(open('$RANCHER_SETTINGS'))
d['virtualMachine']['memoryInGB'] = $TARGET_MEM
d['virtualMachine']['numberCPUs'] = $TARGET_CPUS
json.dump(d, open('$RANCHER_SETTINGS', 'w'), indent=2)
print('   Updated: memoryInGB=${TARGET_MEM}, numberCPUs=${TARGET_CPUS}')
" 2>/dev/null && DOCKER_UPDATED=true
    if [ "$DOCKER_UPDATED" = true ]; then
      echo "   Restarting Rancher Desktop VM (this takes ~30s)..."
      rdctl shutdown 2>/dev/null; sleep 5; rdctl start 2>/dev/null &
      # Wait for Docker to come back
      for i in $(seq 1 60); do
        docker info &>/dev/null && break
        sleep 2
      done
      echo "✅ Rancher Desktop VM reconfigured: ${TARGET_MEM}GB RAM, ${TARGET_CPUS} CPUs"
    fi
    fi # end y/n prompt
  fi

  # Try Docker Desktop (macOS)
  if [ "$DOCKER_UPDATED" = false ]; then
    DD_SETTINGS="$HOME/Library/Group Containers/group.com.docker/settings-store.json"
    DD_SETTINGS_ALT="$HOME/Library/Group Containers/group.com.docker/settings.json"
    DD_FILE=""
    [ -f "$DD_SETTINGS" ] && DD_FILE="$DD_SETTINGS"
    [ -f "$DD_SETTINGS_ALT" ] && DD_FILE="$DD_SETTINGS_ALT"

    if [ -n "$DD_FILE" ]; then
      echo "   Docker Desktop detected."
      echo "   Your system has ${SYS_MEM_GB}GB RAM / ${SYS_CPUS} CPUs."
      echo "   Recommended: ${TARGET_MEM}GB RAM / ${TARGET_CPUS} CPUs for Docker VM."
      echo ""
      read -p "   Update Docker Desktop VM to ${TARGET_MEM}GB/${TARGET_CPUS}CPUs? [Y/n] " -n 1 -r
      echo ""
      if [[ $REPLY =~ ^[Nn]$ ]]; then
        echo "   Skipped. Change manually: Docker Desktop → Settings → Resources → Memory"
      else
      echo "   Updating VM: ${TARGET_MEM}GB RAM, ${TARGET_CPUS} CPUs..."
      TARGET_MEM_MIB=$((TARGET_MEM * 1024))
      python3 -c "
import json
d = json.load(open('$DD_FILE'))
d['memoryMiB'] = $TARGET_MEM_MIB
d['cpus'] = $TARGET_CPUS
json.dump(d, open('$DD_FILE', 'w'), indent=2)
print('   Updated: memoryMiB=${TARGET_MEM_MIB}, cpus=${TARGET_CPUS}')
" 2>/dev/null && DOCKER_UPDATED=true
      if [ "$DOCKER_UPDATED" = true ]; then
        echo "   Restart Docker Desktop to apply. (Or: osascript -e 'quit app \"Docker\"' && open -a Docker)"
      fi
      fi # end y/n prompt
    fi
  fi

  # Linux: Docker uses host resources directly, no VM config needed
  if [ "$DOCKER_UPDATED" = false ] && [[ "$PLATFORM" == "linux" ]]; then
    echo "   Linux detected — Docker uses host resources directly. No VM to configure."
    DOCKER_UPDATED=true
  fi

  if [ "$DOCKER_UPDATED" = false ]; then
    echo ""
    echo "   Could not auto-configure. Please increase Docker VM memory to at least ${MIN_MEM_GB}GB:"
    echo "   Docker Desktop → Settings → Resources → Memory"
    echo "   Rancher Desktop → Preferences → Virtual Machine → Memory"
    echo ""
  fi
fi

# ── Install continuum CLI ─────────────────────────
INSTALL_DIR="${HOME}/.local/bin"
mkdir -p "$INSTALL_DIR"
cp bin/continuum "$INSTALL_DIR/continuum"
chmod +x "$INSTALL_DIR/continuum"
chmod +x bin/continuum
if echo "$PATH" | grep -q "$INSTALL_DIR"; then
  echo "✅ 'continuum' command installed"
else
  # Add to PATH for this session and persist for future sessions
  export PATH="$INSTALL_DIR:$PATH"
  # Add to shell profile if not already there
  for profile in "$HOME/.zshrc" "$HOME/.bashrc" "$HOME/.profile"; do
    if [ -f "$profile" ] && ! grep -q "\.local/bin" "$profile" 2>/dev/null; then
      echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$profile"
      echo "✅ 'continuum' command installed (added to $(basename $profile))"
      break
    fi
  done
fi

# ── Install tray app (macOS) ─────────────────────
# Proper .app bundle required for macOS menubar (NSStatusBar + LSUIElement).
if [[ "$PLATFORM" == "mac" ]]; then
  TRAY_SRC="bin/tray/continuum-tray"
  TRAY_APP="$HOME/Applications/Continuum.app"

  # Build from Swift source if binary doesn't exist
  if [[ ! -f "$TRAY_SRC" ]] && command -v swiftc &>/dev/null; then
    echo "🔨 Building tray app (Swift)..."
    (cd bin/tray && swiftc -O -o continuum-tray ContinuumTray.swift -framework Cocoa 2>/dev/null) || {
      echo "⚠️  Tray app build failed (non-critical). Requires Xcode command line tools."
    }
  fi

  if [[ -f "$TRAY_SRC" ]]; then
    mkdir -p "$TRAY_APP/Contents/MacOS" "$TRAY_APP/Contents/Resources"
    cp "$TRAY_SRC" "$TRAY_APP/Contents/MacOS/Continuum"
    chmod +x "$TRAY_APP/Contents/MacOS/Continuum"
    cp bin/tray/Info.plist "$TRAY_APP/Contents/Info.plist"
    # Auto-start on login via LaunchAgent
    LAUNCH_AGENTS="$HOME/Library/LaunchAgents"
    mkdir -p "$LAUNCH_AGENTS"
    cp bin/tray/com.cambriantech.continuum.plist "$LAUNCH_AGENTS/"
    echo "✅ Tray app installed → $TRAY_APP (auto-starts on login)"
  fi
elif [[ "$PLATFORM" == "windows" ]]; then
  # Windows: create startup shortcut for PowerShell tray
  STARTUP_DIR="$APPDATA/Microsoft/Windows/Start Menu/Programs/Startup"
  if [[ -d "$STARTUP_DIR" ]]; then
    TRAY_PS="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/bin/tray/continuum-tray.ps1"
    cat > "$STARTUP_DIR/continuum-tray.bat" << WINEOF
@echo off
powershell -WindowStyle Hidden -ExecutionPolicy Bypass -File "$TRAY_PS"
WINEOF
    echo "✅ System tray installed (auto-starts on login)"
  fi
elif [[ "$PLATFORM" == "linux" ]] || [[ "$PLATFORM" == "wsl" ]]; then
  # Linux: systemd user service
  SYSTEMD_DIR="$HOME/.config/systemd/user"
  mkdir -p "$SYSTEMD_DIR"
  TRAY_PY="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/bin/tray/continuum-tray.py"
  cat > "$SYSTEMD_DIR/continuum-tray.service" << SVCEOF
[Unit]
Description=Continuum System Tray
After=graphical-session.target

[Service]
ExecStart=/usr/bin/python3 $TRAY_PY
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
SVCEOF
  systemctl --user daemon-reload 2>/dev/null
  systemctl --user enable continuum-tray.service 2>/dev/null
  echo "✅ System tray installed (auto-starts on login)"
fi

# ── Pull pre-built images ────────────────────────
# Our images are public on GHCR — no auth needed.
# Logout to prevent Docker from flooding Keychain with credential prompts on macOS.
docker logout ghcr.io 2>/dev/null || true

echo ""
echo "📦 Pulling pre-built images..."
if docker compose pull 2>&1; then
  echo "✅ Images pulled from registry"
else
  # Pull failed (arch mismatch, network, etc.) — build locally
  echo ""
  echo "⚠️  Pre-built images not available for this platform. Building locally..."
  echo "   (This takes 15-20 minutes the first time. Subsequent starts are instant.)"
  echo ""
  docker compose build --parallel 2>&1 | tail -5
  echo "✅ Images built locally"
fi

# ── Detect Tailscale + auto-enable Grid ──────────
# If Tailscale is connected and TS_AUTHKEY exists, enable grid profile automatically.
# No manual .env editing, no --profile flags, no questions.
PROFILE=""

# Load config.env for TS_AUTHKEY
if [ -f "$HOME/.continuum/config.env" ]; then
  source "$HOME/.continuum/config.env" 2>/dev/null || true
fi

if command -v tailscale &>/dev/null && tailscale status &>/dev/null 2>&1; then
  TS_IP=$(tailscale ip -4 2>/dev/null)
  echo "✅ Tailscale connected ($TS_IP)"

  # Cache Tailscale status for Docker containers (they can't run tailscale CLI)
  mkdir -p "$HOME/.continuum/grid"
  tailscale status --json 2>/dev/null > "$HOME/.continuum/grid/tailscale-status.json" || true

  # Auto-enable grid if we have an auth key
  if [ -n "${TS_AUTHKEY:-}" ]; then
    TS_HOSTNAME="${TS_HOSTNAME:-$(hostname -s)-grid}"

    # Write .env for docker compose (only grid-specific settings)
    if ! grep -q "COMPOSE_PROFILES=grid" .env 2>/dev/null; then
      cat > .env << ENVEOF
TS_HOSTNAME=$TS_HOSTNAME
TS_AUTHKEY=$TS_AUTHKEY
COMPOSE_PROFILES=grid
ENVEOF
      echo "✅ Grid enabled (hostname: $TS_HOSTNAME)"
    else
      echo "✅ Grid already configured"
    fi
    PROFILE="--profile grid"
  else
    echo "ℹ️  Tailscale connected but no TS_AUTHKEY in ~/.continuum/config.env"
    echo "   Grid HTTPS disabled. To enable:"
    echo "   1. Generate auth key: https://login.tailscale.com/admin/settings/keys"
    echo "      (Set: Reusable ON, 90 days, NOT ephemeral)"
    echo "   2. Add to ~/.continuum/config.env: TS_AUTHKEY=tskey-auth-..."
    echo "   3. Re-run: ./setup.sh"
    echo ""
    echo "   Grid discovery still works via host Tailscale (http://$TS_IP:9003)"
  fi
else
  echo "ℹ️  No Tailscale — local only. Install https://tailscale.com for multi-machine Grid."
fi

# Respect existing .env profile
if [ -f .env ] && grep -q "COMPOSE_PROFILES=grid" .env 2>/dev/null; then
  PROFILE=""  # docker compose reads from .env automatically
fi

# ── Start ─────────────────────────────────────────
echo ""
echo "🚀 Starting Continuum..."
echo ""
docker compose $PROFILE up -d

echo ""
echo "⏳ First run downloads voice models (~150MB). Subsequent starts are instant."
echo ""

# Wait for services to be healthy (widget-server is the last in the chain)
echo "Waiting for services..."
for i in $(seq 1 90); do
  if docker compose ps widget-server 2>/dev/null | grep -q "healthy"; then
    break
  fi
  if [ $((i % 10)) -eq 0 ]; then
    HEALTHY=$(docker compose ps 2>/dev/null | grep -c "healthy" || echo "0")
    echo "   $HEALTHY/6 services ready..."
  fi
  sleep 2
done

echo ""

echo "  ✅ Continuum is running!"
echo ""

# Show access URLs
LOCAL_URL="http://localhost:9003"
echo "  🏠 Local: $LOCAL_URL"

if [ -n "${TS_HOSTNAME:-}" ]; then
  # Get tailnet suffix for HTTPS URL
  TAILNET=$(tailscale status --json 2>/dev/null | python3 -c "import sys,json; print(json.load(sys.stdin).get('MagicDNSSuffix',''))" 2>/dev/null || echo "")
  if [ -n "$TAILNET" ]; then
    REMOTE_URL="https://$TS_HOSTNAME.$TAILNET"
    echo "  🌐 Grid:  $REMOTE_URL (HTTPS, accessible from any device)"
  fi
elif command -v tailscale &>/dev/null && tailscale status &>/dev/null 2>&1; then
  TS_IP=$(tailscale ip -4 2>/dev/null)
  if [ -n "$TS_IP" ]; then
    echo "  📱 Remote: http://$TS_IP:9003 (any device on your tailnet)"
  fi
fi

echo ""
echo "  Run 'continuum status' anytime to check health."
echo "  Run 'continuum doctor' to diagnose issues."
echo ""

# Launch platform-specific tray app
if [[ "$PLATFORM" == "mac" ]] && [[ -d "$HOME/Applications/Continuum.app" ]]; then
  pkill -f "Continuum.app" 2>/dev/null || true
  sleep 0.5
  open "$HOME/Applications/Continuum.app"
  echo "  🖥️ Tray app running (menubar)"
elif [[ "$PLATFORM" == "windows" ]]; then
  TRAY_PS="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/bin/tray/continuum-tray.ps1"
  if [[ -f "$TRAY_PS" ]]; then
    powershell.exe -WindowStyle Hidden -File "$TRAY_PS" &
    echo "  🖥️ System tray running"
  fi
elif [[ "$PLATFORM" == "linux" ]] || [[ "$PLATFORM" == "wsl" ]]; then
  TRAY_PY="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/bin/tray/continuum-tray.py"
  if [[ -f "$TRAY_PY" ]] && command -v python3 &>/dev/null; then
    python3 "$TRAY_PY" &>/dev/null &
    echo "  🖥️ System tray running"
  fi
fi

# Open browser (prefer HTTPS grid URL if available)
OPEN_URL="${REMOTE_URL:-$LOCAL_URL}"
open "$OPEN_URL" 2>/dev/null || \
  xdg-open "$OPEN_URL" 2>/dev/null || \
  cmd.exe /c start "$OPEN_URL" 2>/dev/null || \
  echo "  Open: $OPEN_URL"
echo ""
