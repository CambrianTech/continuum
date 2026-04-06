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

# ── Check if Docker is running ────────────────────
if ! docker info &>/dev/null; then
  echo "❌ Docker is installed but not running. Start Docker Desktop and try again."
  exit 1
fi
echo "✅ Docker is running"

# ── Check Docker resources ────────────────────────
# Rancher Desktop and Docker Desktop have configurable VM memory.
# 8GB minimum needed for all containers.
DOCKER_MEM=$(docker info --format '{{.MemTotal}}' 2>/dev/null || echo "0")
DOCKER_MEM_GB=$((DOCKER_MEM / 1073741824))
if [ "$DOCKER_MEM_GB" -lt 8 ] && [ "$DOCKER_MEM_GB" -gt 0 ]; then
  echo ""
  echo "⚠️  Docker has ${DOCKER_MEM_GB}GB RAM. Continuum needs at least 8GB."
  echo "   Increase in Docker Desktop → Settings → Resources → Memory"
  echo "   Or Rancher Desktop → Preferences → Virtual Machine → Memory"
  echo ""
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
  echo "✅ 'continuum' command installed at $INSTALL_DIR/continuum"
  echo "   Add to PATH: export PATH=\"\$INSTALL_DIR:\$PATH\""
fi

# ── Install tray app (macOS) ─────────────────────
if [[ "$PLATFORM" == "mac" ]] && command -v swiftc &>/dev/null; then
  TRAY_BIN="bin/tray/continuum-tray"
  TRAY_INSTALL="$HOME/.local/bin/continuum-tray"

  # Build Swift tray if binary doesn't exist
  if [[ ! -f "$TRAY_BIN" ]]; then
    echo "🔨 Building tray app (Swift)..."
    (cd bin/tray && swiftc -O -o continuum-tray ContinuumTray.swift -framework Cocoa 2>/dev/null) || {
      echo "⚠️  Tray app build failed (non-critical). Requires Xcode command line tools."
    }
  fi

  if [[ -f "$TRAY_BIN" ]]; then
    cp "$TRAY_BIN" "$TRAY_INSTALL"
    chmod +x "$TRAY_INSTALL"
    echo "✅ Tray app installed → $TRAY_INSTALL"
    echo "   Run: continuum-tray &"
  fi
fi

# ── Pull pre-built images ────────────────────────
# Our images are public on GHCR — no auth needed.
# Logout to prevent Docker from flooding Keychain with credential prompts on macOS.
docker logout ghcr.io 2>/dev/null || true

echo ""
echo "📦 Pulling pre-built images..."
if docker compose pull 2>&1 | tee /tmp/continuum-pull.log | grep -q "Pulled"; then
  echo "✅ Images pulled from registry"
else
  # Pull failed (arch mismatch, network, etc.) — build locally
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

# Open browser (prefer HTTPS grid URL if available)
OPEN_URL="${REMOTE_URL:-$LOCAL_URL}"
open "$OPEN_URL" 2>/dev/null || \
  xdg-open "$OPEN_URL" 2>/dev/null || \
  cmd.exe /c start "$OPEN_URL" 2>/dev/null || \
  echo "  Open: $OPEN_URL"
echo ""
