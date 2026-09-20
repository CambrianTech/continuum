#!/bin/bash
# Runs INSIDE nvidia/cuda:12.8.0-devel. Builds ds4 for the 5090 (sm_120) and
# does a ONE-SHOT coding generation (measures prefill/gen t/s AND shows quality).
# Mounts: /ds4 = source (ro), /models = GGUF dir (ro).
set -euo pipefail
MODEL=/models/DeepSeek-V4-Flash-IQ2XXS-w2Q2K-AProjQ8-SExpQ8-OutQ8-chat-v2-imatrix.gguf

echo "=== [1/3] build deps ==="
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq && apt-get install -y -qq build-essential curl >/dev/null

echo "=== [2/3] build ds4 (cuda-generic, sm_120 Blackwell) ==="
cp -r /ds4 /build && cd /build   # build in container-local FS, not the 9p mount
time make cuda-generic CUDA_ARCH=sm_120 -j"$(nproc)" 2>&1 | tail -20
ls -la ds4 ds4-server 2>/dev/null || { echo "BUILD FAILED — no binaries"; exit 1; }

echo "=== [3/3] one-shot coding generation (streamed, 16GB expert cache) ==="
./ds4 -m "$MODEL" \
  -p "Write a Python function that reverses a singly linked list in place. Return only the code." \
  --tokens 160 \
  --ssd-streaming --ssd-streaming-cache-experts 16GB 2>&1 | tail -70
echo "=== SPIKE COMPLETE ==="
