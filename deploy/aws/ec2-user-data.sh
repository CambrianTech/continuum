#!/bin/bash
# ec2-user-data.sh — one-shot headless Continuum on a fresh EC2 instance.
#
# Paste as EC2 user-data (runs as root at first boot) or run with sudo on a
# fresh box. Amazon Linux 2023 and Ubuntu 22.04/24.04 supported — AL2023 is
# NOT covered by get.docker.com (it bails), so we branch on the package
# manager instead of pretending one path fits ([[fallbacks-are-illegal-fail-loud]]).
#
# Result: docker + compose plugin installed, repo cloned to /opt/continuum,
# the containerized headless core up (CPU by default, CUDA overlay when an
# NVIDIA GPU is visible), surviving reboots via docker's restart policy.
# No Node on the host — the core is Rust in a container; package.json is a
# dev convenience, not a runtime dependency ([[rust-is-the-core-node-is-the-shell]]).
#
# Instance guidance (2026):
#   CPU fleet:  c7g/m7g (Graviton, arm64 image exists) or c7i/m7i (amd64)
#   GPU fleet:  g5/g6 (NVIDIA A10G/L4 → continuum-core-cuda overlay)
#   Disk:       models are big — 100GB gp3 minimum, 200GB comfortable.
set -euo pipefail

REPO="${CONTINUUM_REPO:-https://github.com/CambrianTech/continuum.git}"
BRANCH="${CONTINUUM_BRANCH:-main}"
DEST="${CONTINUUM_DEST:-/opt/continuum}"

log() { echo "[continuum-ec2] $*"; }

# ── 1. Docker + compose plugin, per-distro ──────────────────────────────────
if ! command -v docker >/dev/null 2>&1; then
  if command -v dnf >/dev/null 2>&1; then
    # Amazon Linux 2023 (and Fedora-family). get.docker.com does NOT support
    # AL2023 — dnf has docker but NOT the compose plugin, which we fetch from
    # Docker's release artifacts pinned to the current stable line.
    log "installing docker via dnf (Amazon Linux path)"
    dnf install -y docker git
    systemctl enable --now docker
    ARCH="$(uname -m)"  # x86_64 | aarch64 — matches Docker's release naming
    PLUG=/usr/local/lib/docker/cli-plugins
    mkdir -p "$PLUG"
    curl -fsSL \
      "https://github.com/docker/compose/releases/latest/download/docker-compose-linux-${ARCH}" \
      -o "$PLUG/docker-compose"
    chmod +x "$PLUG/docker-compose"
  elif command -v apt-get >/dev/null 2>&1; then
    log "installing docker via get.docker.com (Debian/Ubuntu path)"
    apt-get update -y && apt-get install -y git curl
    curl -fsSL https://get.docker.com | sh
    systemctl enable --now docker
  else
    echo "[continuum-ec2] FATAL: neither dnf nor apt-get — unsupported distro" >&2
    exit 1
  fi
fi
docker compose version >/dev/null || { echo "FATAL: compose plugin missing" >&2; exit 1; }

# ── 2. Clone (or update) the repo ───────────────────────────────────────────
if [ -d "$DEST/.git" ]; then
  git -C "$DEST" fetch origin "$BRANCH" && git -C "$DEST" checkout -q "$BRANCH" \
    && git -C "$DEST" pull -q --ff-only
else
  git clone -q --depth 1 -b "$BRANCH" "$REPO" "$DEST"
fi

# ── 3. Headless config: the substrate self-provisions under ~/.continuum ────
mkdir -p /root/.continuum
[ -f /root/.continuum/config.env ] || cat > /root/.continuum/config.env <<'EOF'
# Continuum headless EC2 — minimal seed; the substrate derives the rest.
CONTINUUM_HEADLESS=1
EOF

# ── 4. Up. CUDA overlay iff an NVIDIA device is actually visible ────────────
cd "$DEST"
OVERLAYS=()
if command -v nvidia-smi >/dev/null 2>&1 && nvidia-smi -L 2>/dev/null | grep -q GPU; then
  log "NVIDIA GPU detected — using the cuda overlay"
  OVERLAYS+=(-f docker-compose.yml -f docker-compose.gpu.yml)
else
  log "no NVIDIA GPU — CPU serving (Graviton/amd64 both fine)"
  OVERLAYS+=(-f docker-compose.yml)
fi
docker compose "${OVERLAYS[@]}" pull
docker compose "${OVERLAYS[@]}" up -d

log "up. Status: docker compose ps   Logs: docker compose logs -f continuum-core"
