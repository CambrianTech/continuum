#!/bin/bash
# HARNESS ARM: mini-swe-agent (v2.4.6) on the SAME 7 gold-gated TB tasks, SAME
# served model/endpoint, SAME run.py oracles — only the harness differs from ours.
# Usage: tb7-miniswe-arm.sh <port> <served-model-id> <outdir>
set -u
PORT="$1"; MODEL="$2"; OUT="$3"; mkdir -p "$OUT"
# Self-bootstrap the opponent harness (mini-swe-agent) — a fresh clone needs
# only python3.10+ on PATH; nothing else is assumed.
if [ ! -x "$HERE/.msa-venv/bin/mini" ]; then
  PY=$(command -v python3.12 || command -v python3.11 || command -v python3.10) || { echo "need python >=3.10 for mini-swe-agent"; exit 4; }
  "$PY" -m venv "$HERE/.msa-venv" && "$HERE/.msa-venv/bin/pip" install --quiet mini-swe-agent || exit 4
fi
HERE=$(cd "$(dirname "$0")" && pwd)
GYM="${TB_GYM:-$HOME/.continuum/benchmarks/tb21-gold-$(hostname -s).jsonl}"
STAGED=~/.continuum/benchmarks/gym/terminal-bench-2-1-staged
export OPENAI_BASE_URL="http://127.0.0.1:$PORT/v1" OPENAI_API_KEY="local"
# Non-interactive: skip mini's first-run wizard (it read stdin and hung arm-B v1);
# model comes from -m per task, key/config from env.
export MSWEA_CONFIGURED=true MSWEA_COST_TRACKING=ignore_errors MSWEA_GLOBAL_COST_LIMIT=0
echo "task,agent_exit,oracle_exit,ok" > "$OUT/results.csv"
while IFS= read -r line; do
  ID=$(python3 -c "import json,sys; print(json.loads(sys.argv[1])['id'])" "$line")
  PROMPT=$(python3 -c "import json,sys; print(json.loads(sys.argv[1])['prompt'])" "$line")
  NAME=${ID#tb21-}
  W="$OUT/work/$NAME"; rm -rf "$W"; mkdir -p "$W"
  cp -R "$STAGED/$NAME/." "$W/"; mkdir -p "$W/app"
  # Their harness, bounded: 40 steps, no cost cap (local), yolo, cwd = task dir.
  ( cd "$W" && perl -e 'alarm shift; exec @ARGV' 2400 \
      "$HERE/.msa-venv/bin/mini" -y --exit-immediately -l 0 -m "openai/$MODEL" \
      -t "$PROMPT (work in $W/app; the oracle greps /app-relative paths already projected)" \
      -o "$OUT/$NAME.traj.json" \
      </dev/null >"$OUT/$NAME.log" 2>&1 )
  AE=$?
  ( cd "$W" && perl -e 'alarm shift; exec @ARGV' 480 python3 run.py 400 >"$OUT/$NAME.grade.log" 2>&1 )
  GE=$?
  OK=$([ $GE -eq 0 ] && echo 1 || echo 0)
  echo "$NAME,$AE,$GE,$OK" >> "$OUT/results.csv"
  echo "GRADED $NAME agent_exit=$AE oracle_exit=$GE ok=$OK"
done < "$GYM"
P=$(grep -c ",1$" "$OUT/results.csv"); echo "MINI-SWE ARM DONE: $P/7"
