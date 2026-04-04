#!/bin/bash
# Continuum Setup — interactive guided setup for Mac, Windows (Git Bash/WSL), Linux
set -e

echo ""
echo "  ╔═══════════════════════════════════════╗"
echo "  ║        continuum — setup              ║"
echo "  ╚═══════════════════════════════════════╝"
echo ""

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

# ── Ask about Tailscale (optional) ────────────────
echo ""
echo "Tailscale gives you encrypted HTTPS access from any device (phone, laptop, etc)."
echo "It's free and takes 2 minutes. Skip if you only want local access."
echo ""
read -p "Set up Tailscale for remote access? [y/N] " SETUP_TAILSCALE

if [[ "$SETUP_TAILSCALE" =~ ^[Yy] ]]; then
  # Check if Tailscale is installed
  if ! command -v tailscale &>/dev/null; then
    echo ""
    echo "📦 Install Tailscale first: https://tailscale.com/download"
    open "https://tailscale.com/download" 2>/dev/null || \
      xdg-open "https://tailscale.com/download" 2>/dev/null || \
      cmd.exe /c start "https://tailscale.com/download" 2>/dev/null || true
    echo ""
    read -p "Press Enter after installing Tailscale..."
  fi

  echo ""
  echo "Two quick steps in the Tailscale admin console:"
  echo ""
  echo "  1. Enable HTTPS: DNS tab → toggle 'HTTPS Certificates' ON"
  echo "  2. Create auth key: Settings → Keys → 'Generate auth key'"
  echo "     (check 'Reusable' and 'Ephemeral')"
  echo ""
  echo "Opening Tailscale admin console..."
  sleep 1
  open "https://login.tailscale.com/admin/dns" 2>/dev/null || \
    xdg-open "https://login.tailscale.com/admin/dns" 2>/dev/null || \
    cmd.exe /c start "https://login.tailscale.com/admin/dns" 2>/dev/null || true

  echo ""
  read -p "Paste your Tailscale auth key (tskey-auth-...): " TS_KEY

  if [[ -z "$TS_KEY" ]]; then
    echo "⚠️  No key provided — starting in local-only mode."
    PROFILE=""
  else
    echo "TS_AUTHKEY=$TS_KEY" > .env
    echo "TS_HOSTNAME=$(hostname | tr '[:upper:]' '[:lower:]' | tr -cd 'a-z0-9-')" >> .env
    echo "COMPOSE_PROFILES=grid" >> .env
    echo "✅ Saved to .env"
    PROFILE="--profile grid"
  fi
else
  PROFILE=""
  echo "✅ Local-only mode (http://localhost:9003)"
fi

# ── Start ─────────────────────────────────────────
echo ""
echo "🚀 Starting Continuum..."
echo ""
docker compose $PROFILE up -d

echo ""
echo "⏳ First run downloads voice models (~2GB). This takes a few minutes."
echo "   Subsequent starts are instant."
echo ""

# Wait for widget-server to be healthy
echo "Waiting for services to start..."
for i in $(seq 1 60); do
  if docker compose ps widget-server 2>/dev/null | grep -q "healthy"; then
    break
  fi
  sleep 2
done

if [[ -n "$PROFILE" ]]; then
  # Get Tailscale hostname
  TS_HOSTNAME=$(grep TS_HOSTNAME .env 2>/dev/null | cut -d= -f2)
  echo ""
  echo "  ╔═══════════════════════════════════════╗"
  echo "  ║  ✅ Continuum is running!              ║"
  echo "  ║                                        ║"
  echo "  ║  Open from any device on your tailnet: ║"
  echo "  ║  https://$TS_HOSTNAME.tailnet-name.ts.net"
  echo "  ╚═══════════════════════════════════════╝"
else
  echo ""
  echo "  ╔═══════════════════════════════════════╗"
  echo "  ║  ✅ Continuum is running!              ║"
  echo "  ║                                        ║"
  echo "  ║  Open: http://localhost:9003            ║"
  echo "  ╚═══════════════════════════════════════╝"
fi
echo ""
