#!/bin/bash
# Continuum — One-command installer
# Usage: curl -fsSL https://cambriantech.github.io/continuum/install.sh | bash
#
# Docker-first: pulls pre-built images, no compilation needed.
# Optional: Tailscale for mesh networking + TLS (voice/video).
set -e

# Log primitives (info/ok/warn/fail/die) come from
# src/scripts/lib/install-common.sh after clone. Until the repo is
# cloned, use these minimal pre-clone versions; they'll be overridden
# when we source the canonical library below.
info()  { echo -e "\033[1;36m→\033[0m $*"; }
ok()    { echo -e "\033[1;32m✓\033[0m $*"; }
warn()  { echo -e "\033[1;33m!\033[0m $*"; }
fail()  { echo -e "\033[1;31m✗\033[0m $*"; exit 1; }
# Alias so the canonical lib's `die` also works here and vice versa.
die()   { fail "$@"; }

REPO="https://github.com/CambrianTech/continuum.git"
INSTALL_DIR="${CONTINUUM_DIR:-$HOME/continuum}"
CONTINUUM_DATA="$HOME/.continuum"

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  Continuum Installer"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# ── 1. Detect environment ───────────────────────────────────
info "Detecting environment..."

OS="$(uname -s)"
ARCH="$(uname -m)"
HAS_GPU=false

case "$OS" in
  Linux)
    if command -v nvidia-smi &>/dev/null || [ -f /usr/lib/wsl/lib/nvidia-smi ]; then
      HAS_GPU=true
      GPU_NAME=$(nvidia-smi --query-gpu=name --format=csv,noheader 2>/dev/null | head -1 || echo "NVIDIA GPU")
      ok "GPU detected: $GPU_NAME"
    fi
    ;;
  Darwin)
    ok "macOS $ARCH"
    ;;
  *) fail "Unsupported OS: $OS" ;;
esac

# ── 2. Pre-clone bootstrap: git + minimal Docker presence check ────
# We can't source the canonical module library yet (lives in the repo).
# Just verify prerequisites so the clone can happen. Deeper checks live
# in the canonical modules that run after the clone.

if ! command -v git &>/dev/null; then
  case "$OS" in
    Linux) fail "git required. Run: sudo apt-get install -y git  (or equivalent), then re-run." ;;
    Darwin) fail "git required. Run: brew install git  (or install Xcode CLI tools), then re-run." ;;
  esac
fi

if ! command -v docker &>/dev/null; then
  case "$OS" in
    Linux)
      info "Docker not found — installing via get.docker.com…"
      curl -fsSL https://get.docker.com | sh
      sudo usermod -aG docker "$USER"
      warn "Added $USER to docker group — log out and back in, then re-run this script"
      exit 0
      ;;
    Darwin)
      fail "Install Docker Desktop (https://docker.com/products/docker-desktop) or Rancher Desktop (https://rancherdesktop.io) and re-run"
      ;;
  esac
fi

# ── 3. Clone / update repo ─────────────────────────────────
if [ -d "$INSTALL_DIR/.git" ]; then
  info "Updating existing installation..."
  cd "$INSTALL_DIR"
  git pull --ff-only 2>/dev/null || warn "Could not update — using existing version"
else
  info "Cloning Continuum..."
  git clone --depth 1 "$REPO" "$INSTALL_DIR"
  cd "$INSTALL_DIR"
fi

# ── 4. Shared modules (same code that Dev runs via npm start) ────
# docs/infrastructure/INSTALL-ARCHITECTURE.md §Module-shape: the canonical
# module library at src/scripts/lib/install-common.sh defines
# mod_submodules_init + mod_docker_wsl_integration + log/sudo primitives.
# Carl and Dev call the SAME functions so there's no drift.
if [ ! -f "src/scripts/lib/install-common.sh" ]; then
  fail "Canonical install library missing at src/scripts/lib/install-common.sh — incomplete clone? Try: rm -rf $INSTALL_DIR && re-run this installer."
fi

# shellcheck source=src/scripts/lib/install-common.sh
source "src/scripts/lib/install-common.sh"

mod_submodules_init
mod_docker_wsl_integration

# Now the real docker daemon check — after WSL integration module had
# a chance to fix it on Windows/WSL2 hosts.
if ! docker info &>/dev/null 2>&1; then
  fail "Docker daemon not reachable. Start Docker Desktop / Rancher Desktop and re-run."
fi
ok "Docker $(docker version --format '{{.Client.Version}}' 2>/dev/null || echo 'ready')"
ok "Source: $INSTALL_DIR"

# ── 3b. Install continuum command ─────────────────────────
BIN_TARGET="/usr/local/bin/continuum"
if [ -w "/usr/local/bin" ]; then
  cp "$INSTALL_DIR/bin/continuum" "$BIN_TARGET"
elif command -v sudo &>/dev/null; then
  sudo cp "$INSTALL_DIR/bin/continuum" "$BIN_TARGET"
else
  BIN_TARGET="$HOME/.local/bin/continuum"
  mkdir -p "$HOME/.local/bin"
  cp "$INSTALL_DIR/bin/continuum" "$BIN_TARGET"
fi
chmod +x "$BIN_TARGET"
ok "Command: $BIN_TARGET"

# ── 4. Configuration ───────────────────────────────────────
mkdir -p "$CONTINUUM_DATA"

CONFIG_FILE="$CONTINUUM_DATA/config.env"
if [ ! -f "$CONFIG_FILE" ]; then
  info "Creating default config (zero API keys = local-only mode)..."
  cat > "$CONFIG_FILE" << 'EOF'
# Continuum Configuration — all API keys OPTIONAL
# System works with zero keys using local Candle inference.
# Add keys to enable cloud providers for better quality.

# ANTHROPIC_API_KEY=sk-ant-...
# OPENAI_API_KEY=sk-...
# DEEPSEEK_API_KEY=sk-...

HTTP_PORT=9000
WS_PORT=9001
EOF
  ok "Config: $CONFIG_FILE"
else
  ok "Config exists: $CONFIG_FILE"
fi

# ── 5. TLS certs (Tailscale) ──────────────────────────────
TS_HOSTNAME=""
if command -v tailscale &>/dev/null; then
  TS_HOSTNAME=$(tailscale status --json 2>/dev/null | python3 -c "import sys,json; print(json.load(sys.stdin).get('Self',{}).get('DNSName','').rstrip('.'))" 2>/dev/null || echo "")

  if [ -n "$TS_HOSTNAME" ]; then
    if [ -f "$CONTINUUM_DATA/$TS_HOSTNAME.crt" ]; then
      ok "TLS: $TS_HOSTNAME (certs provisioned)"
    else
      info "Provisioning TLS certificate for $TS_HOSTNAME..."
      if tailscale cert "$TS_HOSTNAME" 2>/dev/null; then
        mv "$TS_HOSTNAME.crt" "$TS_HOSTNAME.key" "$CONTINUUM_DATA/"
        ok "TLS enabled: https://$TS_HOSTNAME"
      else
        warn "TLS cert failed — Tailscale Starter plan (\$6/month) required for HTTPS"
        warn "Enable at: https://login.tailscale.com/admin/dns → HTTPS Certificates"
      fi
    fi
  fi
else
  warn "Tailscale not installed — no mesh networking or TLS"
  warn "Optional: https://tailscale.com/download"
fi

# ── 6. Pull images ─────────────────────────────────────────
info "Pulling container images..."
docker compose pull 2>/dev/null || warn "Some images not published yet — will build locally"

# ── 7. Start ───────────────────────────────────────────────
info "Starting Continuum..."

COMPOSE_ARGS=""
if [[ "$HAS_GPU" == "true" ]]; then
  COMPOSE_ARGS="--profile gpu"
fi

docker compose $COMPOSE_ARGS up -d

# ── 8. Wait for health ─────────────────────────────────────
info "Waiting for services..."
for i in {1..30}; do
  if curl -sf http://localhost:9003 &>/dev/null || curl -sf https://localhost:9003 -k &>/dev/null; then
    break
  fi
  [ $i -eq 30 ] && warn "Services still starting — check: docker compose logs"
  sleep 2
done

# ── 9. Determine URL + open browser ────────────────────────
if [ -n "$TS_HOSTNAME" ] && [ -f "$CONTINUUM_DATA/$TS_HOSTNAME.crt" ]; then
  URL="https://$TS_HOSTNAME:9003"
else
  URL="http://localhost:9003"
fi

case "$OS" in
  Darwin) open "$URL" 2>/dev/null || true ;;
  Linux)
    if grep -qi microsoft /proc/version 2>/dev/null; then
      cmd.exe /c start "" "$URL" 2>/dev/null || true
    else
      xdg-open "$URL" 2>/dev/null || true
    fi
    ;;
esac

# ── Done ────────────────────────────────────────────────────
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  Continuum is running"
echo ""
echo "  UI:      $URL"
echo ""
echo "  continuum          Open Continuum (from anywhere)"
echo "  continuum start    Start containers"
echo "  continuum stop     Stop containers"
echo "  continuum status   Show running state"
echo "  continuum open     Open browser"
echo ""
if [[ "$HAS_GPU" == "true" ]]; then
  echo "  GPU:     ${GPU_NAME:-detected}"
fi
if [ -n "$TS_HOSTNAME" ]; then
  echo "  Mesh:    https://$TS_HOSTNAME:9003"
fi
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
