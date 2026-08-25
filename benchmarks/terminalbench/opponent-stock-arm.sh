#!/bin/bash
# ARM B — THEIR WORLD: stock upstream llama-server (ggml-org b10612, unmodified)
# + mini-swe-agent, on the same 7 tasks/oracles. Flags = what a competent user
# runs out-of-box: -ngl 99, -c 65536 (agent turns need >4096; f16 KV as stock
# defaults), no spec-decode, no KV quant, no custom anything. Our fork's
# advancements (ngram spec, KV q8_0, slot policy, governor) deliberately absent.
# PRECONDITION: continuum stopped (this owns the GPU). Usage: tb7-stock-arm.sh <outdir>
set -u
OUT="$1"; mkdir -p "$OUT"
HERE=$(cd "$(dirname "$0")" && pwd)
# Self-bootstrap the UNMODIFIED upstream server at a pinned tag — the opponent's
# world must be reproducible byte-for-byte (tag recorded in every result row).
STOCK_TAG=b10612
BIN="$HERE/.llama-stock/llama-$STOCK_TAG/llama-server"
if [ ! -x "$BIN" ]; then
  URL="https://github.com/ggml-org/llama.cpp/releases/download/$STOCK_TAG/llama-$STOCK_TAG-bin-macos-arm64.tar.gz"
  mkdir -p "$HERE/.llama-stock" && curl -sL "$URL" | tar -xz -C "$HERE/.llama-stock" || { echo "stock llama download failed"; exit 4; }
fi
MODEL=~/.continuum/models/Ornith-1.5-35B-Q4_K_M.gguf
PORT=58200
"$BIN" -m "$MODEL" -ngl 99 -c 65536 --port $PORT --host 127.0.0.1 > "$OUT/stock-server.log" 2>&1 &
SRV=$!
trap "kill $SRV 2>/dev/null" EXIT
for i in $(seq 1 60); do curl -s "http://127.0.0.1:$PORT/health" | grep -q '"ok"\|"status"' && break; sleep 5; done
curl -s "http://127.0.0.1:$PORT/v1/models" | head -c 200; echo
# SMOKE GATE (for real this time): one tiny completion through mini before
# burning 7 tasks — if the harness itself can't complete a trivial ask, abort
# loudly as ARM-ENV-FAIL rather than minting seven fake zeros.
export MSWEA_CONFIGURED=true MSWEA_COST_TRACKING=ignore_errors MSWEA_GLOBAL_COST_LIMIT=0
export OPENAI_BASE_URL="http://127.0.0.1:$PORT/v1" OPENAI_API_KEY="local"
SMOKE=$(cd /tmp && perl -e 'alarm shift; exec @ARGV' 300   "$HERE/.msa-venv/bin/mini" -y --exit-immediately -l 0 -m "openai/Ornith-1.5-35B-Q4_K_M"   -t "Run: echo SMOKE_OK — then finish." -o /tmp/msa-smoke.traj.json </dev/null 2>&1); SE=$?
if [ $SE -ne 0 ]; then
  echo "ARM-ENV-FAIL: mini smoke test failed (exit $SE) — no task rows minted"; echo "$SMOKE" | tail -5; exit 3
fi
echo "smoke passed — running 7 tasks"
"$HERE/opponent-miniswe-arm.sh" $PORT "Ornith-1.5-35B-Q4_K_M" "$OUT"
