#!/bin/bash
# Continuum — FROM-SOURCE developer/self-host build.
#
# ┌───────────────────────────────────────────────────────────────────────┐
# │  MOST USERS DO NOT RUN THIS. To just *use* Continuum, run the one-      │
# │  command installer (pre-built Docker images, no compiler needed):      │
# │    • Windows:      irm https://raw.githubusercontent.com/CambrianTech/continuum/main/install.ps1 | iex
# │    • Linux/macOS:  curl -fsSL https://raw.githubusercontent.com/CambrianTech/continuum/main/install.sh | bash
# │  (the github.io URLs these replaced 404 — Pages is not published; verified 2026-08-07)
# │  On Windows that handles WSL2 + Docker + GPU for you — no shell choice. │
# └───────────────────────────────────────────────────────────────────────┘
#
# This script is the FROM-SOURCE path: compiles continuum-core + workers and
# installs the full dev toolchain. Use it only if you are building/hacking on
# Continuum itself.
#
# Usage (must be a real Linux userland — WSL2/Ubuntu, native Linux, or macOS;
# NOT Git Bash/MSYS, which has no apt/toolchain and is rejected up front):
#   git clone https://github.com/CambrianTech/continuum.git
#   cd continuum && bash tools/scripts/install.sh
#
# Installs: Node.js, Rust (pinned via rust-toolchain.toml), Python venv (+ ML
#   packages and the CUDA toolkit when a GPU is detected), system deps.
# Idempotent: safe to re-run (skips what's already installed).

set -eo pipefail

# Don't run as root — the script uses sudo only where needed (apt install).
# Running as root puts config/venv under /root instead of $HOME.
if [ "$(id -u)" -eq 0 ] && [ -n "$SUDO_USER" ]; then
  echo "Don't run with sudo. The script elevates for apt only."
  echo "Usage: bash scripts/install.sh"
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

source "$SCRIPT_DIR/shared/preflight.sh"
# Shared primitives (logs, module_*, ensure_sudo_warmed) — see
# docs/infrastructure/INSTALL-ARCHITECTURE.md for the module contract.
source "$SCRIPT_DIR/lib/install-common.sh"

cd "$PROJECT_DIR"

echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}  Continuum Tower Install${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

PLATFORM=$(preflight_detect_platform)
echo -e "  Platform: ${GREEN}${PLATFORM}${NC}"

# Fail fast on an unsupported host shell. The from-source dev install needs a
# real Linux userland (apt + the build toolchain). Git Bash / MSYS / Cygwin are
# Windows shells with no package manager, so the Node/npm/Rust steps would
# cascade into "command not found" — better to stop with a clear redirect.
case "$PLATFORM" in
  windows-shell)
    echo -e "  ${RED}✗ This is Git Bash / MSYS on Windows — not a Linux environment.${NC}"
    echo -e "  ${YELLOW}Continuum's from-source install needs WSL2 Ubuntu. Either:${NC}"
    echo -e "  ${YELLOW}  • open WSL ('wsl') and run this from your WSL checkout, OR${NC}"
    echo -e "  ${YELLOW}  • use the Docker-first Windows installer: install.ps1${NC}"
    exit 1
    ;;
  unknown)
    echo -e "  ${RED}✗ Unsupported platform ($(uname -s)). This installer supports${NC}"
    echo -e "  ${YELLOW}  WSL2/Linux and macOS. On Windows use WSL2 or install.ps1.${NC}"
    exit 1
    ;;
esac

# ============================================================================
# Modular install steps (new pattern — see INSTALL-ARCHITECTURE.md)
# ============================================================================
# Module functions live in tools/scripts/lib/install-common.sh (already sourced
# above). Each mod_* is idempotent self-guarded; steps needing sudo call
# ensure_sudo_warmed so the password is prompted at most ONCE per run.
#
# Running here: the modules that apply to both Carl's curl path and Dev's
# local-build path. Platform/applicability guards inside each module make
# them safe no-ops where they don't apply.
mod_submodules_init
mod_docker_wsl_integration

# Cold storage: auto-detect a large drive and route models + build cache there
# (migrating what's on the home fs) BEFORE any cargo build, so the build uses the
# relocated CARGO_TARGET_DIR. No-op on single-drive machines. Reconfigurable via
# ~/.continuum/config.env. (Windows twin: Mod-ColdStorage in win-modules.ps1.)
mod_cold_storage

# ============================================================================
# GPU detection
# ============================================================================

HAS_CUDA=false
HAS_METAL=false
GPU_NAME=""

detect_gpu() {
  case "$PLATFORM" in
    macos)
      # All Apple Silicon Macs have Metal
      if sysctl -n machdep.cpu.brand_string 2>/dev/null | grep -qi "apple"; then
        HAS_METAL=true
        GPU_NAME="Apple Silicon (Metal)"
      fi
      ;;
    linux|wsl)
      # Check for NVIDIA GPU via nvidia-smi (WSL path differs)
      local smi=""
      if command -v nvidia-smi &>/dev/null; then
        smi="nvidia-smi"
      elif [ -f /usr/lib/wsl/lib/nvidia-smi ]; then
        smi="/usr/lib/wsl/lib/nvidia-smi"
      fi
      if [ -n "$smi" ]; then
        GPU_NAME=$($smi --query-gpu=name --format=csv,noheader 2>/dev/null | head -1)
        if [ -n "$GPU_NAME" ]; then
          HAS_CUDA=true
        fi
      fi
      ;;
  esac

  if $HAS_CUDA; then
    # Re-derive smi path for VRAM query (local was scoped to case block)
    local smi_path="nvidia-smi"
    [ -f /usr/lib/wsl/lib/nvidia-smi ] && smi_path="/usr/lib/wsl/lib/nvidia-smi"
    local vram=$($smi_path --query-gpu=memory.total --format=csv,noheader,nounits 2>/dev/null | head -1)
    echo -e "  GPU:      ${GREEN}${GPU_NAME} (CUDA, ${vram}MiB VRAM)${NC}"
    # nvcc (CUDA compiler) is REQUIRED to build `--features cuda` — candle-kernels
    # + cudarc compile GPU kernels at build time. `install_cuda_toolkit` (below)
    # provisions it when missing/too-old. A Blackwell GPU (sm_120, e.g. RTX 5090)
    # needs CUDA >= 12.8; without nvcc the build falls back to CPU.
    if command -v nvcc &>/dev/null; then
      echo -e "  CUDA:     ${GREEN}nvcc $(nvcc --version 2>/dev/null | grep -oP 'release \K[0-9.]+' | head -1) present${NC}"
    else
      echo -e "  CUDA:     ${YELLOW}nvcc not found — will install CUDA toolkit (required for GPU inference)${NC}"
    fi
  elif $HAS_METAL; then
    echo -e "  GPU:      ${GREEN}${GPU_NAME}${NC}"
  else
    echo -e "  GPU:      ${YELLOW}None detected (CPU-only mode)${NC}"
  fi
}

detect_gpu
echo ""

# ============================================================================
# CUDA toolkit (nvcc) — provisioned as a detected prerequisite
# ============================================================================
# Modern-app behavior: a CUDA GPU is useless for GPU inference without nvcc
# (the build needs it to compile candle-kernels/cudarc), so if one is present
# and the toolkit is missing or below the Blackwell floor, install it. Fully
# idempotent — a re-run/update skips when a recent-enough toolkit exists.
install_cuda_toolkit() {
  $HAS_CUDA || return 0
  case "$PLATFORM" in linux|wsl) ;; *) return 0 ;; esac

  # Blackwell (sm_120 / RTX 5090) needs >= 12.8; 12.9 is the newest 12.x with
  # broad cudarc/candle support. Bump TARGET as the toolchain validates newer.
  local MIN="12.8" TARGET="12-9"
  if command -v nvcc &>/dev/null; then
    local cur=$(nvcc --version 2>/dev/null | grep -oP 'release \K[0-9]+\.[0-9]+' | head -1)
    if [ -n "$cur" ] && [ "$(printf '%s\n%s\n' "$MIN" "$cur" | sort -V | head -1)" = "$MIN" ]; then
      echo -e "  ${GREEN}✅ CUDA toolkit $cur already present (>= $MIN) — skipping${NC}"
      return 0
    fi
    echo -e "  ${YELLOW}nvcc ${cur:-unknown} is below the Blackwell floor $MIN — upgrading${NC}"
  fi

  # Tiered sudo — same one-prompt contract as install_system_deps.
  local SUDO="" CAN_SUDO=true
  if [ "$(id -u)" -eq 0 ]; then SUDO=""
  elif sudo -n true 2>/dev/null; then SUDO="sudo"
  elif [ -t 0 ]; then SUDO="sudo"
  else CAN_SUDO=false; fi
  if ! $CAN_SUDO; then
    echo -e "  ${RED}CUDA toolkit needed but no terminal for sudo. Re-run in a terminal:${NC}"
    echo -e "  ${YELLOW}  bash tools/scripts/install.sh${NC}"
    return 0
  fi
  ensure_sudo_warmed

  # NVIDIA CUDA apt repo. wsl-ubuntu for WSL2 (the GPU driver is the Windows
  # host driver passed through — NEVER install a Linux driver under WSL);
  # ubuntu2404 for native Linux. sbsa for aarch64.
  local distro="ubuntu2404"
  [ "$PLATFORM" = "wsl" ] && distro="wsl-ubuntu"
  local cuda_arch="x86_64"
  [ "$(uname -m)" = "aarch64" ] && cuda_arch="sbsa"
  local keyring="/tmp/cuda-keyring.deb"
  echo -e "  Installing CUDA toolkit ${TARGET//-/.} (~3GB) from NVIDIA ${distro} repo..."
  if curl -fsSL -o "$keyring" \
      "https://developer.download.nvidia.com/compute/cuda/repos/${distro}/${cuda_arch}/cuda-keyring_1.1-1_all.deb"; then
    $SUDO dpkg -i "$keyring" >/dev/null 2>&1
    rm -f "$keyring"
    $SUDO apt-get update -qq
    if $SUDO apt-get install -y "cuda-toolkit-${TARGET}"; then
      echo -e "  ${GREEN}✅ CUDA toolkit installed -> /usr/local/cuda-${TARGET//-/.}/bin/nvcc${NC}"
      echo -e "  ${YELLOW}     (cargo-features.sh adds /usr/local/cuda/bin to PATH for --features cuda)${NC}"
    else
      echo -e "  ${YELLOW}⚠️ CUDA toolkit install failed — GPU inference falls back to CPU until resolved${NC}"
    fi
  else
    echo -e "  ${YELLOW}⚠️ Could not fetch NVIDIA cuda-keyring — skipping CUDA toolkit (CPU fallback)${NC}"
  fi
}

# ============================================================================
# Step 1: System dependencies
# ============================================================================

echo -e "${YELLOW}[1/9] System dependencies${NC}"

install_system_deps() {
  case "$PLATFORM" in
    macos)
      preflight_check_build_tools
      # Homebrew packages
      for pkg in jq git; do
        if ! command -v "$pkg" &>/dev/null; then
          echo -e "  Installing $pkg..."
          brew install "$pkg"
        fi
      done
      # ONNX Runtime — required for Silero VAD (live mode speech-to-text)
      if ! brew list onnxruntime &>/dev/null; then
        echo -e "  Installing onnxruntime (for live mode STT)..."
        brew install onnxruntime
      fi
      ;;
    linux|wsl)
      # ── Tiered sudo: auto if root/passwordless, prompt if interactive, skip if headless ──
      local SUDO=""
      local CAN_SUDO=true
      if [ "$(id -u)" -eq 0 ]; then
        SUDO=""  # already root
      elif sudo -n true 2>/dev/null; then
        SUDO="sudo"  # passwordless sudo available
      elif [ -t 0 ]; then
        SUDO="sudo"  # stdin is a real terminal — sudo can prompt
      else
        CAN_SUDO=false  # non-interactive (SSH pipe, CI, curl|bash without /dev/tty)
      fi

      # Essential build tools + deps
      local needed=()
      command -v gcc &>/dev/null || needed+=("build-essential")
      command -v pkg-config &>/dev/null || needed+=("pkg-config")
      command -v jq &>/dev/null || needed+=("jq")
      command -v curl &>/dev/null || needed+=("curl")
      command -v git &>/dev/null || needed+=("git")
      # Python3 (may be missing on minimal images)
      command -v python3 &>/dev/null || needed+=("python3")
      # Python venv support
      if ! python3 -m venv --help &>/dev/null 2>&1; then
        # Detect python version for correct package name
        local pyver=$(python3 -c "import sys; print(f'{sys.version_info.major}.{sys.version_info.minor}')" 2>/dev/null || echo "3.12")
        needed+=("python${pyver}-venv")
      fi
      # Dev libraries needed by Rust crates
      pkg-config --exists openssl 2>/dev/null || needed+=("libssl-dev")
      pkg-config --exists glib-2.0 2>/dev/null || needed+=("libglib2.0-dev")
      pkg-config --exists alsa 2>/dev/null || needed+=("libasound2-dev")
      # cmake needed by native build scripts (whisper-rs, livekit)
      command -v cmake &>/dev/null || needed+=("cmake")
      # libclang needed by bindgen (whisper-rs-sys, livekit FFI)
      [ -f /usr/lib/llvm-*/lib/libclang.so ] || needed+=("libclang-dev")
      # protobuf compiler needed by tonic-prost (gRPC code gen)
      command -v protoc &>/dev/null || needed+=("protobuf-compiler")
      # libva needed by webrtc-sys (LiveKit video)
      pkg-config --exists libva 2>/dev/null || needed+=("libva-dev")
      # Vulkan (required for Bevy GPU rendering — without it wgpu falls back to llvmpipe)
      dpkg -l libvulkan1 2>/dev/null | grep -q '^ii' || needed+=("libvulkan1")
      # NVIDIA Vulkan ICD — the GL package provides the Vulkan driver + ICD manifest.
      # Must match the installed compute driver version (e.g. libnvidia-gl-535).
      if command -v nvidia-smi &>/dev/null || [ -d /usr/lib/wsl/lib ]; then
        if ! [ -f /usr/share/vulkan/icd.d/nvidia_icd.json ]; then
          local nv_ver=$(dpkg -l 'libnvidia-compute-*' 2>/dev/null | awk '/^ii/{print $2}' | grep -oP '\d+' | head -1)
          if [ -n "$nv_ver" ]; then
            needed+=("libnvidia-gl-${nv_ver}")
          else
            needed+=("libnvidia-gl-535")
          fi
        fi
      fi

      # ONNX Runtime — installed to user-space below (no apt package exists).
      # Do NOT add to needed[] — it's handled by the GitHub release download.

      if [ ${#needed[@]} -gt 0 ]; then
        if $CAN_SUDO; then
          echo -e "  Installing: ${needed[*]}"
          # One-prompt-contract: ensure_sudo_warmed prompts ONCE if needed,
          # arms keepalive, every subsequent sudo within the run is silent.
          # See docs/infrastructure/INSTALL-ARCHITECTURE.md.
          ensure_sudo_warmed
          $SUDO apt-get update -qq
          $SUDO apt-get install -y "${needed[@]}"
        else
          echo -e "  ${RED}Missing system packages: ${needed[*]}${NC}"
          echo -e "  ${RED}Cannot prompt for password (no terminal). Run this in a terminal:${NC}"
          echo -e "  ${YELLOW}  sudo apt-get install -y ${needed[*]}${NC}"
          echo -e "  ${RED}Then re-run this script.${NC}"
          exit 1
        fi
      fi

      # ONNX Runtime — required for Silero VAD (voice activity detection)
      # Installed to ~/.continuum/lib/ (no sudo needed, user-space only)
      # The ort crate finds it via ORT_DYLIB_PATH env var (set in start scripts)
      local ORT_LIB_DIR="$HOME/.continuum/lib"
      if [ ! -f "$ORT_LIB_DIR/libonnxruntime.so" ]; then
        local ORT_VERSION="1.23.0"
        local ORT_ARCH
        case "$(uname -m)" in
          x86_64)  ORT_ARCH="x64" ;;
          aarch64) ORT_ARCH="aarch64" ;;
          *)       ORT_ARCH="x64" ;;
        esac
        local ORT_VARIANT="linux-${ORT_ARCH}"
        if command -v nvidia-smi &>/dev/null || [ -f /usr/lib/wsl/lib/nvidia-smi ]; then
          ORT_VARIANT="linux-${ORT_ARCH}-gpu"
        fi
        local ORT_URL="https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VERSION}/onnxruntime-${ORT_VARIANT}-${ORT_VERSION}.tgz"
        echo -e "  Installing ONNX Runtime ${ORT_VERSION} (${ORT_VARIANT}) to ~/.continuum/lib/..."
        local ORT_TMP="/tmp/onnxruntime-install"
        rm -rf "$ORT_TMP"
        mkdir -p "$ORT_TMP" "$ORT_LIB_DIR"
        if curl -sSL "$ORT_URL" | tar xz -C "$ORT_TMP" --strip-components=1; then
          cp -a "$ORT_TMP"/lib/libonnxruntime* "$ORT_LIB_DIR/"
          echo -e "  ${GREEN}✅ ONNX Runtime installed to ~/.continuum/lib/${NC}"
        else
          echo -e "  ${YELLOW}⚠️ ONNX Runtime download failed — VAD will be unavailable${NC}"
        fi
        rm -rf "$ORT_TMP"
      fi
      ;;
  esac
  echo -e "  ${GREEN}✅ System deps OK${NC}"
}

install_system_deps

# Provision the CUDA toolkit if a CUDA GPU was detected (no-op otherwise,
# idempotent on re-run). Runs after system deps so apt + curl are ready.
install_cuda_toolkit

# CONTINUUM_DEPS_ONLY=1 — called by npm start to check deps without full build.
# Still installs all infrastructure (Node, Rust, Python, Postgres, LiveKit, Tailscale)
# but skips the build step (npm install, tsc, cargo build).
if [ "${CONTINUUM_DEPS_ONLY:-0}" = "1" ]; then
  # Jump past the build step to continue with infrastructure installs
  SKIP_BUILD=1
else
  SKIP_BUILD=0
fi

# ============================================================================
# Step 2: Node.js
# ============================================================================

echo -e "${YELLOW}[2/9] Node.js${NC}"

install_node() {
  if command -v node &>/dev/null; then
    local ver=$(node --version)
    echo -e "  ${GREEN}✅ Node.js ${ver} already installed${NC}"
    return
  fi

  case "$PLATFORM" in
    macos)
      echo -e "  Installing via Homebrew..."
      brew install node
      ;;
    linux|wsl)
      echo -e "  Installing Node.js 22 via nvm..."
      if [ ! -d "$HOME/.nvm" ]; then
        curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.3/install.sh | bash
      fi
      export NVM_DIR="$HOME/.nvm"
      [ -s "$NVM_DIR/nvm.sh" ] && . "$NVM_DIR/nvm.sh"
      nvm install 22
      nvm use 22
      ;;
  esac
  # Verify the install actually put node on PATH — don't claim success on a
  # bare "node: command not found" (e.g. nvm sourced into a subshell that
  # didn't persist, or an unsupported shell). Fail loud and actionable.
  if ! command -v node &>/dev/null; then
    echo -e "  ${RED}✗ Node.js install ran but 'node' is still not on PATH.${NC}"
    echo -e "  ${YELLOW}  Open a new shell (or 'source ~/.nvm/nvm.sh') and re-run,${NC}"
    echo -e "  ${YELLOW}  or install Node 22 via your platform's package manager.${NC}"
    exit 1
  fi
  echo -e "  ${GREEN}✅ Node.js $(node --version) installed${NC}"
}

install_node

# ============================================================================
# Step 3: Rust
# ============================================================================

echo -e "${YELLOW}[3/9] Rust${NC}"

install_rust() {
  if command -v rustc &>/dev/null; then
    echo -e "  ${GREEN}✅ Rust $(rustc --version | awk '{print $2}') already installed${NC}"
    return
  fi

  echo -e "  Installing via rustup..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  source "$HOME/.cargo/env"
  echo -e "  ${GREEN}✅ Rust $(rustc --version | awk '{print $2}') installed${NC}"
}

install_rust
export PATH="$HOME/.cargo/bin:$PATH"

# ============================================================================
# Step 4: Python ML environment (if GPU detected)
# ============================================================================

echo -e "${YELLOW}[4/9] Python ML environment${NC}"

VENV_DIR="$HOME/.continuum/venv"

install_python_ml() {
  if [ -f "$VENV_DIR/bin/python3" ]; then
    echo -e "  ${GREEN}✅ Python venv exists at $VENV_DIR${NC}"
    # Check if key packages are installed
    if "$VENV_DIR/bin/python3" -c "import torch, transformers, peft" 2>/dev/null; then
      echo -e "  ${GREEN}✅ ML packages already installed${NC}"
      return
    fi
    echo -e "  Upgrading ML packages..."
  else
    echo -e "  Creating venv at $VENV_DIR..."
    python3 -m venv "$VENV_DIR"
  fi

  local pip="$VENV_DIR/bin/pip"

  if $HAS_CUDA; then
    echo -e "  Installing PyTorch (CUDA)..."
    $pip install -q torch --index-url https://download.pytorch.org/whl/cu128
  elif $HAS_METAL; then
    echo -e "  Installing PyTorch (Metal/MPS)..."
    $pip install -q torch
  else
    echo -e "  Installing PyTorch (CPU)..."
    $pip install -q torch --index-url https://download.pytorch.org/whl/cpu
  fi

  echo -e "  Installing ML packages..."
  local ml_pkgs="transformers peft accelerate datasets trl pytest"
  # bitsandbytes is CUDA-only — fails on macOS/MPS
  if $HAS_CUDA; then
    ml_pkgs="$ml_pkgs bitsandbytes"
  fi
  $pip install -q $ml_pkgs

  echo -e "  ${GREEN}✅ Python ML environment ready${NC}"
}

if $HAS_CUDA || $HAS_METAL; then
  install_python_ml
else
  echo -e "  ${YELLOW}Skipped (no GPU detected). Run with GPU for training/inference.${NC}"
fi

# ============================================================================
# Step 5: npm install + Rust build
# ============================================================================

if [ "$SKIP_BUILD" = "0" ]; then
  echo -e "${YELLOW}[5/9] Building Continuum${NC}"

  # Preflight: the build needs npm. Without this check, a missing npm only
  # surfaces as "npm: command not found" swallowed by the `| tail` pipe, and
  # the install appears to continue. Stop loud and actionable instead.
  if ! command -v npm &>/dev/null; then
    echo -e "  ${RED}✗ npm not found — cannot build. Node.js/npm must be installed${NC}"
    echo -e "  ${YELLOW}  and on PATH (the [2/8] Node.js step provides them on a${NC}"
    echo -e "  ${YELLOW}  supported platform). Open a fresh shell and re-run.${NC}"
    exit 1
  fi

  echo -e "  Installing npm dependencies..."
  npm install --silent 2>&1 | tail -3
  # PIPESTATUS[0] is npm's real exit code (the pipe's own status is tail's).
  if [ "${PIPESTATUS[0]}" -ne 0 ]; then
    echo -e "  ${RED}✗ npm install failed${NC}"
    exit 1
  fi

  # The old `build:ts` / `build:cli` scripts left with the retired Node shell
  # (moved to legacy/src, #1840) — they no longer exist in the root package.json.
  # Calling them under `set -eo pipefail` made `npm run` exit non-zero ("Missing
  # script") and ABORTED the whole native install (install-audit FATAL). The
  # current TS deliverables are the client SDK + web app; build them BEST-EFFORT
  # in an `if` (so a client-build failure can never abort a headless-core install —
  # the Rust core below is the deliverable). The `if` condition also shields the
  # pipeline from `set -e`.
  echo -e "  Building clients (SDK + web, best-effort)..."
  if npm run build:clients 2>&1 | tail -3; then
    echo -e "  ${GREEN}✅ clients built${NC}"
  else
    echo -e "  ${YELLOW}⚠  client build skipped/failed — headless core install continues${NC}"
  fi

  echo -e "  Building Rust core + workers..."
  bash "$SCRIPT_DIR/setup-rust.sh" 2>&1 | tail -5
fi

# ============================================================================
# Config
# ============================================================================

CONFIG_DIR="$HOME/.continuum"
CONFIG_FILE="$CONFIG_DIR/config.env"
mkdir -p "$CONFIG_DIR"

# Auto-create persona SQLite directories (personas store state in per-persona SQLite DBs)
mkdir -p "$CONFIG_DIR/personas"

# Ensure bin directory exists (LiveKit binary, future CLI tools)
mkdir -p "$CONFIG_DIR/bin"

# ============================================================================
# PostgreSQL
# ============================================================================

echo -e "${YELLOW}[6/9] PostgreSQL${NC}"

install_postgres() {
  # macOS: keg-only postgres may not be in PATH — find it
  if [ "$PLATFORM" = "macos" ] && ! command -v psql &>/dev/null; then
    for keg in /opt/homebrew/opt/postgresql@{16,17,15}/bin; do
      if [ -x "$keg/psql" ]; then
        export PATH="$keg:$PATH"
        break
      fi
    done
  fi

  # Fast path: if we can already connect to the continuum database, skip everything.
  # No sudo, no brew upgrade, no postgres user switch, no trust auth check.
  if psql -d continuum -c "SELECT 1" &>/dev/null; then
    echo -e "  ${GREEN}✅ PostgreSQL ready (continuum database accessible)${NC}"
    return
  fi

  if command -v psql &>/dev/null; then
    echo -e "  ${GREEN}✅ PostgreSQL already installed${NC}"
  else
    case "$PLATFORM" in
      macos) brew install postgresql@16 && brew services start postgresql@16 ;;
      linux|wsl)
        # One-prompt-contract: warm sudo cache once for all postgres ops below.
        ensure_sudo_warmed
        sudo apt-get install -y postgresql postgresql-client
        sudo service postgresql start 2>/dev/null || sudo pg_ctlcluster 16 main start 2>/dev/null || true
        ;;
    esac
    echo -e "  ${GREEN}✅ PostgreSQL installed${NC}"
  fi

  # Trust-auth + createuser logic is Linux-only. On macOS, Homebrew postgres
  # accepts local connections without auth by default and runs as the
  # invoking user — `createdb continuum` works directly with no sudo. The
  # earlier unconditional `ensure_sudo_warmed` here was the failure mode
  # that broke `npm start` when stdin wasn't a TTY (parallel-start.sh
  # invokes this with CONTINUUM_DEPS_ONLY=1 every restart).
  case "$PLATFORM" in
    macos)
      # Homebrew postgres: peer auth, user is whoever started brew services.
      # Just ensure the continuum database exists.
      if ! psql -lqt 2>/dev/null | cut -d \| -f 1 | grep -qw continuum; then
        createdb continuum 2>/dev/null || true
      fi
      ;;
    linux|wsl)
      # apt-installed postgres needs trust-auth + role setup, all sudo.
      ensure_sudo_warmed
      local pg_hba=$(sudo -u postgres psql -t -c "SHOW hba_file" 2>/dev/null | tr -d ' ')
      if [ -n "$pg_hba" ] && [ -f "$pg_hba" ]; then
        if grep -q "scram-sha-256" "$pg_hba" 2>/dev/null; then
          sudo sed -i 's/scram-sha-256/trust/g' "$pg_hba"
          sudo service postgresql restart 2>/dev/null || sudo pg_ctlcluster 16 main restart 2>/dev/null || true
        fi
      fi
      local pg_user="${USER:-postgres}"
      if ! sudo -u postgres psql -tc "SELECT 1 FROM pg_roles WHERE rolname='$pg_user'" 2>/dev/null | grep -q 1; then
        sudo -u postgres createuser -s "$pg_user" 2>/dev/null || true
      fi
      if ! psql -lqt 2>/dev/null | cut -d \| -f 1 | grep -qw continuum; then
        createdb continuum 2>/dev/null || sudo -u postgres createdb continuum 2>/dev/null || true
      fi
      ;;
  esac
  echo -e "  ${GREEN}✅ Database 'continuum' ready${NC}"
}

install_postgres

# ============================================================================
# LiveKit SFU server (voice/video calls)
# ============================================================================

echo -e "${YELLOW}[7/9] LiveKit SFU${NC}"

install_livekit() {
  if [ -f "$PROJECT_DIR/workers/livekit-server" ] || command -v livekit-server &>/dev/null; then
    echo -e "  ${GREEN}✅ LiveKit already installed${NC}"
    return
  fi

  if [ -f "$SCRIPT_DIR/install-livekit.sh" ]; then
    echo -e "  Installing LiveKit..."
    bash "$SCRIPT_DIR/install-livekit.sh" 2>&1 | tail -3
    echo -e "  ${GREEN}✅ LiveKit installed${NC}"
  else
    echo -e "  ${YELLOW}⚠️ install-livekit.sh not found — voice/video calls will not work${NC}"
  fi
}

install_livekit

# ============================================================================
# Inference + LoRA-training engine — NATIVE, no separate install (Unsloth excised)
# ============================================================================
#
# Continuum OWNS its inference + genome-forge stack: the core spawns its own
# llama-server (built from the vendored llama.cpp submodule) to serve models over
# an OpenAI-compatible /v1, and forges LoRA genes natively via mlx_lm on Apple
# Silicon. Nothing to install here — the engine comes up WITH the core. (The mlx
# trainer wants a Python venv with mlx-lm; the core provisions
# ~/.continuum/genome/venv lazily on the first forge/train.)
echo -e "${YELLOW}[8/9] Inference + training engine${NC}"
echo -e "  ${GREEN}✅ native — llama-server (serving) + mlx_lm (forge) come up with the core${NC}"

# ============================================================================
# Tailscale mesh VPN (multi-tower networking)
# ============================================================================

echo -e "${YELLOW}[9/9] Tailscale (grid mode only)${NC}"

# Tailscale is OPTIONAL — it's the substrate for grid (multi-machine) mode
# where peers reach each other for forge/inference distribution. Single-
# machine local users (the majority of Carl's audience) don't need it.
#
# Opt-in via:
#   CONTINUUM_GRID=1 bash install.sh   — wants grid, install + configure
#   bash install.sh --grid             — same, flag form
#
# Default: SKIP. No download, no daemon, no prompts. Carl's local-only
# install completes faster and his attack surface is smaller.
WANTS_GRID="${CONTINUUM_GRID:-0}"
for arg in "$@"; do
  [ "$arg" = "--grid" ] && WANTS_GRID=1
done

if [ "$WANTS_GRID" != "1" ]; then
  echo -e "  ${GREEN}⏭  Skipped — local-only install (no grid).${NC}"
  echo -e "     Re-run with ${YELLOW}CONTINUUM_GRID=1${NC} to enable multi-machine mode later."
else
  case "$PLATFORM" in
    macos)
      if [ -d "/Applications/Tailscale.app" ]; then
        echo -e "  ${GREEN}✅ Tailscale installed — sign in via menu bar${NC}"
      else
        brew install --cask tailscale 2>/dev/null
        echo -e "  ${GREEN}✅ Tailscale installed — sign in via menu bar${NC}"
      fi
      echo -e "  ${YELLOW}  After signing in, enable Tailscale SSH so peers can reach this Mac${NC}"
      echo -e "  ${YELLOW}  without per-device keys: bash scripts/enable-tailscale-ssh.sh${NC}"
      ;;
    linux|wsl)
      bash "$SCRIPT_DIR/install-tailscale.sh"
      ;;
  esac
fi

# DEPS_ONLY mode: all infrastructure installed, skip config/summary/auto-launch
if [ "$SKIP_BUILD" = "1" ]; then
  exit 0
fi

if [ ! -f "$CONFIG_FILE" ]; then
  echo -e "\n${YELLOW}Creating default config at $CONFIG_FILE${NC}"
  cat > "$CONFIG_FILE" << 'ENVEOF'
# Continuum Tower Configuration
# Add API keys here for cloud provider access.
# Uncomment and fill in the ones you need:

# ANTHROPIC_API_KEY=sk-ant-...
# OPENAI_API_KEY=sk-proj-...
# DEEPSEEK_API_KEY=sk-...
# HF_TOKEN=hf_...

# Storage path for models, adapters, datasets (default: ~/.continuum)
# CONTINUUM_STORAGE_PATH=/path/to/storage

# PostgreSQL (auto-configured by install script)
# DATABASE_URL is appended below with the actual username
ENVEOF
  # Append DATABASE_URL with actual username (can't use heredoc single-quotes for this)
  echo "DATABASE_URL=postgres://${USER}@localhost:5432/continuum" >> "$CONFIG_FILE"
  echo -e "  ${YELLOW}Edit $CONFIG_FILE to add your API keys${NC}"
fi

# ============================================================================
# Summary
# ============================================================================

echo ""
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${GREEN}  ✅ Continuum Tower installed!${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo -e "  Platform:  ${PLATFORM}"
if $HAS_CUDA; then
  echo -e "  GPU:       ${GPU_NAME} (CUDA)"
elif $HAS_METAL; then
  echo -e "  GPU:       ${GPU_NAME} (Metal)"
fi
echo -e "  Node:      $(node --version)"
echo -e "  Rust:      $(rustc --version 2>/dev/null | awk '{print $2}' || echo 'not found')"
if [ -f "$VENV_DIR/bin/python3" ]; then
  echo -e "  Python ML: $VENV_DIR"
fi
if command -v tailscale &>/dev/null; then
  ts_ip=$(tailscale ip -4 2>/dev/null || echo "not connected")
  echo -e "  Tailscale: ${ts_ip}"
fi
echo ""
echo -e "  ${YELLOW}Start:${NC}  npm start"
echo -e "  ${YELLOW}Test:${NC}   continuum ping"
echo -e "  ${YELLOW}Config:${NC} $CONFIG_FILE"
echo ""

# ============================================================================
# Auto-launch if called from bootstrap (CONTINUUM_AUTO_LAUNCH=1)
# ============================================================================

if [ "${CONTINUUM_AUTO_LAUNCH:-0}" = "1" ]; then
  echo -e "${YELLOW}Auto-launching system...${NC}"
  npm start
fi
