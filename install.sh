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

# Container runtime. Linux gets Docker Engine (standard). Mac gets Podman +
# krunkit because Apple's hypervisor exposes no GPU to containers (Docker
# Desktop, Apple container, and other VMMs are all blocked — no IOMMU on
# Apple GPUs). krunkit is the only VMM that routes Vulkan API calls from
# the container out to MoltenVK on the host, giving Carl ~80% of native
# Metal throughput. That keeps the "always accelerated, never CPU
# fallback" directive intact.
#
# CONTAINER_CMD is used for every subsequent `compose` / `info` call, so
# the same script works with either runtime.
case "$OS" in
  Linux)
    if ! command -v docker &>/dev/null; then
      info "Docker not found — installing via get.docker.com…"
      curl -fsSL https://get.docker.com | sh
      sudo usermod -aG docker "$USER"
      warn "Added $USER to docker group — log out and back in, then re-run this script"
      exit 0
    fi
    CONTAINER_CMD=docker
    ;;
  Darwin)
    if ! command -v podman &>/dev/null; then
      if ! command -v brew &>/dev/null; then
        fail "Homebrew required to install Podman + krunkit. Install from https://brew.sh then re-run."
      fi
      info "Podman + krunkit not found — installing for Metal-backed Vulkan container…"
      brew tap slp/krunkit
      brew install podman krunkit
    fi
    if ! command -v krunkit &>/dev/null; then
      info "krunkit missing (needed for Vulkan→MoltenVK→Metal passthrough) — installing…"
      brew tap slp/krunkit 2>/dev/null || true
      brew install krunkit
    fi
    # Apple Silicon: Podman machine uses the libkrun provider so krunkit's
    # Vulkan passthrough is wired automatically. CPU/memory sized for a 4B
    # Q4_K_M model (~3GB VRAM) plus headroom for support services.
    if ! podman machine list --format '{{.Running}}' 2>/dev/null | grep -q true; then
      if ! podman machine list 2>/dev/null | grep -q podman-machine; then
        info "Initializing Podman machine (libkrun provider)…"
        CONTAINERS_MACHINE_PROVIDER=libkrun \
          podman machine init --cpus=8 --memory=16384 --rootful=false
      fi
      info "Starting Podman machine…"
      podman machine start
    fi
    CONTAINER_CMD=podman
    ;;
  *) fail "Unsupported OS: $OS" ;;
esac

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

# Real daemon check. On Linux this verifies Docker Engine is up (after the
# WSL integration module had a chance to fix it on Windows/WSL2 hosts);
# on Mac it verifies `podman machine start` above actually connected.
if ! $CONTAINER_CMD info &>/dev/null 2>&1; then
  case "$OS" in
    Darwin) fail "Podman machine not reachable. Run: podman machine start — then re-run this installer." ;;
    *)      fail "Docker daemon not reachable. Start Docker Desktop / Rancher Desktop and re-run." ;;
  esac
fi
ok "$CONTAINER_CMD $($CONTAINER_CMD version --format '{{.Client.Version}}' 2>/dev/null || echo 'ready')"
ok "Source: $INSTALL_DIR"

# ── 3b. Install continuum command (modular, headless-safe) ─
# Was an inline `sudo cp` that crashed on "no TTY for password" when the
# install ran headless (curl|bash without -t, BigMama SSH dry-run, CI).
# Now goes through mod_continuum_bin_link which routes to a user-space
# fallback (~/.local/bin) when sudo would prompt without a TTY.
mod_continuum_bin_link "$INSTALL_DIR/bin/continuum"

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

# ── 6. Pick compose files + profile ───────────────────────
# Base file is always loaded. On GPU hosts, layer docker-compose.gpu.yml
# so continuum-core picks up the cuda image override (otherwise compose
# silently uses the CPU image and inference falls back to CPU). The same
# -f set MUST be passed to both `pull` and `up`, or pull grabs base
# images while up tries to use override-named images that aren't local.
COMPOSE_FILES="-f docker-compose.yml"
COMPOSE_ARGS=""
if [[ "$OS" == "Darwin" ]]; then
  # Mac Carl path — layer the Vulkan-in-container override so the
  # continuum-core service picks up the vulkan image + /dev/dri passthrough
  # for krunkit's virtio-GPU. Without this override the base file pulls
  # the CPU-only continuum-core image, which violates the "always GPU"
  # directive.
  if [ -f "docker-compose.mac.yml" ]; then
    COMPOSE_FILES="$COMPOSE_FILES -f docker-compose.mac.yml"
  else
    warn "docker-compose.mac.yml missing — Mac detected but Vulkan override won't apply. Continuum would run CPU-only; aborting rather than ship a CPU inference path."
    fail "Fix: ensure you cloned with repository integrity — the Mac override file is part of the PR891 install architecture."
  fi
elif [[ "$HAS_GPU" == "true" ]]; then
  if [ -f "docker-compose.gpu.yml" ]; then
    COMPOSE_FILES="$COMPOSE_FILES -f docker-compose.gpu.yml"
  else
    warn "docker-compose.gpu.yml missing — GPU detected but cuda override won't apply. Continuing on CPU images."
  fi
  COMPOSE_ARGS="--profile gpu"
fi

# ── 7. Pull images ─────────────────────────────────────────
info "Pulling container images..."
$CONTAINER_CMD compose $COMPOSE_FILES $COMPOSE_ARGS pull 2>/dev/null || warn "Some images not published yet — will build locally"

# ── 8. Start ───────────────────────────────────────────────
info "Starting Continuum..."
$CONTAINER_CMD compose $COMPOSE_FILES $COMPOSE_ARGS up -d

# ── 8. Wait for health ─────────────────────────────────────
info "Waiting for services..."
for i in {1..30}; do
  if curl -sf http://localhost:9003 &>/dev/null || curl -sf https://localhost:9003 -k &>/dev/null; then
    break
  fi
  [ $i -eq 30 ] && warn "Services still starting — check: $CONTAINER_CMD compose logs"
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
