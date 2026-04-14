#!/bin/bash
# Continuum — One-command installer
# Usage: curl -fsSL https://cambriantech.github.io/continuum/install.sh | bash
#
# Docker-first: pulls pre-built images, no compilation needed.
# Optional: Tailscale for mesh networking + TLS (voice/video).
set -e

info()  { echo -e "\033[1;36m→\033[0m $*"; }
ok()    { echo -e "\033[1;32m✓\033[0m $*"; }
warn()  { echo -e "\033[1;33m!\033[0m $*"; }
fail()  { echo -e "\033[1;31m✗\033[0m $*"; exit 1; }

REPO="https://github.com/CambrianTech/continuum.git"
INSTALL_DIR="${CONTINUUM_DIR:-$HOME/continuum}"
CONTINUUM_DATA="$HOME/.continuum"

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  Continuum Installer"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# ── 0. Warm sudo cache up front ─────────────────────────────
# Matches src/scripts/install.sh pattern: ask once, use for the rest of
# the run (usermod on Linux, /usr/local/bin copy, Postgres/Tailscale
# follow-ups from nested scripts). Without this, later steps either
# re-prompt unexpectedly or fail silently on headless runs.
OS_EARLY="$(uname -s)"
if [ "$OS_EARLY" = "Linux" ] && [ "$(id -u)" -ne 0 ] && ! sudo -n true 2>/dev/null; then
  if [ -t 0 ]; then
    echo -e "\033[1;33m!\033[0m Some install steps need admin access — prompting once up front so nothing re-prompts later."
    sudo -v
    # Keep sudo alive while this installer runs (refresh timestamp every
    # 50s). Dies with the parent when install.sh exits.
    ( while true; do sudo -n true 2>/dev/null; sleep 50; done ) &
    SUDO_KEEPALIVE_PID=$!
    trap 'kill "$SUDO_KEEPALIVE_PID" 2>/dev/null || true' EXIT
  fi
fi

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

# ── 2. Docker ───────────────────────────────────────────────
if ! command -v docker &>/dev/null; then
  info "Docker not found"
  case "$OS" in
    Linux)
      info "Installing Docker..."
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

# Detect WSL2 + Docker Desktop with broken integration before we trust
# `docker info`. Symptom: Docker Desktop is running on the Windows side
# (shared-sockets mounts exist), the current distro is in
# IntegratedWslDistros, but /var/run/docker.sock isn't materialized in
# this distro and the CLI fails "no such file". Standard Docker Desktop
# integration setup wants a manual toggle — we auto-enable it instead.
fix_wsl_docker_desktop_integration() {
  # Only bother on WSL2 with Docker Desktop shared mount present.
  grep -qi microsoft /proc/version 2>/dev/null || return 1
  [ -d /mnt/wsl/docker-desktop ] || return 1

  local distro="${WSL_DISTRO_NAME:-$(grep '^NAME=' /etc/os-release | cut -d\" -f2 | head -1)}"
  [ -n "$distro" ] || return 1

  # Find the Windows user's Docker Desktop settings file. Docker Desktop
  # stores the list of integrated distros in a per-user JSON file at
  # C:\Users\<user>\AppData\Roaming\Docker\settings-store.json. We just
  # need any settings file whose IntegratedWslDistros we can update.
  local settings
  settings=$(ls /mnt/c/Users/*/AppData/Roaming/Docker/settings-store.json 2>/dev/null | head -1)
  [ -n "$settings" ] && [ -w "$settings" ] || return 1

  info "Docker Desktop detected; ensuring WSL integration for '$distro'…"
  # Add distro to IntegratedWslDistros if not already present. Python3 is
  # more reliable than jq in guaranteeing JSON round-trip preserves
  # Docker's formatting.
  python3 - "$settings" "$distro" <<'PY'
import json, sys
path, distro = sys.argv[1], sys.argv[2]
with open(path) as f:
    cfg = json.load(f)
distros = cfg.setdefault("IntegratedWslDistros", [])
if distro not in distros:
    distros.append(distro)
    with open(path, "w") as f:
        json.dump(cfg, f, indent=2)
    print(f"enabled {distro}", flush=True)
else:
    print(f"already enabled {distro}", flush=True)
PY

  # Bounce Docker Desktop so the setting takes effect (hook-installs
  # /var/run/docker.sock into our distro on restart). Non-fatal if the
  # shutdown fails — we'll surface the socket check either way.
  local pwsh="/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe"
  if [ -x "$pwsh" ]; then
    "$pwsh" -Command 'Stop-Process -Name "Docker Desktop" -Force -ErrorAction SilentlyContinue; Start-Sleep 2; Start-Process -FilePath "C:\\Program Files\\Docker\\Docker\\Docker Desktop.exe"' >/dev/null 2>&1 || true
  fi

  # Poll for the socket (up to 60s). Clean output while we wait.
  for i in $(seq 1 30); do
    [ -S /var/run/docker.sock ] && { ok "WSL integration active"; return 0; }
    sleep 2
  done
  return 1
}

if ! docker info &>/dev/null 2>&1; then
  if fix_wsl_docker_desktop_integration; then
    # Integration came up — reconfirm.
    docker info &>/dev/null 2>&1 \
      || fail "Docker WSL integration enabled but daemon still unreachable. Try: wsl --shutdown (from Windows PowerShell), then re-run."
  else
    fail "Docker installed but not running. Start Docker Desktop/Rancher Desktop and re-run."
  fi
fi

ok "Docker $(docker version --format '{{.Client.Version}}' 2>/dev/null || echo 'ready')"

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

# Vendored substrates (llama.cpp, whisper.cpp) live as submodules. The
# Dockerfiles fail fast if these aren't populated, so we just init them
# here — zero onboarding tax. Safe on update runs too: git submodule
# update is a no-op when submodules are already at the pinned commit.
git submodule update --init --recursive 2>&1 | grep -vE '^(Submodule.*registered|Cloning into)' || true

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
