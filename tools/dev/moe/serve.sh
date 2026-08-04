#!/bin/bash
# Serve a MoE through our llama.cpp fork with the DEVICE-resident expert cache on, and report what
# the cache actually did: clamp, hit rate, decode tok/s, plus a coherence check on the output.
#
# Why the device cache has room on V4-Flash specifically: its non-expert (resident) weights are under
# 1 GB, so nearly the whole card is available as expert cache. On a model with heavy resident weights
# the clamp will correctly shrink the budget toward zero instead of oversubscribing (the #23 crash).
#
#   usage: tools/dev/moe/serve.sh [n_cpu_moe] [vram_cache_gb]
# See common.sh for the environment it resolves.

cd "$(dirname "${BASH_SOURCE[0]}")" || exit 1
source ./common.sh

NCPU="${1:-48}"           # expert layers left on CPU / streamed
CACHE_GB="${2:-24}"       # device-resident expert cache; clamped to real free VRAM by the engine

MODEL=$(resolve_model) || exit 1
LOG="$OUT_DIR/serve-ncpu${NCPU}-cache${CACHE_GB}.log"

export GGML_OP_OFFLOAD_MIN_BATCH=1     # batch-1 decode -> GPU; this is the seam that feeds the cache
export GGML_MOE_VRAM_CACHE_GB="$CACHE_GB"
export GGML_MOE_OFFLOAD_STATS=1

echo "engine : $ENGINE"
echo "model  : $MODEL"
echo "config : --n-cpu-moe $NCPU, device cache ${CACHE_GB}GB, port $PORT"

"$ENGINE" --model "$MODEL" --n-gpu-layers 99 --n-cpu-moe "$NCPU" \
          -c 8192 --port "$PORT" --host 127.0.0.1 > "$LOG" 2>&1 &
SV=$!

wait_for_listen "$LOG"; rc=$?
case $rc in
    1) echo "FATAL during load:"; grep -aiE "out of memory|error|CUDA|Segmentation|unsupported" "$LOG" | tail -12; reap_engine "$SV"; exit 1 ;;
    2) echo "TIMEOUT waiting for the server to listen; last log lines:"; tail -12 "$LOG"; reap_engine "$SV"; exit 1 ;;
esac

echo "=== startup (cache clamp, fit, arch) ==="
grep -aiE "listening|device cache|clamp|arch|CUDA0 buffer|MOE-PAGER" "$LOG" | tail -12

echo "=== warm generate (is it coherent?) ==="
curl -s "http://127.0.0.1:$PORT/v1/chat/completions" -H 'Content-Type: application/json' \
     -d "$CODING_PROMPT" -o "$OUT_DIR/resp.json" -w "http %{http_code} in %{time_total}s\n"
head -c 500 "$OUT_DIR/resp.json"; echo

echo "=== cache hits + decode rate ==="
grep -aiE "MOE-PAGER|hit|miss|RETAIN|eval time" "$LOG" | tail -16
echo "decode: $(decode_tok_s "$LOG") tok/s"

reap_engine "$SV"
