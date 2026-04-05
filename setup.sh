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
cp src/scripts/continuum.sh "$INSTALL_DIR/continuum"
chmod +x "$INSTALL_DIR/continuum"
if echo "$PATH" | grep -q "$INSTALL_DIR"; then
  echo "✅ 'continuum' command installed"
else
  echo "✅ 'continuum' command installed at $INSTALL_DIR/continuum"
  echo "   Add to PATH: export PATH=\"$INSTALL_DIR:\$PATH\""
fi

# ── Pull pre-built images ────────────────────────
echo ""
echo "📦 Pulling pre-built images from GitHub Container Registry..."
echo "   (This replaces a 30+ minute build with a 2 minute download)"
echo ""
docker compose pull --ignore-pull-failures 2>&1 | grep -E "Pulled|exists|error" || true

# ── Detect Tailscale (no questions, no prompts) ──
# If Tailscale is installed and connected, the Grid auto-discovers peers.
# The Rust GridModule handles everything: discovery, connection, routing.
# No Docker sidecar needed — host Tailscale IPs are reachable directly.
# For dedicated server HTTPS (Docker sidecar): continuum grid enable
PROFILE=""
if [ -f .env ] && grep -q "COMPOSE_PROFILES=grid" .env; then
  PROFILE="--profile grid"
fi
if command -v tailscale &>/dev/null && tailscale status &>/dev/null 2>&1; then
  TS_IP=$(tailscale ip -4 2>/dev/null)
  echo "✅ Tailscale connected ($TS_IP) — Grid will auto-discover peers"
  # Write peer list so Docker containers can discover Tailscale nodes.
  # Containers can't run `tailscale status` (no CLI), but they CAN reach
  # Tailscale IPs via the host network. This file bridges the gap.
  mkdir -p "$HOME/.continuum/grid"
  tailscale status --json 2>/dev/null > "$HOME/.continuum/grid/tailscale-status.json" || true
else
  echo "ℹ️  No Tailscale — local only. Install https://tailscale.com for multi-machine Grid."
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

# Show remote access info if Tailscale is available
if command -v tailscale &>/dev/null && tailscale status &>/dev/null 2>&1; then
  TS_IP=$(tailscale ip -4 2>/dev/null)
  if [[ -n "$TS_IP" ]]; then
    echo "  📱 Remote (any device on your tailnet): http://$TS_IP:9003"
  fi
fi

LOCAL_URL="http://localhost:9003"
echo "  🏠 Local: $LOCAL_URL"
echo ""

# Open browser
open "$LOCAL_URL" 2>/dev/null || \
  xdg-open "$LOCAL_URL" 2>/dev/null || \
  cmd.exe /c start "$LOCAL_URL" 2>/dev/null || \
  echo "  Open: $LOCAL_URL"
echo ""
