#!/bin/bash
# Detect platform GPU and return appropriate cargo feature flags.
# Source this file to get CARGO_GPU_FEATURES variable.
#
# Usage:
#   source scripts/shared/cargo-features.sh
#   cargo build --release --no-default-features $CARGO_GPU_FEATURES
#
# Results (matches Carl-OOTB matrix):
#   macOS:                           --features metal,accelerate
#   Linux + Nvidia (incl. WSL):      --features cuda,load-dynamic-ort
#   Linux + AMD (ROCm runtime):      --features rocm,load-dynamic-ort
#   Linux + AMD/Intel (Vulkan only): --features vulkan,load-dynamic-ort
#   Windows-native (DX12):           --features directml
#   Windows-native + Nvidia:         --features cuda,directml (both)
#   Linux (no GPU detected):         empty → continuum-core panics at startup
#                                    (#998 — no CPU fallback per architecture)

CARGO_GPU_FEATURES=""

case "$(uname -s)" in
  Darwin)
    CARGO_GPU_FEATURES="--features metal,accelerate"
    ;;
  Linux)
    # Probe order: CUDA > ROCm > Vulkan. CUDA is highest priority because
    # ORT's CUDA EP + llama.cpp CUDA + Candle CUDA give the most paths.
    # ROCm covers AMD with full ORT EP + Candle (when AMD is available).
    # Vulkan is the fallback that works on AMD/Intel without proprietary
    # runtime libs — covers llama.cpp inference but ORT EPs are absent
    # (no ort/vulkan EP exists today).
    if command -v nvidia-smi &>/dev/null || [ -f /usr/lib/wsl/lib/nvidia-smi ]; then
      CARGO_GPU_FEATURES="--features cuda,load-dynamic-ort"
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
    elif command -v rocminfo &>/dev/null; then
      # AMD with ROCm runtime — full ORT ROCm EP + llama.cpp ROCm path.
      CARGO_GPU_FEATURES="--features rocm,load-dynamic-ort"
    elif command -v vulkaninfo &>/dev/null && vulkaninfo --summary 2>/dev/null | grep -q "deviceName"; then
      # AMD/Intel without ROCm but with Vulkan loader — llama.cpp Vulkan
      # path covers the LLM. ORT EPs are absent (no ort/vulkan); the
      # ORT consumers (fastembed, TTS, STT) will still hard-fail at
      # session create per #985's helper, surfacing the gap clearly.
      CARGO_GPU_FEATURES="--features vulkan,load-dynamic-ort"
    fi
    ;;
  MINGW*|MSYS*|CYGWIN*)
    # Windows-native (Git Bash / MSYS / Cygwin). DX12 is universally
    # available on Win10+ → DirectML EP works on any GPU. Add CUDA on
    # top if Nvidia is present so ORT picks CUDA first (faster) +
    # DirectML stays as a co-listed EP for non-CUDA-supported ops.
    CARGO_GPU_FEATURES="--features directml"
    # candle-cuda's affine.cu compiles via nvcc, which needs the MSVC host
    # compiler cl.exe on PATH (an active vcvars env). Only add cuda when cl.exe
    # is actually reachable; otherwise nvcc fatals "Cannot find compiler
    # 'cl.exe'" and the ENTIRE core build dies. directml needs no kernel
    # compilation, so it stays as the universal Windows GPU EP and the build
    # degrades gracefully instead of hard-failing. [[windows-build-env-drift]]
    if command -v nvidia-smi &>/dev/null && command -v cl.exe &>/dev/null; then
      CARGO_GPU_FEATURES="--features cuda,directml"
    fi
    ;;
esac
