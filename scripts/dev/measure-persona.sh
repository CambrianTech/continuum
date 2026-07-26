#!/usr/bin/env bash
# measure-persona.sh — the repeatable inference measurement loop.
#
# WHY THIS EXISTS: the recurring failure was hand-measuring from whatever stale
# prompt-capture happened to be lying around, never committing a baseline, and
# leaving scattered mess. This is the engineer's answer: ONE command that
# (1) cleans its own workspace, (2) optionally builds + boots exactly one fresh
# server, (3) drives one real inference through the live stack, (4) reads the
# numbers straight off the response — which is fresh by construction, so there
# is no stale-data window to guard against — and (5) prints tokens/sec, latency,
# and prompt-token cost against a committed baseline, ratcheting on demand.
#
# Self-cleaning: the workspace is wiped at the START of every run, so the prior
# run's artifacts stay for inspection until the next run, never accumulating.
#
# Usage:
#   scripts/dev/measure-persona.sh                 # build, boot, measure vs baseline
#   scripts/dev/measure-persona.sh --no-reboot     # measure against already-running core
#   scripts/dev/measure-persona.sh --update        # accept current numbers as the new baseline
#
# Env overrides:
#   MODEL="..."        model id to drive (default: the local llama-server GGUF)
#   PROMPT="..."       the probe prompt (default: a fixed generation-heavy task)
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CU="$HOME/.continuum/cache/cargo-target/debug/continuum"
WORK="$HOME/.continuum/measure"
BASELINE="$REPO/scripts/dev/measure-baseline.json"
MODEL="${MODEL:-continuum-ai/qwen2.5-coder-14b-instruct-GGUF}"
# Generation-heavy by default so tokens/sec is a real reading, not 3 tokens of noise.
PROMPT="${PROMPT:-Write a Python function fib(n) that returns the nth Fibonacci number iteratively, with a docstring and three example calls. Output only the code.}"

DO_REBOOT=1
DO_UPDATE=0
for arg in "$@"; do
  case "$arg" in
    --no-reboot) DO_REBOOT=0 ;;
    --update)    DO_UPDATE=1 ;;
    *) echo "unknown arg: $arg" >&2; exit 2 ;;
  esac
done

die() { echo "FAIL: $*" >&2; exit 1; }
[ -x "$CU" ] || die "continuum not built at $CU (run: continuum start)"

# --- (1) self-clean: wipe our own workspace at the start of every run ---
rm -rf "$WORK"; mkdir -p "$WORK"
echo "workspace: $WORK (wiped clean)"

# --- (2) build + boot exactly ONE fresh server ---
if [ "$DO_REBOOT" = 1 ]; then
  echo "rebuilding + rebooting core (continuum reboot)…"
  "$CU" reboot >"$WORK/reboot.log" 2>&1 || { tail -30 "$WORK/reboot.log" >&2; die "continuum reboot failed"; }
fi
# BSD pgrep has no -c; count with wc. One core is required; >1 means a stale
# server is racing this measurement — fail loud rather than measure a coin flip.
CORES=$(pgrep -f 'continuum-core' | wc -l | tr -d ' ')
LLAMAS=$(pgrep -f 'llama-server' | wc -l | tr -d ' ')
echo "processes: core=${CORES} llama-server=${LLAMAS}"
[ "$CORES" -ge 1 ] || die "no continuum-core process running (run: continuum start)"
[ "$CORES" -le 1 ] || die "$CORES continuum-core processes running — kill the stragglers; the harness measures ONE"
"$CU" ping >/dev/null 2>&1 || die "core not responding to ping after boot"

# --- (3) drive ONE real inference; the response IS the measurement ---
echo "driving inference: model=$MODEL"
WALL_START=$(python3 -c 'import time;print(int(time.time()*1000))')
"$CU" ai/generate --model "$MODEL" --prompt "$PROMPT" >"$WORK/gen.json" 2>"$WORK/gen.err" \
  || { cat "$WORK/gen.err" >&2; die "ai/generate failed"; }
WALL_END=$(python3 -c 'import time;print(int(time.time()*1000))')
WALL_MS=$((WALL_END - WALL_START))

# --- (4)+(5) read numbers off the fresh response; compare to baseline ---
python3 - "$WORK/gen.json" "$WALL_MS" "$BASELINE" "$DO_UPDATE" <<'PY'
import sys, json, os
gen_file, wall_ms, baseline_path, do_update = sys.argv[1], int(sys.argv[2]), sys.argv[3], sys.argv[4] == "1"
d = json.loads(open(gen_file).read())
if not d.get("success"):
    print("FAIL: ai/generate returned success=false:", d.get("text",""), file=sys.stderr); sys.exit(1)

usage = d.get("usage", {}) or {}
in_tok  = int(usage.get("inputTokens", 0))
out_tok = int(usage.get("outputTokens", 0))
server_ms = int(d.get("responseTimeMs", 0))
tps = round(out_tok / (server_ms / 1000.0), 2) if server_ms > 0 else 0.0

cur = {
    "tokens_per_sec":    tps,
    "server_latency_ms": server_ms,
    "wall_latency_ms":   wall_ms,
    "prompt_tokens":     in_tok,
    "output_tokens":     out_tok,
}

base = json.load(open(baseline_path)).get("metrics", {}) if os.path.exists(baseline_path) else {}

# direction map: True = higher is better
HIGHER_BETTER = {"tokens_per_sec": True}
def fmt(name, val):
    b = base.get(name)
    if b is None:
        return f"  {name:20} {val:>10}   (no baseline)"
    hib = HIGHER_BETTER.get(name, False)
    delta = val - b
    pct = (delta / b * 100) if b else 0.0
    if delta == 0:
        tag = "→ same"
    else:
        good = (delta > 0) if hib else (delta < 0)
        tag = "✓ better" if good else "✗ WORSE"
    return f"  {name:20} {val:>10}   base {b:>10}   {delta:+g} ({pct:+.1f}%) {tag}"

print()
print("=== inference measurement (model: %s) ===" % d.get("model",""))
# Directional metrics get a ✓/✗ verdict; output_tokens is sampling variance, info only.
for k in ("tokens_per_sec","server_latency_ms","wall_latency_ms","prompt_tokens"):
    print(fmt(k, cur[k]))
print(f"  {'output_tokens':20} {cur['output_tokens']:>10}   (info — sampling variance)")
print(f"  response_preview     {d.get('text','')[:70]!r}")
print()

if do_update:
    json.dump({"_doc":"Baseline for scripts/dev/measure-persona.sh. tokens_per_sec higher=better; latencies/prompt_tokens lower=better. Refresh with --update after a real improvement and commit.","metrics":cur},
              open(baseline_path,"w"), indent=2); open(baseline_path,"a").write("\n")
    print(f"baseline updated → {baseline_path}")
elif base:
    fails = []
    if "tokens_per_sec"    in base and cur["tokens_per_sec"]    < base["tokens_per_sec"]    * 0.90: fails.append("tokens_per_sec dropped >10%")
    if "server_latency_ms" in base and cur["server_latency_ms"] > base["server_latency_ms"] * 1.20: fails.append("server_latency_ms rose >20%")
    if "prompt_tokens"     in base and cur["prompt_tokens"]     > base["prompt_tokens"]     * 1.10: fails.append("prompt_tokens grew >10%")
    if fails:
        print("RATCHET FAIL:", file=sys.stderr)
        for f in fails: print("  "+f, file=sys.stderr)
        sys.exit(1)
    print("ratchet OK — within baseline.")
PY
