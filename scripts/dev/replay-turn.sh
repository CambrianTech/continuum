#!/usr/bin/env bash
# replay-turn.sh — the glass-box turn-replay harness.
#
# WHY THIS EXISTS (the principle Joel hammered): you must NEVER have to guess why
# an LLM inferenced something — you must be able to REPLAY the exact turn, see the
# prompt IN and the response OUT, time everything, and prove causation by mutating
# ONE variable. A capture (`~/.continuum/fixtures/prompt-captures/<persona>.jsonl`)
# holds the verbatim system prompt + message thread of a real turn. This harness
# feeds that EXACT prompt back through the live inference seam (`ai/generate`,
# which takes `system_prompt` + `messages` verbatim) so the reading is fresh by
# construction — there is no capture-file-staleness window, the response IS the
# measurement. It then re-runs with one labelled mutation (default: strip the
# [Silence Option] affordance block) and prints the A/B delta. That is how you
# prove "it works" / "it doesn't" instead of theorizing.
#
# Single command, amnesiac-proof, self-contained. Repeatable forever.
#
# Usage:
#   scripts/dev/replay-turn.sh                          # newest capture, last turn, A/B silence strip
#   scripts/dev/replay-turn.sh --persona <uuid>         # pick a persona's capture file
#   scripts/dev/replay-turn.sh --turn -1                # which turn (negative = from end; default -1)
#   scripts/dev/replay-turn.sh --no-mutate              # replay verbatim only (prove repeatability)
#
# Env overrides:
#   MODEL="..."     model id to drive (default: the live llama-server GGUF)
#   STRIP_RE="..."  python regex (DOTALL) of the system-prompt region to delete for variant B
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CU="$HOME/.continuum/cache/cargo-target/debug/continuum"
CAPDIR="$HOME/.continuum/fixtures/prompt-captures"
WORK="$HOME/.continuum/replay"
MODEL="${MODEL:-continuum-ai/qwen2.5-coder-14b-instruct-GGUF}"
# Default mutation: remove the [Silence Option] affordance block (the text that
# defines the "reply PASS to stay silent" escape hatch) — the one variable under test.
STRIP_RE="${STRIP_RE:-\n*\[Silence Option\].*?(which to pick\.|(?=\n\n\[)|$)}"

PERSONA=""
TURN="-1"
MUTATE=1
while [ $# -gt 0 ]; do
  case "$1" in
    --persona) PERSONA="$2"; shift 2 ;;
    --turn)    TURN="$2"; shift 2 ;;
    --no-mutate) MUTATE=0; shift ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

die() { echo "FAIL: $*" >&2; exit 1; }
[ -x "$CU" ] || die "continuum not built at $CU (run: continuum start)"

# --- self-clean workspace ---
rm -rf "$WORK"; mkdir -p "$WORK"

# --- process guard: measure exactly ONE core (no coin-flip readings) ---
CORES=$(pgrep -f 'continuum-core' | wc -l | tr -d ' ')
[ "$CORES" -eq 1 ] || die "$CORES continuum-core processes — need exactly 1 (run: continuum start / kill stragglers)"
"$CU" ping >/dev/null 2>&1 || die "core not responding to ping"

# --- pick the capture file ---
if [ -n "$PERSONA" ]; then
  CAP="$CAPDIR/$PERSONA.jsonl"
else
  CAP=$(ls -t "$CAPDIR"/*.jsonl 2>/dev/null | head -1) || true
fi
[ -n "${CAP:-}" ] && [ -f "$CAP" ] || die "no capture file found (looked in $CAPDIR)"
echo "capture: $CAP"
echo "turn   : $TURN     model: $MODEL     mutate: $MUTATE"

# --- extract the chosen turn → paramsA (verbatim) + paramsB (one variable stripped) ---
python3 - "$CAP" "$TURN" "$MODEL" "$MUTATE" "$STRIP_RE" "$WORK" <<'PY'
import sys, json, re, os
cap, turn, model, mutate, strip_re, work = sys.argv[1], int(sys.argv[2]), sys.argv[3], sys.argv[4]=="1", sys.argv[5], sys.argv[6]
recs = [json.loads(l) for l in open(cap).read().splitlines() if l.strip()]
if not recs: sys.exit("capture has no records")
rec = recs[turn]
system = rec.get("system","")
messages = rec.get("messages",[])
# messages is the verbatim thread the model saw — feed it straight back.
def params(sysp):
    return {"model": model, "system_prompt": sysp, "messages": messages,
            "temperature": 0.0}  # greedy: deterministic, so A/B is a clean one-variable test
json.dump(params(system), open(f"{work}/paramsA.json","w"))
stripped = re.sub(strip_re, "", system, flags=re.DOTALL) if mutate else system
json.dump(params(stripped), open(f"{work}/paramsB.json","w"))
removed = len(system) - len(stripped)
# record what we're testing so the output is self-explanatory
json.dump({"system_chars": len(system), "n_messages": len(messages),
           "stripped_chars": removed, "captured_at_ms": rec.get("captured_at_ms"),
           "orig_response": (rec.get("response",{}) or {}).get("text",""),
           "task_preview": (json.dumps(messages)[:240])},
          open(f"{work}/meta.json","w"))
print(f"prompt IN: system={len(system)} chars (~{len(system)//4} tok), messages={len(messages)}")
if mutate:
    print(f"variant B strips {removed} chars (the [Silence Option] affordance)")
PY

run_variant() {  # $1=label $2=paramsfile
  local label="$1" pf="$2"
  local w0 w1
  w0=$(python3 -c 'import time;print(int(time.time()*1000))')
  "$CU" ai/generate "$(cat "$pf")" >"$WORK/$label.json" 2>"$WORK/$label.err" \
    || { cat "$WORK/$label.err" >&2; die "ai/generate ($label) failed"; }
  w1=$(python3 -c 'import time;print(int(time.time()*1000))')
  echo "$((w1 - w0))" >"$WORK/$label.wall"
}

echo; echo "=== replaying A (verbatim) ==="
run_variant A "$WORK/paramsA.json"
if [ "$MUTATE" = 1 ]; then echo "=== replaying B ([Silence Option] stripped) ==="; run_variant B "$WORK/paramsB.json"; fi

# --- report: prompt OUT for each variant, timed, with verdict ---
python3 - "$WORK" "$MUTATE" <<'PY'
import sys, json, os
work, mutate = sys.argv[1], sys.argv[2]=="1"
meta = json.load(open(f"{work}/meta.json"))
def read(label):
    d = json.load(open(f"{work}/{label}.json"))
    wall = int(open(f"{work}/{label}.wall").read().strip())
    u = d.get("usage",{}) or {}
    out = int(u.get("outputTokens",0)); srv = int(d.get("responseTimeMs",0))
    tps = round(out/(srv/1000.0),2) if srv>0 else 0.0
    return {"text": d.get("text",""), "finish": d.get("finishReason",""),
            "out_tok": out, "in_tok": int(u.get("inputTokens",0)),
            "server_ms": srv, "wall_ms": wall, "tps": tps}
def show(label, r):
    print(f"\n--- variant {label}: prompt OUT ---")
    print(f"  response : {r['text'][:160]!r}")
    print(f"  finish   : {r['finish']}   out_tok: {r['out_tok']}   in_tok: {r['in_tok']}")
    print(f"  TIMING   : server {r['server_ms']}ms   wall {r['wall_ms']}ms   {r['tps']} tok/s")

print(f"\noriginal captured response was: {meta['orig_response'][:80]!r}")
A = read("A"); show("A", A)
if mutate:
    B = read("B"); show("B", B)
    print("\n=== VERDICT (one-variable: [Silence Option] block) ===")
    a_silent = A["text"].strip().upper().startswith("PASS")
    b_silent = B["text"].strip().upper().startswith("PASS")
    print(f"  A (block present): {'PASS/silent' if a_silent else 'answered'}")
    print(f"  B (block removed): {'PASS/silent' if b_silent else 'answered'}")
    if a_silent and not b_silent:
        print("  >> PROVEN: the [Silence Option] affordance CAUSES the silence. Removing it makes her answer.")
    elif a_silent and b_silent:
        print("  >> NOT the cause: she stays silent even without the block. Look elsewhere (model/prompt).")
    elif not a_silent:
        print("  >> A already answered — this turn does not reproduce the all-PASS failure; pick a failing turn.")
PY
echo; echo "artifacts: $WORK/{paramsA,paramsB,A,B}.json"
