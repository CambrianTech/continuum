#!/bin/bash
# Detect platform GPU and return appropriate cargo feature flags.
# Source this file to get CARGO_GPU_FEATURES variable.
#
# Usage:
#   source scripts/shared/cargo-features.sh
#   cargo build --release --no-default-features $CARGO_GPU_FEATURES
#
# Results:
#   macOS:         --features metal
#   Linux + CUDA:  --features cuda
#   Linux (no GPU): (empty — CPU only)
#   AMD ROCm:      (empty for now — future: --features rocm)

CARGO_GPU_FEATURES=""

case "$(uname -s)" in
  Darwin)
    CARGO_GPU_FEATURES="--features metal"
    ;;
  Linux)
    # CUDA: check for nvidia-smi in standard and WSL paths
    if command -v nvidia-smi &>/dev/null || [ -f /usr/lib/wsl/lib/nvidia-smi ]; then
      CARGO_GPU_FEATURES="--features cuda"
      # Ensure CUDA toolkit + nvidia-smi are in PATH
      for cuda_dir in /usr/local/cuda /opt/cuda; do
        if [ -d "$cuda_dir/bin" ] && ! command -v nvcc &>/dev/null; then
          export PATH="$cuda_dir/bin:$PATH"
          export LD_LIBRARY_PATH="${cuda_dir}/lib64:${LD_LIBRARY_PATH:-}"
        fi
      done
      # WSL2: nvidia-smi lives in /usr/lib/wsl/lib, not standard PATH
      if [ -d /usr/lib/wsl/lib ] && ! command -v nvidia-smi &>/dev/null; then
        export PATH="/usr/lib/wsl/lib:$PATH"
      fi
    # ROCm (AMD): future support
    # elif command -v rocminfo &>/dev/null; then
    #   CARGO_GPU_FEATURES="--features rocm"
    fi
    ;;
esac
