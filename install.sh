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

# Container runtime + inference setup.
#
# Linux: Docker Engine + continuum-core runs containerized (with cuda/vulkan
# GPU passthrough via /dev/dri or runtime:nvidia). Everything in containers.
#
# Mac: Docker Desktop for support services ONLY. continuum-core runs NATIVELY
# on the host to access Metal for Candle embeddings, Bevy headless avatar
# render, vision processing, and audio MPS paths — Apple's hypervisor exposes
# no GPU to containers (Docker themselves confirmed in Feb 2026), so anything
# Metal-needing must be on the host. LLM inference routes to Docker Model
# Runner's vllm-metal backend, which also runs native on the host — Docker
# Desktop manages the process but the compute happens on Apple Silicon directly.
#
# CONTAINER_CMD is used for every `compose` / `info` call. On Mac that
# handles support services; continuum-core-server is launched separately
# as a native host process via `npm start`.
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
    if ! command -v docker &>/dev/null; then
      fail "Docker Desktop required on Mac.

  1. Download: https://docker.com/products/docker-desktop  (4.62+ for Model Runner)
  2. Install the .dmg, then launch Docker Desktop from Launchpad
  3. When prompted, grant Admin password for the vmnetd privileged helper
     (one-time macOS permission for container networking — standard Docker setup)
  4. Wait for the whale icon in your menu bar to show 'Docker Desktop is running'
  5. Re-run this install script
"
    fi
    if ! docker info &>/dev/null 2>&1; then
      fail "Docker Desktop is installed but not running.

  1. Launch Docker Desktop from Launchpad (or Applications)
  2. If this is your first launch, macOS will prompt for Admin password for the
     vmnetd privileged helper — click 'Allow' and enter your password
  3. Wait for the whale icon in your menu bar to show 'Docker Desktop is running'
  4. Re-run this install script

  (Scripted \`open -a Docker\` can't satisfy the macOS privileged-helper prompt —
   that's why this script asks you to launch Docker Desktop manually once.)
"
    fi
    # ── Docker Desktop VM memory — set to 80% of physical RAM ─────
    # Default Docker Desktop VM memory on Mac is ~6-8GB. For our workload
    # (Qwen 4-7B GGUF + vllm-metal + LiveKit + Bevy + multiple personas)
    # that OOM-kills containers mid-run. Calculate 80% of host RAM, set
    # it in Docker Desktop's settings-store.json, restart Docker Desktop
    # to pick up the new limit.
    #
    # Minimum floor: 16GB Docker VM. Below that, Continuum can't run
    # sanely — postgres + node-server + widget-server + livekit-bridge +
    # model-init + model cache collectively need that much just to breathe
    # alongside native continuum-core, Bevy, and sensory subsystems.
    # Physical RAM below 20GB means even 80% leaves macOS itself starving,
    # so we refuse install rather than ship a broken experience.
    #
    # Target assumption per Joel: 32GB+ M-series is the typical Continuum
    # user. 16GB MacBook Air is the entry-tier floor. Below 20GB physical
    # = hard refuse. Never set "stupidly low" numbers.
    PHYS_BYTES=$(sysctl -n hw.memsize)
    PHYS_MIB=$((PHYS_BYTES / 1048576))
    PHYS_GB=$((PHYS_MIB / 1024))

    if [[ "$PHYS_GB" -lt 20 ]]; then
      fail "This Mac has ${PHYS_GB}GB physical RAM. Continuum requires at least 20GB to run the full sensory stack (continuum-core native Metal + Docker Desktop VM + macOS itself). Entry-tier support starts at 20GB — ideally 32GB+ for comfortable concurrent-persona performance."
    fi

    # 80% of physical RAM, but never below 16GB (Docker VM floor).
    TARGET_MIB=$((PHYS_MIB * 80 / 100))
    if [[ "$TARGET_MIB" -lt 16384 ]]; then
      TARGET_MIB=16384
    fi

    CURRENT_MIB=$(docker system info --format '{{.MemTotal}}' 2>/dev/null | awk '{printf "%d\n", $1/1048576}')
    SETTINGS_FILE="$HOME/Library/Group Containers/group.com.docker/settings-store.json"
    # Bump if current is substantially below target (>10% gap — don't thrash
    # restarts over rounding noise).
    if [[ -f "$SETTINGS_FILE" ]] && [[ -n "$CURRENT_MIB" ]] && [[ "$CURRENT_MIB" -lt "$((TARGET_MIB * 90 / 100))" ]]; then
      info "Docker Desktop VM memory is ${CURRENT_MIB}MiB; bumping to ${TARGET_MIB}MiB (80% of ${PHYS_GB}GB host RAM, 16GB floor) for Continuum's inference + sensory workload…"
      python3 - <<PYEOF
import json, os
p = os.path.expanduser("$SETTINGS_FILE")
with open(p) as f:
    d = json.load(f)
d["MemoryMiB"] = $TARGET_MIB
with open(p, "w") as f:
    json.dump(d, f, indent=2)
PYEOF
      info "Restarting Docker Desktop to apply memory limit…"
      docker desktop restart >/dev/null 2>&1 || true
      # Wait for daemon to come back
      for _ in $(seq 1 30); do
        if docker info &>/dev/null 2>&1; then break; fi
        sleep 4
      done
      if ! docker info &>/dev/null 2>&1; then
        fail "Docker Desktop didn't come back after memory-limit restart. Launch it manually from Launchpad."
      fi
      NEW_MIB=$(docker system info --format '{{.MemTotal}}' 2>/dev/null | awk '{printf "%d\n", $1/1048576}')
      ok "Docker Desktop VM memory now ${NEW_MIB}MiB (target ${TARGET_MIB}MiB)"
    elif [[ -n "$CURRENT_MIB" ]]; then
      ok "Docker Desktop VM memory already ${CURRENT_MIB}MiB (≥ ${TARGET_MIB}MiB target)"
    fi

    # Docker Model Runner provides host-native vllm-metal for LLM inference.
    # Ships with Docker Desktop 4.62+. If `docker model` isn't available the
    # user's Docker Desktop is too old.
    if ! docker model --help &>/dev/null 2>&1; then
      fail "Docker Model Runner not available (needs Docker Desktop 4.62+).

  1. Open Docker Desktop → Settings → Software Updates → Check for updates
  2. Install the update (restart Docker Desktop if prompted)
  3. Re-run this install script
"
    fi
    # CLI-plugins directory — Model Runner installs its backend plugins here.
    # One-time sudo mkdir; ensure_sudo_warmed gives us a single-prompt warmup
    # that all remaining install steps reuse.
    if [ ! -d /usr/local/cli-plugins ]; then
      info "Creating /usr/local/cli-plugins for Docker Model Runner (one-time sudo)…"
      ensure_sudo_warmed
      sudo mkdir -p /usr/local/cli-plugins
    fi
    # Install the vllm-metal backend if not present. This downloads and
    # registers the host-native vllm binary that Docker Desktop orchestrates.
    # `docker model status` output lists each registered backend in a BACKEND
    # column with a STATUS column next to it. `list-runners` was the wrong
    # subcommand name (caught during M5 validation 2026-04-16).
    if ! docker model status 2>/dev/null | awk '/^vllm[[:space:]]+Running/{found=1} END{exit !found}'; then
      info "Installing vllm-metal runner (native Metal LLM inference on host)…"
      docker model install-runner --backend vllm
    fi
    # Rust toolchain — continuum-core-server is built natively on Mac (not
    # containerized) so it can link Metal for Candle embeddings, Bevy, vision,
    # and audio MPS paths. Build happens during `npm start` at end of install.
    if ! command -v cargo &>/dev/null; then
      info "Rust not found — installing via rustup (needed for native continuum-core build)…"
      curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
      # shellcheck disable=SC1091
      [ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"
    fi
    # Node toolchain — required to build TypeScript side + run launcher.
    if ! command -v node &>/dev/null; then
      if command -v brew &>/dev/null; then
        info "Node not found — installing via Homebrew…"
        brew install node
      else
        fail "Node.js required. Install from https://nodejs.org or via Homebrew, then re-run."
      fi
    fi
    CONTAINER_CMD=docker
    ;;
esac

# ── Per-service memory caps — auto-calculated from host RAM ────────
# Joel's directive: don't ask users to set mem limits; auto-calc from host.
# Don't paper over OOMs with undersized limits; size containers for the
# actual mission. Mission per-service budgets:
#
#   continuum-core (Linux container; on Mac it runs NATIVE and this cap
#   is informational / unused because docker-compose.mac.yml sets
#   replicas=0): needs to hold 4-8B param Qwen at Q4 (~4GB) + KV cache
#   for 5 concurrent personas (~2GB) + embeddings + Bevy + vision +
#   audio. Budget = host - 10GB (reserve for OS + Docker VM overhead
#   + support services).
#
#   livekit-bridge: native WebRTC encode/decode buffers, multiple
#   streams. Budget scales with host — roughly host/8.
#
#   node-server: TS orchestrator + IPC buffers + RAG state. Budget
#   same as livekit-bridge.
#
#   model-init: one-time downloader, fits in 2GB.
#
#   widget-server: static + light TS, 1GB.
#
#   postgres: our dataset is small, 512MB (already set in compose).
#   livekit server: 256m (already set in compose).
#
# Physical RAM is whichever host this runs on.
if [[ -n "${PHYS_MIB:-}" ]]; then
  # Mac branch set PHYS_MIB already. Linux sets it here from /proc/meminfo.
  :
elif [[ -f /proc/meminfo ]]; then
  PHYS_MIB=$(awk '/^MemTotal:/{print int($2/1024)}' /proc/meminfo)
fi

if [[ -n "${PHYS_MIB:-}" ]]; then
  PHYS_GB=$((PHYS_MIB / 1024))

  # continuum-core cap: (host - 10GB) on ≥32GB machines, (host - 8GB)
  # on 20-31GB machines. Floor at 10GB.
  if [[ $PHYS_GB -ge 32 ]]; then
    CONTINUUM_CORE_MEM=$((PHYS_GB - 10))g
  elif [[ $PHYS_GB -ge 20 ]]; then
    CONTINUUM_CORE_MEM=$((PHYS_GB - 8))g
  else
    CONTINUUM_CORE_MEM=10g
  fi

  # Scale livekit-bridge + node-server with host. Floor 2GB each.
  # 16GB host → 2g, 32GB → 4g, 64GB → 8g.
  SCALED=$((PHYS_GB / 8))
  [[ $SCALED -lt 2 ]] && SCALED=2
  LIVEKIT_BRIDGE_MEM=${SCALED}g
  NODE_SERVER_MEM=${SCALED}g

  # Static + small — these don't need to scale.
  MODEL_INIT_MEM=2g
  WIDGET_SERVER_MEM=1g

  export CONTINUUM_CORE_MEM LIVEKIT_BRIDGE_MEM NODE_SERVER_MEM MODEL_INIT_MEM WIDGET_SERVER_MEM

  info "Memory caps (${PHYS_GB}GB host): continuum-core=${CONTINUUM_CORE_MEM}, livekit-bridge=${LIVEKIT_BRIDGE_MEM}, node-server=${NODE_SERVER_MEM}, model-init=${MODEL_INIT_MEM}, widget-server=${WIDGET_SERVER_MEM}"
fi

# (OS-branch case/esac above handled Linux/Darwin and set CONTAINER_CMD.)
case "$OS" in
  Linux|Darwin) : ;;
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
  # Mac path — the docker-compose.mac.yml override sets continuum-core's
  # replicas to 0 so support services boot in containers but continuum-core
  # stays off. We run continuum-core NATIVELY on Mac (via `npm start` below)
  # so Candle embeddings, Bevy headless render, vision processing, and audio
  # MPS paths all get real Metal. LLM inference routes to Docker Model
  # Runner's vllm-metal backend — also host-native, no container GPU tax.
  if [ -f "docker-compose.mac.yml" ]; then
    COMPOSE_FILES="$COMPOSE_FILES -f docker-compose.mac.yml"
  else
    warn "docker-compose.mac.yml missing — Mac detected but override won't apply. Without it, docker compose would try to run continuum-core in a container, which on Mac means CPU-only for Candle/Bevy/vision."
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

# ── 7. Pull support-service images ─────────────────────────
# Image tag resolution: compose files honor ${CONTINUUM_IMAGE_TAG:-latest}.
# Main-branch installs (Carl's default) use :latest. Reviewers validating
# a PR before merge can pin the PR's staged image set:
#   CONTINUUM_IMAGE_TAG=pr-891 curl -fsSL install.sh | bash
# CI tags every PR build with pr-<number> (see .github/workflows/docker-images.yml).
# Merging to main promotes that image set to :latest, so main and :latest
# are always in sync by construction.
#
# On Mac: `continuum-core` is not pulled (replicas=0 in docker-compose.mac.yml);
# only support services (postgres, node-server, widget-server, livekit-bridge,
# model-init) are pulled. continuum-core runs natively from `npm start` below.
info "Pulling container images (tag: ${CONTINUUM_IMAGE_TAG:-latest})..."
$CONTAINER_CMD compose $COMPOSE_FILES $COMPOSE_ARGS pull 2>/dev/null || warn "Some images not published yet — will build locally"

# ── 8. Start support services ──────────────────────────────
info "Starting support services..."
$CONTAINER_CMD compose $COMPOSE_FILES $COMPOSE_ARGS up -d

# ── 8b. Start continuum-core natively on Mac ───────────────
# Mac runs continuum-core as a native host process so it can link Metal
# directly. `npm start` drives the full build (cargo build --release
# --features=metal + TS compile) and launches the server daemonized.
if [[ "$OS" == "Darwin" ]]; then
  info "Building + launching native continuum-core-server (Metal-enabled)..."
  info "  First run: cargo build takes 5-15 min. Subsequent runs: incremental."
  # CONTINUUM_CORE_TCP=9100 tells the native continuum-core-server to bind an
  # additional TCP listener alongside its Unix socket. Containerized
  # node-server (Option B Mac architecture) reaches the host-native
  # continuum-core via tcp://host.docker.internal:9100 because Unix sockets
  # don't traverse Docker Desktop's VM boundary on Mac. Native callers
  # (jtag CLI, continuum bin) keep using the Unix socket as before.
  export CONTINUUM_CORE_TCP=9100
  (cd "$INSTALL_DIR/src" && npm install --silent && npm start) || \
    warn "npm start failed — check logs at ~/.continuum/jtag/logs/system/continuum-core.log"
fi

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
