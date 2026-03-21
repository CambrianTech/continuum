#!/bin/bash
# Continuum Tower Install — One command to get a tower running.
#
# Usage:
#   git clone https://github.com/CambrianTech/continuum.git
#   cd continuum/src
#   bash scripts/install.sh
#
# Works on: macOS (Apple Silicon), Ubuntu/Debian (x86_64), WSL2
# Installs: Node.js, Rust, Python venv (with ML packages if GPU detected), system deps
# Idempotent: safe to run multiple times (skips what's already installed)

set -e

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

cd "$PROJECT_DIR"

echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}  Continuum Tower Install${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

PLATFORM=$(preflight_detect_platform)
echo -e "  Platform: ${GREEN}${PLATFORM}${NC}"

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
    # Check for nvcc (CUDA compiler — needed for training, not inference)
    if ! command -v nvcc &>/dev/null; then
      echo -e "  CUDA:     ${YELLOW}nvcc not found — inference works, training needs CUDA toolkit${NC}"
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
# Step 1: System dependencies
# ============================================================================

echo -e "${YELLOW}[1/5] System dependencies${NC}"

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
      ;;
    linux|wsl)
      # Check if we need sudo
      local SUDO=""
      if [ "$(id -u)" -ne 0 ]; then
        SUDO="sudo"
      fi
      # Essential build tools + deps
      local needed=()
      command -v gcc &>/dev/null || needed+=("build-essential")
      command -v pkg-config &>/dev/null || needed+=("pkg-config")
      command -v jq &>/dev/null || needed+=("jq")
      command -v curl &>/dev/null || needed+=("curl")
      command -v git &>/dev/null || needed+=("git")
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

      if [ ${#needed[@]} -gt 0 ]; then
        echo -e "  Installing: ${needed[*]}"
        $SUDO apt-get update -qq
        $SUDO apt-get install -y "${needed[@]}"
      fi
      ;;
  esac
  echo -e "  ${GREEN}✅ System deps OK${NC}"
}

install_system_deps

# ============================================================================
# Step 2: Node.js
# ============================================================================

echo -e "${YELLOW}[2/5] Node.js${NC}"

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
  echo -e "  ${GREEN}✅ Node.js $(node --version) installed${NC}"
}

install_node

# ============================================================================
# Step 3: Rust
# ============================================================================

echo -e "${YELLOW}[3/5] Rust${NC}"

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

echo -e "${YELLOW}[4/5] Python ML environment${NC}"

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
  $pip install -q transformers peft accelerate datasets trl bitsandbytes pytest

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

echo -e "${YELLOW}[5/5] Building Continuum${NC}"

echo -e "  Installing npm dependencies..."
npm install --silent 2>&1 | tail -3

echo -e "  Building TypeScript..."
npm run build:ts 2>&1 | tail -1

echo -e "  Building Rust workers..."
bash scripts/setup-rust.sh 2>&1 | tail -5

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

echo -e "${YELLOW}[6/6] PostgreSQL${NC}"

install_postgres() {
  if command -v psql &>/dev/null; then
    echo -e "  ${GREEN}✅ PostgreSQL already installed${NC}"
  else
    case "$PLATFORM" in
      macos) brew install postgresql@16 && brew services start postgresql@16 ;;
      linux|wsl)
        sudo apt-get install -y postgresql postgresql-client
        sudo service postgresql start 2>/dev/null || sudo pg_ctlcluster 16 main start 2>/dev/null || true
        ;;
    esac
    echo -e "  ${GREEN}✅ PostgreSQL installed${NC}"
  fi

  # Set trust auth for local connections (no password needed for development)
  local pg_hba=$(sudo -u postgres psql -t -c "SHOW hba_file" 2>/dev/null | tr -d ' ')
  if [ -n "$pg_hba" ] && [ -f "$pg_hba" ]; then
    if grep -q "scram-sha-256" "$pg_hba" 2>/dev/null; then
      sudo sed -i 's/scram-sha-256/trust/g' "$pg_hba"
      sudo service postgresql restart 2>/dev/null || sudo pg_ctlcluster 16 main restart 2>/dev/null || true
    fi
  fi

  # Create user and database if they don't exist
  local pg_user="${USER:-postgres}"
  if ! sudo -u postgres psql -tc "SELECT 1 FROM pg_roles WHERE rolname='$pg_user'" 2>/dev/null | grep -q 1; then
    sudo -u postgres createuser -s "$pg_user" 2>/dev/null || true
  fi
  if ! psql -lqt 2>/dev/null | cut -d \| -f 1 | grep -qw continuum; then
    createdb continuum 2>/dev/null || sudo -u postgres createdb continuum 2>/dev/null || true
  fi
  echo -e "  ${GREEN}✅ Database 'continuum' ready${NC}"
}

install_postgres

# ============================================================================
# LiveKit SFU server (voice/video calls)
# ============================================================================

echo -e "${YELLOW}[7/7] LiveKit SFU${NC}"

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
echo ""
echo -e "  ${YELLOW}Start:${NC}  cd src && npm start"
echo -e "  ${YELLOW}Test:${NC}   ./jtag ping"
echo -e "  ${YELLOW}Config:${NC} $CONFIG_FILE"
echo ""

# ============================================================================
# Auto-launch if called from bootstrap (CONTINUUM_AUTO_LAUNCH=1)
# ============================================================================

if [ "${CONTINUUM_AUTO_LAUNCH:-0}" = "1" ]; then
  echo -e "${YELLOW}Auto-launching system...${NC}"
  npm start
fi
