#!/usr/bin/env bash
#
# Python Environment Bootstrap for LoRA Training
#
# Purpose: Set up isolated Python environment with PyTorch + PEFT
# Philosophy: "Works from clean install on M1 MacBook Air"
#
# Requirements: None (downloads micromamba automatically)
# Platforms: macOS (M1/M2/M3), Linux x86_64/aarch64
#

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Paths
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
GENOME_DIR="${SCRIPT_DIR}"
MICROMAMBA_DIR="${GENOME_DIR}/micromamba"
MICROMAMBA_BIN="${MICROMAMBA_DIR}/bin/micromamba"
ENV_NAME="jtag-training"
ENV_PREFIX="${GENOME_DIR}/envs/${ENV_NAME}"

echo -e "${BLUE}🧬 LoRA Training Environment Bootstrap${NC}"
echo "========================================"
echo ""
echo "This will install:"
echo "  - Micromamba (lightweight conda alternative)"
echo "  - Python 3.11"
echo "  - PyTorch 2.6+ with MPS support"
echo "  - Transformers, PEFT, TRL, Datasets"
echo ""
echo "Installation directory: ${GENOME_DIR}"
echo ""

# Step 1: Check if already bootstrapped
if [ -f "${GENOME_DIR}/train-wrapper.sh" ] && [ -d "${ENV_PREFIX}" ]; then
    echo -e "${GREEN}✅ Environment already bootstrapped${NC}"
    echo ""
    echo "To re-bootstrap, delete and re-run:"
    echo "  rm -rf ${GENOME_DIR}/micromamba ${GENOME_DIR}/envs"
    echo "  bash ${SCRIPT_DIR}/bootstrap.sh"
    exit 0
fi

# Step 2: Detect platform
echo -e "${BLUE}📋 Detecting platform...${NC}"
OS="$(uname -s)"
ARCH="$(uname -m)"

case "${OS}" in
    Darwin*)
        if [[ "${ARCH}" == "arm64" ]]; then
            PLATFORM="osx-arm64"
            echo -e "${GREEN}✅ Detected: macOS Apple Silicon (M1/M2/M3)${NC}"
        else
            PLATFORM="osx-64"
            echo -e "${GREEN}✅ Detected: macOS Intel${NC}"
        fi
        ;;
    Linux*)
        if [[ "${ARCH}" == "aarch64" ]]; then
            PLATFORM="linux-aarch64"
            echo -e "${GREEN}✅ Detected: Linux ARM64${NC}"
        else
            PLATFORM="linux-64"
            echo -e "${GREEN}✅ Detected: Linux x86_64${NC}"
        fi
        ;;
    *)
        echo -e "${RED}❌ Unsupported platform: ${OS}${NC}"
        exit 1
        ;;
esac
echo ""

# Step 3: Download micromamba if needed
if [ ! -f "${MICROMAMBA_BIN}" ]; then
    echo -e "${BLUE}⬇️  Downloading micromamba...${NC}"
    mkdir -p "${MICROMAMBA_DIR}"

    # Download latest micromamba
    MICROMAMBA_URL="https://micro.mamba.pm/api/micromamba/${PLATFORM}/latest"

    echo "   URL: ${MICROMAMBA_URL}"
    echo "   Target: ${MICROMAMBA_DIR}"

    # Download and extract
    curl -Ls "${MICROMAMBA_URL}" | tar -xvj -C "${MICROMAMBA_DIR}" bin/micromamba 2>&1 | grep -v "^x " || true

    if [ ! -f "${MICROMAMBA_BIN}" ]; then
        echo -e "${RED}❌ Failed to download micromamba${NC}"
        exit 1
    fi

    chmod +x "${MICROMAMBA_BIN}"
    echo -e "${GREEN}✅ Micromamba downloaded${NC}"
else
    echo -e "${GREEN}✅ Micromamba already installed${NC}"
fi
echo ""

# Step 4: Create environment
echo -e "${BLUE}🔧 Creating Python environment: ${ENV_NAME}${NC}"

# Export micromamba settings
export MAMBA_ROOT_PREFIX="${GENOME_DIR}"
export MAMBA_EXE="${MICROMAMBA_BIN}"

# Check if environment exists
if "${MICROMAMBA_BIN}" env list | grep -q "${ENV_NAME}"; then
    echo -e "${YELLOW}⚠️  Environment ${ENV_NAME} exists, removing...${NC}"
    "${MICROMAMBA_BIN}" env remove -n "${ENV_NAME}" -y
fi

echo "   Creating environment with Python 3.11..."
"${MICROMAMBA_BIN}" create -n "${ENV_NAME}" python=3.11 -y -c conda-forge

echo -e "${GREEN}✅ Environment created${NC}"
echo ""

# Step 5: Install dependencies
echo -e "${BLUE}📦 Installing PyTorch + ML libraries...${NC}"
echo "   This may take 5-10 minutes..."
echo ""

# Activate environment and install packages
"${MICROMAMBA_BIN}" run -n "${ENV_NAME}" pip install --upgrade pip

# Install PyTorch (with MPS support for Apple Silicon)
if [[ "${PLATFORM}" == "osx-arm64" ]]; then
    echo "   Installing PyTorch with Apple Silicon MPS support..."
    "${MICROMAMBA_BIN}" run -n "${ENV_NAME}" pip install torch torchvision torchaudio
else
    echo "   Installing PyTorch (CPU/CUDA auto-detect)..."
    "${MICROMAMBA_BIN}" run -n "${ENV_NAME}" pip install torch torchvision torchaudio
fi

# Install transformers ecosystem
echo "   Installing Transformers, PEFT, TRL, Datasets..."
"${MICROMAMBA_BIN}" run -n "${ENV_NAME}" pip install \
    transformers \
    peft \
    datasets \
    trl \
    accelerate \
    bitsandbytes \
    sentencepiece \
    protobuf

echo -e "${GREEN}✅ Dependencies installed${NC}"
echo ""

# Step 6: Verify installation
echo -e "${BLUE}🔍 Verifying installation...${NC}"

"${MICROMAMBA_BIN}" run -n "${ENV_NAME}" python -c "
import sys
import torch
import transformers
import peft
import trl
import datasets
import accelerate

print('✅ All imports successful')
print(f'   Python: {sys.version.split()[0]}')
print(f'   PyTorch: {torch.__version__}')
print(f'   Transformers: {transformers.__version__}')
print(f'   PEFT: {peft.__version__}')
print(f'   TRL: {trl.__version__}')
print(f'   Datasets: {datasets.__version__}')

# Check MPS availability
if torch.backends.mps.is_available():
    print('   Device: Apple Silicon MPS available ✅')
elif torch.cuda.is_available():
    print(f'   Device: CUDA {torch.cuda.get_device_name(0)} ✅')
else:
    print('   Device: CPU only (training will be slow)')
"

echo ""

# Step 7: Create wrapper script
echo -e "${BLUE}📝 Creating train-wrapper.sh...${NC}"

cat > "${GENOME_DIR}/train-wrapper.sh" << 'WRAPPER_EOF'
#!/usr/bin/env bash
#
# Python Training Wrapper
# Activates micromamba environment and runs training script
#

set -euo pipefail

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
MICROMAMBA_BIN="${SCRIPT_DIR}/micromamba/bin/micromamba"
ENV_NAME="jtag-training"

# Export micromamba settings
export MAMBA_ROOT_PREFIX="${SCRIPT_DIR}"
export MAMBA_EXE="${MICROMAMBA_BIN}"

# Run Python script with arguments
"${MICROMAMBA_BIN}" run -n "${ENV_NAME}" python "$@"
WRAPPER_EOF

chmod +x "${GENOME_DIR}/train-wrapper.sh"

echo -e "${GREEN}✅ Wrapper script created${NC}"
echo ""

# Step 8: Final check
echo "=" "========================================="
echo -e "${GREEN}🎉 Bootstrap complete!${NC}"
echo ""
echo "Environment details:"
echo "  Name: ${ENV_NAME}"
echo "  Location: ${ENV_PREFIX}"
echo "  Wrapper: ${GENOME_DIR}/train-wrapper.sh"
echo ""
echo "Test with:"
echo "  ${GENOME_DIR}/train-wrapper.sh --version"
echo ""
echo "LoRA training is now ready! 🧬"
