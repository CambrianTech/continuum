#!/bin/bash
# Gold-gate sweep: for every staged TB task, apply the OFFICIAL solution and run
# the oracle. exit 0 = gradeable here; anything else = env-blocked (named), the
# task is excluded from the honest denominator. Bounds: 240s solution, 480s oracle.
STAGED=~/.continuum/benchmarks/gym/terminal-bench-2-1-staged
REPO=~/.continuum/benchmarks/gym/terminal-bench-2-1-repo/tasks
OUT="$1"; WORK="$2"
echo "task,phase,exit,hint" > "$OUT"
for dir in "$STAGED"/*/; do
  t=$(basename "$dir")
  sol="$REPO/$t/solution/solve.sh"
  w="$WORK/$t"; rm -rf "$w"; mkdir -p "$w"; cp -R "$dir." "$w/" 2>/dev/null
  cd "$w" || continue
  mkdir -p app
  # Protect the HARNESS runner: several official solutions write their ARTIFACT
  # as run.py (it IS the required /app/run.py); run from task root they
  # OVERWROTE the verifier and the sweep "graded" the solution script itself
  # (exit 0, no output) — 7 false golds on 2026-08-24.
  cp run.py .harness-run.py
  if [ ! -f "$sol" ]; then echo "$t,solution,404,no official solve.sh" >> "$OUT"; continue; fi
  sed "s|/app|$w/app|g" "$sol" > solve-local.sh
  # Container contract: solve.sh runs with cwd=/app — our app/ dir, never root.
  SOUT=$(cd app && perl -e 'alarm shift; exec @ARGV' 600 bash ../solve-local.sh 2>&1); SE=$?
  cp .harness-run.py run.py
  if [ $SE -ne 0 ]; then
    hint=$(echo "$SOUT" | tail -1 | tr ',' ';' | cut -c1-90)
    echo "$t,solution,$SE,$hint" >> "$OUT"; continue
  fi
  GOUT=$(perl -e 'alarm shift; exec @ARGV' 900 python3 run.py 400 2>&1); GE=$?
  hint=$(echo "$GOUT" | grep -oE "No module named '[^']*'|ModuleNotFoundError[^,]*|command not found[^,]*" | head -1 | tr ',' ';' | cut -c1-90)
  echo "$t,oracle,$GE,$hint" >> "$OUT"
done
PASS=$(grep -c ",oracle,0," "$OUT"); TOTAL=$(ls "$STAGED" | wc -l | tr -d ' ')
echo "SWEEP DONE: $PASS/$TOTAL tasks gold-gated gradeable"
# Emit THIS HOST's gold gym — the honest, derived-per-host denominator
# (gold sets are host truths, never committed lists).
GYM_OUT=~/.continuum/benchmarks/tb21-gold-$(hostname -s).jsonl
python3 - "$OUT" "$GYM_OUT" <<'PYEOF'
import csv,json,sys
gold={r['task'] for r in csv.DictReader(open(sys.argv[1])) if r['phase']=='oracle' and r['exit']=='0'}
import os
src=os.path.expanduser('~/.continuum/benchmarks/gym/terminal-bench.jsonl')
n=0
with open(sys.argv[2],'w') as out:
    for l in open(src):
        t=json.loads(l)
        if t['id'].removeprefix('tb21-') in gold: out.write(l); n+=1
print(f"gold gym written: {sys.argv[2]} ({n} tasks) — run it with: continuum benchmark/round --benchmark {sys.argv[2]} --persona <name>")
PYEOF
