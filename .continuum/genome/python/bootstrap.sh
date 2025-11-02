#!/bin/bash
#
# Genome Training Environment Bootstrap
#
# Purpose: Bulletproof setup for LoRA fine-tuning Python environment
# Strategy: Self-contained micromamba in .continuum (no global pollution)
#
# Usage: ./bootstrap.sh [--force]
#

set -e  # Exit on error

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONTINUUM_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
GENOME_PYTHON_DIR="$CONTINUUM_ROOT/.continuum/genome/python"
MICROMAMBA_ROOT="$GENOME_PYTHON_DIR/micromamba"
ENV_NAME="jtag-genome-training"
ENV_PATH="$MICROMAMBA_ROOT/envs/$ENV_NAME"

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${BLUE}ℹ️  $1${NC}"
}

log_success() {
    echo -e "${GREEN}✅ $1${NC}"
}

log_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

log_error() {
    echo -e "${RED}❌ $1${NC}"
}

# Parse arguments
FORCE=false
if [[ "$1" == "--force" ]]; then
    FORCE=true
    log_warning "Force mode: Will recreate environment if it exists"
fi

# Check if environment already exists
if [[ -d "$ENV_PATH" && "$FORCE" == "false" ]]; then
    log_success "Environment already exists: $ENV_PATH"
    log_info "To recreate, run: $0 --force"
    exit 0
fi

log_info "Starting genome training environment bootstrap..."
log_info "Installation directory: $GENOME_PYTHON_DIR"

# Step 1: Install micromamba if not present
if [[ ! -f "$MICROMAMBA_ROOT/bin/micromamba" ]]; then
    log_info "Installing micromamba..."

    # Create directory
    mkdir -p "$MICROMAMBA_ROOT/bin"

    # Use official micromamba installer script
    # This is the recommended method from micromamba docs
    log_info "Using official micromamba installer..."

    "${SHELL}" <(curl -L micro.mamba.pm/install.sh) <<EOF
$MICROMAMBA_ROOT
y
EOF

    # Check if installation succeeded
    if [[ ! -f "$MICROMAMBA_ROOT/bin/micromamba" ]]; then
        log_error "Failed to install micromamba"
        log_info "Trying alternative download method..."

        # Fallback: Direct binary download
        OS=$(uname -s | tr '[:upper:]' '[:lower:]')
        ARCH=$(uname -m)

        if [[ "$OS" == "darwin" && "$ARCH" == "arm64" ]]; then
            BINARY_URL="https://github.com/mamba-org/micromamba-releases/releases/latest/download/micromamba-osx-arm64"
        elif [[ "$OS" == "darwin" && "$ARCH" == "x86_64" ]]; then
            BINARY_URL="https://github.com/mamba-org/micromamba-releases/releases/latest/download/micromamba-osx-64"
        elif [[ "$OS" == "linux" && "$ARCH" == "x86_64" ]]; then
            BINARY_URL="https://github.com/mamba-org/micromamba-releases/releases/latest/download/micromamba-linux-64"
        elif [[ "$OS" == "linux" && "$ARCH" == "aarch64" ]]; then
            BINARY_URL="https://github.com/mamba-org/micromamba-releases/releases/latest/download/micromamba-linux-aarch64"
        else
            log_error "Unsupported platform: $OS-$ARCH"
            exit 1
        fi

        log_info "Downloading from: $BINARY_URL"
        curl -Ls "$BINARY_URL" -o "$MICROMAMBA_ROOT/bin/micromamba"
        chmod +x "$MICROMAMBA_ROOT/bin/micromamba"

        if [[ ! -f "$MICROMAMBA_ROOT/bin/micromamba" ]]; then
            log_error "Failed to download micromamba binary"
            exit 1
        fi
    fi

    log_success "Micromamba installed successfully"
else
    log_success "Micromamba already installed"
fi

# Step 2: Initialize micromamba
export MAMBA_ROOT_PREFIX="$MICROMAMBA_ROOT"
export MAMBA_EXE="$MICROMAMBA_ROOT/bin/micromamba"

log_info "Micromamba root: $MAMBA_ROOT_PREFIX"

# Step 3: Remove existing environment if force mode
if [[ "$FORCE" == "true" && -d "$ENV_PATH" ]]; then
    log_warning "Removing existing environment..."
    "$MAMBA_EXE" env remove -n "$ENV_NAME" -y 2>/dev/null || true
    rm -rf "$ENV_PATH"
fi

# Step 4: Create environment from environment.yml
log_info "Creating conda environment: $ENV_NAME"
log_warning "This will take 5-10 minutes (downloading PyTorch, Unsloth, etc.)"

"$MAMBA_EXE" env create -f "$GENOME_PYTHON_DIR/environment.yml" -y --root-prefix "$MAMBA_ROOT_PREFIX"

if [[ ! -d "$ENV_PATH" ]]; then
    log_error "Failed to create environment"
    exit 1
fi

log_success "Base environment created successfully"

# Step 5: Verify installation
log_info "Verifying installation..."

# Test Python imports
log_info "Testing Python imports..."

"$MAMBA_EXE" run -n "$ENV_NAME" python3 -c "
import sys, torch, transformers, datasets, trl, peft
print('✅ Python:', sys.version.split()[0])
print('✅ PyTorch:', torch.__version__)
print('✅ MPS (Apple Silicon):', torch.backends.mps.is_available())
print('✅ Transformers:', transformers.__version__)
print('✅ Datasets:', datasets.__version__)
print('✅ TRL:', trl.__version__)
print('✅ PEFT:', peft.__version__)
"

if [[ $? -eq 0 ]]; then
    log_success "All imports successful"
else
    log_error "Import verification failed"
    exit 1
fi

# Step 6: Create activation helper script
log_info "Creating activation helper script..."

cat > "$GENOME_PYTHON_DIR/activate.sh" << 'EOF'
#!/bin/bash
# Helper script to activate genome training environment

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONTINUUM_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
MICROMAMBA_ROOT="$CONTINUUM_ROOT/.continuum/genome/python/micromamba"
ENV_NAME="jtag-genome-training"

export MAMBA_ROOT_PREFIX="$MICROMAMBA_ROOT"
export MAMBA_EXE="$MICROMAMBA_ROOT/bin/micromamba"

# Activate environment
eval "$("$MAMBA_EXE" shell hook --shell bash --root-prefix "$MAMBA_ROOT_PREFIX")"
"$MAMBA_EXE" activate "$ENV_NAME"

echo "✅ Genome training environment activated"
echo "   Python: $(python3 --version)"
echo "   Environment: $CONDA_PREFIX"
EOF

chmod +x "$GENOME_PYTHON_DIR/activate.sh"

log_success "Activation helper created: $GENOME_PYTHON_DIR/activate.sh"

# Step 7: Create Python wrapper script for training
log_info "Creating Python wrapper script..."

cat > "$GENOME_PYTHON_DIR/train-wrapper.sh" << 'EOF'
#!/bin/bash
# Wrapper script to run Python training in isolated environment
# Usage: ./train-wrapper.sh <python-script> [args...]

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONTINUUM_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
MICROMAMBA_ROOT="$CONTINUUM_ROOT/.continuum/genome/python/micromamba"
ENV_NAME="jtag-genome-training"

export MAMBA_ROOT_PREFIX="$MICROMAMBA_ROOT"
export MAMBA_EXE="$MICROMAMBA_ROOT/bin/micromamba"

# Run Python script with all arguments using micromamba run
exec "$MAMBA_EXE" run -n "$ENV_NAME" python3 "$@"
EOF

chmod +x "$GENOME_PYTHON_DIR/train-wrapper.sh"

log_success "Python wrapper created: $GENOME_PYTHON_DIR/train-wrapper.sh"

# Done!
echo ""
log_success "🎉 Genome training environment ready!"
echo ""
log_info "Quick start:"
log_info "  1. Activate manually: source $GENOME_PYTHON_DIR/activate.sh"
log_info "  2. Or use wrapper: $GENOME_PYTHON_DIR/train-wrapper.sh <script.py> [args]"
log_info "  3. TypeScript code will use wrapper automatically"
echo ""
log_info "Environment location: $ENV_PATH"
log_info "Disk usage: $(du -sh "$ENV_PATH" 2>/dev/null | cut -f1 || echo 'unknown')"
echo ""