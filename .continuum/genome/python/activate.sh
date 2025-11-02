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
