#!/bin/bash
# Depth-tax A/B (2026-08-25): our fork's llama-server, one flag varied at a time,
# measured at ~32k prompt depth via the server's own timings. Stock upstream did
# 54.9 tok/s decode @32k on this host; our lane measured 16-30 in-round. Find the
# flag(s) that cost the difference. PRECONDITION: continuum stopped.
# Usage: depth-tax-ab.sh <outdir>
set -u
OUT="$1"; mkdir -p "$OUT"
BIN=~/.continuum/bin/llama-server
MODEL=~/.continuum/models/Ornith-1.5-35B-Q4_K_M.gguf
PORT=58300
# ~32k-token prompt from real repo text (deterministic)
PROMPT_FILE="$OUT/prompt-32k.txt"
[ -f "$PROMPT_FILE" ] || find ~/Development/continuum/core/continuum-core/src/cognition -name "*.rs" | head -20 | xargs cat | head -c 120000 > "$PROMPT_FILE"
echo "variant,prefill_tok_s,decode_tok_s" > "$OUT/results.csv"
run_variant() {
  NAME="$1"; shift
  "$BIN" -m "$MODEL" --port $PORT --host 127.0.0.1 -c 40960 -ngl 99 "$@" > "$OUT/$NAME.server.log" 2>&1 &
  SRV=$!
  for i in $(seq 1 90); do curl -s "http://127.0.0.1:$PORT/health" | grep -q ok && break; sleep 2; done
  R=$(python3 - "$PROMPT_FILE" $PORT <<'PYEOF'
import json,sys,urllib.request
prompt=open(sys.argv[1]).read()
body=json.dumps({"prompt":prompt,"n_predict":256,"temperature":0}).encode()
req=urllib.request.Request(f"http://127.0.0.1:{sys.argv[2]}/completion",body,{"Content-Type":"application/json"})
d=json.load(urllib.request.urlopen(req,timeout=1200))
t=d.get("timings",{})
pp=t.get("prompt_per_second",0); tg=t.get("predicted_per_second",0)
print(f"{pp:.1f},{tg:.1f}")
PYEOF
)
  kill $SRV 2>/dev/null; wait $SRV 2>/dev/null; sleep 3
  echo "$NAME,$R" >> "$OUT/results.csv"; echo "VARIANT $NAME → prefill,decode = $R"
}
# Baseline: the flags our lane actually runs (mirror lane_args: spec+q8_0+fa)
run_variant fork-lane-like --cache-type-k q8_0 --cache-type-v q8_0 --flash-attn on --spec-type ngram-simple
run_variant no-ngram-spec  --cache-type-k q8_0 --cache-type-v q8_0 --flash-attn on
run_variant f16-kv         --flash-attn on --spec-type ngram-simple
run_variant no-flash-attn  --cache-type-k q8_0 --cache-type-v q8_0 --spec-type ngram-simple
run_variant plain-stocklike
echo "A/B DONE"; column -t -s, "$OUT/results.csv"
