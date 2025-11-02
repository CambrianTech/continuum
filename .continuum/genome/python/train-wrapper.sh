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
