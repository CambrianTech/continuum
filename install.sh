#!/bin/bash
# Continuum — Cross-platform installer
# Usage: curl -fsSL https://raw.githubusercontent.com/CambrianTech/continuum/main/install.sh | bash
# Or:    ./install.sh (from repo root)
#
# Installs all prerequisites, builds the system, and starts it.
# Works on macOS (ARM + Intel) and Linux (Ubuntu/Debian, WSL2).
# Zero API keys required — local inference works out of the box.
set -e

echo "🧬 Continuum Installer"
echo "======================"
echo ""

# ─── Platform Detection ─────────────────────────────────────────────
OS="$(uname -s)"
ARCH="$(uname -m)"
IS_WSL=false
IS_MAC=false
IS_LINUX=false

case "$OS" in
    Darwin) IS_MAC=true ;;
    Linux)
        IS_LINUX=true
        if grep -qi microsoft /proc/version 2>/dev/null; then
            IS_WSL=true
        fi
        ;;
    *)
        echo "❌ Unsupported OS: $OS"
        echo "   Continuum supports macOS and Linux (including WSL2)"
        exit 1
        ;;
esac

echo "📋 Platform: $OS ($ARCH)$([ "$IS_WSL" = true ] && echo ' [WSL2]')"
echo ""

# ─── Helper: install a package ──────────────────────────────────────
install_pkg() {
    local pkg="$1"
    local name="${2:-$pkg}"

    if command -v "$pkg" &>/dev/null; then
        echo "  ✅ $name: $(command -v $pkg)"
        return 0
    fi

    echo "  📦 Installing $name..."
    if [ "$IS_MAC" = true ]; then
        brew install "$pkg" 2>/dev/null || { echo "  ❌ Failed to install $name via brew"; return 1; }
    elif [ "$IS_LINUX" = true ]; then
        sudo apt-get install -y "$pkg" 2>/dev/null || sudo dnf install -y "$pkg" 2>/dev/null || {
            echo "  ❌ Failed to install $name (tried apt + dnf)"
            return 1
        }
    fi
    echo "  ✅ $name: installed"
}

# ─── Step 1: System Prerequisites ───────────────────────────────────
echo "📋 Step 1: Checking prerequisites"
echo "----------------------------------"

# Homebrew (macOS only)
if [ "$IS_MAC" = true ] && ! command -v brew &>/dev/null; then
    echo "  📦 Installing Homebrew..."
    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
fi

# Node.js
if ! command -v node &>/dev/null; then
    echo "  📦 Installing Node.js..."
    if [ "$IS_MAC" = true ]; then
        brew install node
    else
        curl -fsSL https://deb.nodesource.com/setup_22.x | sudo -E bash -
        sudo apt-get install -y nodejs
    fi
fi
echo "  ✅ Node.js: $(node --version)"

# Rust
if ! command -v rustc &>/dev/null; then
    echo "  📦 Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi
echo "  ✅ Rust: $(rustc --version | awk '{print $2}')"

# Git (should exist but check)
if ! command -v git &>/dev/null; then
    install_pkg git "Git"
fi
echo "  ✅ Git: $(git --version | awk '{print $3}')"

# Python3 (for academy training — optional but nice to have)
if command -v python3 &>/dev/null; then
    echo "  ✅ Python3: $(python3 --version 2>&1 | awk '{print $2}')"
else
    echo "  ⚠️  Python3: not found (optional — needed for academy training)"
fi

echo ""

# ─── Step 2: Clone (if running from curl) ───────────────────────────
if [ ! -f "src/package.json" ]; then
    if [ -f "package.json" ] && grep -q "continuum" package.json; then
        # We're in src/ directory
        cd ..
    elif [ ! -d "src" ]; then
        echo "📋 Step 2: Cloning repository"
        echo "-----------------------------"
        git clone https://github.com/CambrianTech/continuum.git
        cd continuum
        echo "  ✅ Repository cloned"
        echo ""
    fi
fi

# ─── Step 3: Config Bootstrap ────────────────────────────────────────
echo "📋 Step 3: Configuration"
echo "------------------------"

CONFIG_DIR="$HOME/.continuum"
CONFIG_FILE="$CONFIG_DIR/config.env"

mkdir -p "$CONFIG_DIR"

if [ ! -f "$CONFIG_FILE" ]; then
    echo "  📝 Creating config.env with defaults (zero API keys = local-only mode)"
    cat > "$CONFIG_FILE" << 'ENVEOF'
# Continuum Configuration
# All API keys are OPTIONAL — the system works with zero keys using local inference.
# Add keys to enable cloud providers for better quality on complex tasks.

# Cloud providers (uncomment and add your key to enable):
# ANTHROPIC_API_KEY=sk-ant-...
# OPENAI_API_KEY=sk-...
# DEEPSEEK_API_KEY=sk-...
# GROQ_API_KEY=gsk_...
# FIREWORKS_API_KEY=fw_...
# XAI_API_KEY=xai-...
# TOGETHER_API_KEY=tgp_...
# GOOGLE_API_KEY=AIza...

# Server config (defaults are fine)
HTTP_PORT=9000
WS_PORT=9001
ENVEOF
    echo "  ✅ Config created: $CONFIG_FILE"
    echo "  💡 Add API keys later: nano $CONFIG_FILE"
else
    echo "  ✅ Config exists: $CONFIG_FILE"
fi

echo ""

# ─── Step 4: Install Dependencies ────────────────────────────────────
echo "📋 Step 4: Installing dependencies"
echo "-----------------------------------"
cd src
npm install --no-audit --no-fund 2>&1 | tail -3
echo "  ✅ Node dependencies installed"
echo ""

# ─── Step 5: Build + Start ──────────────────────────────────────────
echo "📋 Step 5: Building and starting"
echo "---------------------------------"
echo "  This takes ~2 minutes (Rust build + TypeScript + browser)..."
echo ""
npm start

echo ""
echo "🧬 Continuum is running!"
echo "========================"
echo ""
echo "  🌐 Open: http://localhost:9000"
echo "  📝 Config: $CONFIG_FILE"
echo "  🔑 Add API keys for cloud AI (optional)"
echo "  🛑 Stop: npm stop (from src/)"
echo ""
