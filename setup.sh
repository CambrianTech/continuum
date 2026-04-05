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
    echo ""
    # Try to open in browser
    open "https://$TS_HOSTNAME.ts.net" 2>/dev/null || \
      xdg-open "https://$TS_HOSTNAME.ts.net" 2>/dev/null || \
      cmd.exe /c start "https://$TS_HOSTNAME.ts.net" 2>/dev/null || true
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

  # Auto-detect Tailscale on host — zero config remote access.
  # If Tailscale is installed and connected, Docker ports are accessible
  # via the Tailscale IP (WireGuard encrypts the tunnel automatically).
  if command -v tailscale &>/dev/null && tailscale status &>/dev/null 2>&1; then
    TS_IP=$(tailscale ip -4 2>/dev/null)
    if [[ -n "$TS_IP" ]]; then
      echo "  📱 Remote access (any device on your tailnet):"
      echo "     http://$TS_IP:9003"
      echo ""
    fi
  fi

  LOCAL_URL="http://localhost:9003"
  echo "  Opening $LOCAL_URL ..."
  # Open browser (works on Mac, Linux, WSL2)
  open "$LOCAL_URL" 2>/dev/null || \
    xdg-open "$LOCAL_URL" 2>/dev/null || \
    cmd.exe /c start "$LOCAL_URL" 2>/dev/null || \
    echo "  Open: $LOCAL_URL"
fi
echo ""
