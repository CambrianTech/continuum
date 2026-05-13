#!/usr/bin/env bash
# Blackwell RTX 5090 sm_120 baseline bench for Qwen-VL multimodal.
#
# Purpose: prove the local-multimodal path required by #1072 alpha contract
# works on the Blackwell tier with measurable performance, and produce the
# numbers that docs/benchmarks/blackwell-rtx5090-qwen-vl.md cites.
#
# Reproducer for one specific tier (RTX 5090, sm_120, Windows WSL2 + Docker
# Desktop). Other tiers run the same script with their CUDA arch substituted
# via $CUDA_ARCH or via cmake's `native` auto-detection.
#
# Idempotent: the heavy bits (llama.cpp clone+build, Qwen2-VL GGUF + mmproj
# download) live in a named Docker volume `qwen-vl-bench-work` so re-runs
# skip the slow setup. `--force-rebuild` blows the volume away.
#
# Usage:
#   scripts/bench-blackwell-vl.sh                # text+vision bench
#   scripts/bench-blackwell-vl.sh --force-rebuild
#
# Env:
#   CUDA_ARCH     CUDA compute capability arch (default: 120-real for sm_120).
#                 Use 'native' to auto-detect.
#   MODEL_REPO    HF repo for the Qwen-VL GGUF (default: bartowski/Qwen2-VL-7B-Instruct-GGUF)
#   MODEL_FILE    Q4_K_M GGUF filename
#   MMPROJ_FILE   multimodal projector GGUF filename
#   TEST_IMAGE_URL  publicly fetchable image for the vision smoke

set -euo pipefail

CUDA_ARCH="${CUDA_ARCH:-120-real}"
MODEL_REPO="${MODEL_REPO:-bartowski/Qwen2-VL-7B-Instruct-GGUF}"
MODEL_FILE="${MODEL_FILE:-Qwen2-VL-7B-Instruct-Q4_K_M.gguf}"
MMPROJ_FILE="${MMPROJ_FILE:-mmproj-Qwen2-VL-7B-Instruct-f16.gguf}"
TEST_IMAGE_URL="${TEST_IMAGE_URL:-https://upload.wikimedia.org/wikipedia/commons/4/4d/Cat_November_2010-1a.jpg}"
VOLUME="qwen-vl-bench-work"
CUDA_IMAGE="nvidia/cuda:12.8.0-devel-ubuntu22.04"

if [ "${1:-}" = "--force-rebuild" ]; then
    docker volume rm "$VOLUME" >/dev/null 2>&1 || true
fi
docker volume create "$VOLUME" >/dev/null

echo "=== host GPU ==="
nvidia-smi --query-gpu=name,compute_cap,memory.free,driver_version --format=csv | head -3
echo ""
echo "=== bench config ==="
echo "  CUDA_ARCH:   $CUDA_ARCH"
echo "  MODEL_REPO:  $MODEL_REPO"
echo "  MODEL_FILE:  $MODEL_FILE"
echo "  MMPROJ_FILE: $MMPROJ_FILE"
echo "  VOLUME:      $VOLUME"
echo ""

docker run --rm --gpus all \
    -v "$VOLUME:/work" \
    -w /work \
    -e CUDA_ARCH="$CUDA_ARCH" \
    -e MODEL_REPO="$MODEL_REPO" \
    -e MODEL_FILE="$MODEL_FILE" \
    -e MMPROJ_FILE="$MMPROJ_FILE" \
    -e TEST_IMAGE_URL="$TEST_IMAGE_URL" \
    --name qwen-vl-bench \
    "$CUDA_IMAGE" \
    bash -c '
set -euo pipefail
echo "=== install deps ==="
apt-get update -qq >/dev/null
apt-get install -y -qq cmake build-essential git curl ca-certificates libcurl4-openssl-dev pkg-config >/dev/null
echo "ok"

echo ""
echo "=== build llama.cpp (upstream main, sm_120-targeted) ==="
cd /work
if [ ! -d llama.cpp ]; then
    git clone --depth=1 https://github.com/ggerganov/llama.cpp llama.cpp
fi
cd llama.cpp
echo "llama.cpp HEAD: $(git log -1 --format=%h\ %s\ \(%ad\) --date=short)"

if [ ! -x build/bin/llama-bench ] || [ ! -x build/bin/llama-mtmd-cli ]; then
    mkdir -p build && cd build
    cmake .. -DGGML_CUDA=ON -DCMAKE_CUDA_ARCHITECTURES="$CUDA_ARCH" -DGGML_CCACHE=OFF -DLLAMA_CURL=ON 2>&1 | tail -5
    cmake --build . --target llama-bench llama-cli llama-mtmd-cli -j 8 2>&1 | tail -3
fi
ls -la /work/llama.cpp/build/bin/llama-bench /work/llama.cpp/build/bin/llama-mtmd-cli

echo ""
echo "=== download Qwen-VL model + mmproj ==="
mkdir -p /work/models/qwen-vl
cd /work/models/qwen-vl
for f in "$MODEL_FILE" "$MMPROJ_FILE"; do
    if [ ! -s "$f" ] || [ "$(stat -c%s "$f")" -lt 100000 ]; then
        echo "  downloading $f..."
        curl -sL -o "$f" "https://huggingface.co/${MODEL_REPO}/resolve/main/${f}"
    fi
done
ls -la /work/models/qwen-vl/
mkdir -p /work/test-images
cd /work/test-images
if [ ! -s cat.jpg ] || [ "$(stat -c%s cat.jpg)" -lt 1000 ]; then
    curl -sL -o cat.jpg "$TEST_IMAGE_URL"
fi
ls -la /work/test-images/cat.jpg

echo ""
echo "=== llama-bench text-only Q4_K_M -ngl 99 -p 512 -n 128 -r 3 ==="
nvidia-smi --query-gpu=memory.used,memory.free --format=csv,noheader,nounits
/work/llama.cpp/build/bin/llama-bench \
    -m /work/models/qwen-vl/${MODEL_FILE} \
    -ngl 99 -p 512 -n 128 -r 3 2>&1 | tail -8

echo ""
echo "=== llama-mtmd-cli vision smoke + cat.jpg ==="
nvidia-smi --query-gpu=memory.used,memory.free --format=csv,noheader,nounits
/work/llama.cpp/build/bin/llama-mtmd-cli \
    -m /work/models/qwen-vl/${MODEL_FILE} \
    --mmproj /work/models/qwen-vl/${MMPROJ_FILE} \
    --image /work/test-images/cat.jpg \
    -p "Describe this image in one sentence." \
    -ngl 99 -n 64 --temp 0 2>&1 | tail -25
echo ""
nvidia-smi --query-gpu=memory.used,memory.free --format=csv,noheader,nounits
'
