#!/bin/bash
# Continuum Setup — interactive guided setup for Mac, Windows (Git Bash/WSL), Linux
set -e

echo ""
echo "  ╔═══════════════════════════════════════╗"
echo "  ║        continuum — setup              ║"
echo "  ╚═══════════════════════════════════════╝"
echo ""

# ── Windows: WSL2 required ────────────────────────
if [[ "$OSTYPE" == "msys" ]] || [[ "$OSTYPE" == "cygwin" ]]; then
  echo "❌ Windows requires WSL2 to run Continuum."
  echo ""
  echo "   1. Open PowerShell as Administrator"
  echo "   2. Run: wsl --install"
  echo "   3. Restart your computer"
  echo "   4. Open Ubuntu from Start menu"
  echo "   5. Run this setup again from inside WSL2"
  echo ""
  echo "   Also install Docker Desktop: https://docker.com/products/docker-desktop"
  echo "   (In Docker Desktop settings, enable WSL2 integration)"
  echo ""
  exit 1
elif [[ "$(uname -r 2>/dev/null)" == *microsoft* ]] || [[ "$(uname -r 2>/dev/null)" == *WSL* ]]; then
  echo "✅ WSL2 detected"
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
  elif [[ ! "$TS_KEY" == tskey-auth-* ]]; then
    echo ""
    echo "❌ That doesn't look like a Tailscale auth key."
    echo "   Auth keys start with 'tskey-auth-'"
    echo ""
    echo "   Go to: https://login.tailscale.com/admin/settings/keys"
    echo "   Click 'Generate auth key', check 'Reusable' + 'Ephemeral', copy the key."
    echo ""
    open "https://login.tailscale.com/admin/settings/keys" 2>/dev/null || \
      xdg-open "https://login.tailscale.com/admin/settings/keys" 2>/dev/null || \
      cmd.exe /c start "https://login.tailscale.com/admin/settings/keys" 2>/dev/null || true
    read -p "Paste your auth key: " TS_KEY
    if [[ ! "$TS_KEY" == tskey-auth-* ]]; then
      echo "⚠️  Skipping Tailscale — starting in local-only mode."
      PROFILE=""
      TS_KEY=""
    fi
  fi

  if [[ -n "$TS_KEY" ]]; then
    TS_HOST=$(hostname | tr '[:upper:]' '[:lower:]' | tr -cd 'a-z0-9-')
    cat > .env <<EOF
TS_AUTHKEY=$TS_KEY
TS_HOSTNAME=$TS_HOST
COMPOSE_PROFILES=grid
EOF
    echo "✅ Saved to .env (hostname: $TS_HOST)"
    echo ""
    echo "⚠️  IMPORTANT: Make sure you enabled HTTPS certificates in Tailscale:"
    echo "   https://login.tailscale.com/admin/dns → 'HTTPS Certificates' must be ON"
    echo "   (Without this, the HTTPS URL won't work)"
    echo ""
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

echo ""

if [[ -n "$PROFILE" ]]; then
  # Verify Tailscale actually connected
  TS_STATUS=$(docker compose logs tailscale 2>&1 | tail -20)

  if echo "$TS_STATUS" | grep -qi "logged in\|Success\|ready"; then
    TS_HOSTNAME=$(grep TS_HOSTNAME .env 2>/dev/null | cut -d= -f2)
    echo "  ✅ Continuum is running!"
    echo ""
    echo "  Open from any device on your tailnet:"
    echo "  https://$TS_HOSTNAME.your-tailnet.ts.net"
    echo ""
    echo "  (Find your exact URL: tailscale status)"
  elif echo "$TS_STATUS" | grep -qi "auth\|unauthorized\|invalid"; then
    echo "  ⚠️  Continuum started but Tailscale auth failed."
    echo ""
    echo "  Your auth key may be invalid or expired."
    echo "  Generate a new one: https://login.tailscale.com/admin/settings/keys"
    echo "  Then update .env and run: docker compose --profile grid restart tailscale"
    echo ""
    echo "  Local access still works: http://localhost:9003"
  elif echo "$TS_STATUS" | grep -qi "certificate\|acme\|tls"; then
    echo "  ⚠️  Continuum started but HTTPS certificates failed."
    echo ""
    echo "  You need to enable HTTPS in Tailscale:"
    echo "  https://login.tailscale.com/admin/dns → toggle 'HTTPS Certificates' ON"
    echo "  Then: docker compose --profile grid restart tailscale"
    echo ""
    echo "  Local access still works: http://localhost:9003"
  else
    echo "  ✅ Continuum is running!"
    echo ""
    echo "  Tailscale is still connecting (can take 30s)..."
    echo "  Check status: docker compose logs tailscale"
    echo ""
    echo "  Local access: http://localhost:9003"
  fi
else
  echo "  ✅ Continuum is running!"
  echo ""
  echo "  Open: http://localhost:9003"
fi
echo ""
