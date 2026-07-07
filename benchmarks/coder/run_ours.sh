#!/usr/bin/env bash
# run_ours.sh — score OUR system on the Rust coder gym, through the full Continuum stack
# (RAG + tools + PX + the persona's own act→observe loop). This is the "ours" row; opponents
# are scored by oneshot_opponent.py against an external /v1. Same gym, same grader (rustc
# compile+run via the eval's test_grade), so the numbers are directly comparable.
#
# Prereq: a running core serving the local model you want to measure (`cu ping` returns ok).
# We warm-check the model first — a cold model 500s and would report a false 0 (learned the
# hard way). Never trust a run with inference errors.
set -euo pipefail

CU="${CU:-$HOME/.continuum/cache/cargo-target/debug/cu}"
PERSONA="${PERSONA:-90e758b2-3cf3-45c1-b100-de7c4ab5a549}"
GYM="${GYM:-$(cd "$(dirname "$0")/../.." && pwd)/docs/genome/humaneval-rs.jsonl}"
LABEL="${1:-ours}"
LIMIT="${2:-40}"
MAX_ACTS="${MAX_ACTS:-6}"

slice="$(mktemp /tmp/coder_gym_XXXX.jsonl)"
head -n "$LIMIT" "$GYM" > "$slice"

# Warm-gate: the eval fails LOUD ("inference failed") on a cold model, so make sure it answers.
port="$(ps aux | grep '[l]lama-server' | grep -oE '\-\-port [0-9]+' | grep -oE '[0-9]+' | head -1 || true)"
if [ -n "${port:-}" ]; then
  warm="$(curl -s -m 25 "http://127.0.0.1:${port}/v1/chat/completions" \
    -H 'Content-Type: application/json' \
    -d '{"messages":[{"role":"user","content":"hi"}],"max_tokens":4}' 2>/dev/null | grep -c '"content"' || true)"
  [ "${warm:-0}" = "1" ] || { echo "model not warm on :$port — wait for load, then retry" >&2; exit 1; }
fi

echo "running OUR system on $LIMIT tasks (max_acts=$MAX_ACTS)…" >&2
"$CU" cognition/eval --persona_id "$PERSONA" --eval_set "$slice" --max_acts "$MAX_ACTS" --max_retries 0 2>&1 \
  | python3 -c "
import sys,json,re
m=re.search(r'\{.*\}',sys.stdin.read(),re.S); d=json.loads(m.group(0)) if m else {}
res=d.get('results') or []
inf=sum(1 for x in res if 'inference failed' in str(x.get('grade','')))
p=sum(1 for x in res if x.get('ok'))
print(f'| $LABEL | {p}/{len(res)} | {d.get(\"pass_rate\",0):.0%} | Continuum system | inf-errs {inf} |')
"
rm -f "$slice"
