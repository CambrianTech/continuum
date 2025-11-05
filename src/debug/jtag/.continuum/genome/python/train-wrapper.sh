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
