#!/bin/bash
# SWE-bench-Verified gold-gate: derive YOUR host's honestly-gradeable instance list
# by running OFFICIAL gold patches through the official grader, family-spread.
# Usage: goldgate_verified.sh <out.csv> [per_family=3]
set -u
OUT="$1"; PER="${2:-3}"
ROWS=~/.continuum/benchmarks/swe/princeton-nlp__SWE-bench_Verified__default__test.rows.jsonl
[ -f "$ROWS" ] || continuum benchmark/fetch --benchmark swe-bench-verified >/dev/null
SAMPLE=$(python3 - "$ROWS" "$PER" <<'PYEOF'
import json,random,sys
rows=[json.loads(l) for l in open(sys.argv[1])]
def iid(r): return (r.get('row',r) if 'row' in r else r).get('instance_id','')
from collections import defaultdict
fam=defaultdict(list)
for r in rows:
    i=iid(r)
    if i: fam[i.split('__')[0]].append(i)
random.seed(20260825)  # fixed seed: everyone gates the SAME sample
for k,v in sorted(fam.items()):
    for i in random.sample(v, min(int(sys.argv[2]), len(v))): print(i)
PYEOF
)
echo "instance,resolved" > "$OUT"
for I in $SAMPLE; do
  R=$(perl -e 'alarm shift; exec @ARGV' 1800 continuum benchmark/swe-grade \
      --dataset princeton-nlp/SWE-bench_Verified --instance "$I" --gold true 2>&1 \
      | grep -o '"resolved": [a-z]*' | tail -1 | grep -o 'true\|false')
  echo "$I,${R:-timeout}" >> "$OUT"; echo "GATE $I → ${R:-timeout}"
done
echo "GATE DONE: $(grep -c ',true' "$OUT") gradeable — a family whose gold FAILS is excluded BY NAME; its instances are env-blocked on this host, never model zeros"
