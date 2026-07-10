#!/usr/bin/env python3
"""
headtohead.py — the SYSTEM-LIFT ISOLATOR. Score ONE model TWO ways on the SAME gym, same
grader, and print the delta — the number the scoreboard has always listed as "pending".

  RAW    = the model one-shot against its own /v1 endpoint (no Continuum context) —
           `oneshot_opponent.py`, zero-dependency, the outsider-reproducible path.
  SYSTEM = the SAME model through the full Continuum cognition loop (grounding, tool menu,
           act→observe) — `cu benchmark/run --base_model_id <model>`, its own ephemeral lane.

  Δ = SYSTEM − RAW.  Positive = our loop LIFTS the model.  Negative = our context TAXES it.

This is the honest self-check the SCOREBOARD names: "System-lift isolation — same local model,
one-shot vs run_ours, to attribute the gap between model-fit and our loop/PX." Hermes measured
52% raw / 42% system by hand once; this makes that a one-command, per-model, reproducible row.

We never depend on an opponent: RAW just needs a /v1 URL you already run; SYSTEM needs a booted
core (`cu`). Either arm can be skipped (--skip-raw / --skip-system) to get a single number.

Usage:
  # same served model both ways (clean isolation):
  python3 headtohead.py \
      --endpoint http://127.0.0.1:58057/v1 \
      --model unsloth/Devstral-Small-2507-GGUF \
      --base-model-id unsloth/Devstral-Small-2507-GGUF \
      --label "Devstral-Small-24B" --limit 10

Emits a JSON blob and a ready-to-paste SCOREBOARD row.
"""
import argparse, json, os, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
ONESHOT = os.path.join(HERE, "oneshot_opponent.py")
DEFAULT_GYM = os.path.join(HERE, "..", "..", "docs", "genome", "humaneval-rs.jsonl")
DEFAULT_CU = os.path.expanduser("~/.continuum/cache/cargo-target/debug/cu")
def resolve_persona(cu):
    """The resident persona whose cognition runs the SYSTEM arm — resolved LIVE from the
    booted core (`cu cognition/personas`), never a hardcoded id (a baked UUID only exists on
    one machine and breaks every other install). Any resident works: the arm swaps her served
    brain to --base-model-id on an ephemeral lane, so WHO she is doesn't change the measurement
    (same weights, same gym). Fails loud when no persona is resident."""
    r = subprocess.run([cu, "cognition/personas"], capture_output=True, text=True)
    try:
        personas = json.loads(r.stdout).get("personas") or []
    except json.JSONDecodeError:
        personas = []
    if not personas:
        raise SystemExit("no resident persona (is the core booted?) — cannot run the SYSTEM arm. "
                         f"cu output: {r.stdout[:200]} {r.stderr[:200]}")
    p = personas[0]
    print(f"[persona] {p.get('name')} ({p.get('persona_id')})", file=sys.stderr)
    return p["persona_id"]


def run_raw(args):
    """RAW arm — delegate to the zero-dep opponent harness, capture its JSON out file."""
    out = os.path.join(args.tmp, "raw.json")
    cmd = [sys.executable, ONESHOT,
           "--endpoint", args.endpoint, "--model", args.model, "--label", args.label,
           "--gym", args.gym, "--limit", str(args.limit),
           "--max-tokens", str(args.max_tokens), "--timeout", str(args.timeout),
           "--out", out]
    if args.api_key:
        cmd += ["--api-key", args.api_key]
    print(f"[raw] one-shot {args.model} @ {args.endpoint} ({args.limit} tasks)…", file=sys.stderr)
    subprocess.run(cmd, check=True)
    d = json.load(open(out))
    return {"passed": d["passed"], "tasks": d["tasks"],
            "attempted": d.get("attempted", d["tasks"]),
            "pass_rate": d["pass_rate"],
            "endpoint_errors": d.get("endpoint_errors", 0),
            "excluded": d.get("excluded", False)}


def run_system(args):
    """SYSTEM arm — the same model through the full loop via `cu benchmark/run`.

    `--base_model_id` swaps the persona onto this model's OWN ephemeral lane (the humane-eval
    invariant: her living brain is untouched), so the measured weights are identical to RAW.
    Foreground, because the detached path returns a placeholder 0/0 before the run lands.
    """
    cmd = [args.cu, "benchmark/run", "--name", args.benchmark,
           "--persona_id", args.persona_id, "--limit", str(args.limit)]
    if args.base_model_id:
        cmd += ["--base_model_id", args.base_model_id]
    print(f"[system] {args.base_model_id or 'served brain'} through full cognition "
          f"({args.limit} tasks; ~100s/task local)…", file=sys.stderr)
    t0 = time.time()
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        raise SystemExit(f"[system] cu benchmark/run failed:\n{r.stdout}\n{r.stderr}")
    # cu prints the result JSON on stdout; take the last JSON object it emitted.
    blob = _last_json(r.stdout)
    if blob is None:
        raise SystemExit(f"[system] no JSON in cu output:\n{r.stdout}\n{r.stderr}")
    total = blob.get("total", 0) or 0
    elapsed = time.time() - t0
    mean_out = blob.get("meanOutputTokensPerTask", None)
    print(f"[system] {blob.get('score')}/{total} in {elapsed:.0f}s "
          f"({mean_out:.0f} out-tok/task)" if mean_out is not None else
          f"[system] {blob.get('score')}/{total} in {elapsed:.0f}s", file=sys.stderr)
    # A zero is a MODEL claim only if the model actually GENERATED. Two infra tells,
    # either of which excludes the cell (never a false 0% on the README):
    #  (a) impossibly fast — full cognition can't finish a task under ~3s, so a
    #      sub-floor wall-time zero means the lane never came up (14B: 0/8 in 23s).
    #  (b) degenerate output — the lane served but generated near-nothing, so no
    #      answer could contain code (forged-4B ~65 tok/task, 14B ~2 tok/task).
    DEGENERATE_FLOOR = 40  # tokens/task — below this, no Rust fn can exist
    too_fast = blob.get("score", 0) == 0 and total > 0 and elapsed < 3.0 * total
    degenerate = (blob.get("score", 0) == 0 and mean_out is not None
                  and mean_out < DEGENERATE_FLOOR)
    suspect = too_fast or degenerate
    if suspect:
        why = ("lane never came up (sub-floor wall-time)" if too_fast
               else f"degenerate output ({mean_out:.0f} tok/task < {DEGENERATE_FLOOR}) — serving suspect")
        print(f"[system] ⚠ EXCLUDED: 0/{total} — {why}, not a model score", file=sys.stderr)
    return {"passed": blob.get("score", 0), "tasks": total,
            "pass_rate": blob.get("pass_rate", 0.0), "excluded": suspect,
            "elapsed_s": round(elapsed),
            "mean_output_tokens": round(mean_out) if mean_out is not None else None}


def _last_json(text):
    """Return the last top-level JSON object in a stream of mixed log + JSON lines."""
    depth, buf, found = 0, [], None
    for ch in text:
        if ch == "{":
            if depth == 0:
                buf = []
            depth += 1
        if depth > 0:
            buf.append(ch)
        if ch == "}":
            depth -= 1
            if depth == 0 and buf:
                try:
                    found = json.loads("".join(buf))
                except json.JSONDecodeError:
                    pass
    return found


def main():
    ap = argparse.ArgumentParser(description="Score one model RAW (one-shot /v1) vs SYSTEM "
                                             "(full Continuum loop) on the same gym; print Δ.")
    ap.add_argument("--endpoint", help="OpenAI-compatible /v1 for the RAW arm")
    ap.add_argument("--model", help="model name the RAW endpoint expects")
    ap.add_argument("--base-model-id", default=None,
                    help="loadable id for the SYSTEM arm's ephemeral lane; omit to use the "
                         "persona's currently-served brain")
    ap.add_argument("--label", required=True, help="scoreboard label, e.g. 'Devstral-Small-24B'")
    ap.add_argument("--benchmark", default="humaneval-rs", help="named benchmark for the SYSTEM arm")
    ap.add_argument("--gym", default=DEFAULT_GYM, help="gym jsonl for the RAW arm (same tasks)")
    ap.add_argument("--limit", type=int, default=10)
    ap.add_argument("--persona-id", default=None,
                    help="resident persona UUID; omitted -> resolved live from the booted core")
    ap.add_argument("--cu", default=DEFAULT_CU)
    ap.add_argument("--max-tokens", type=int, default=1024)
    ap.add_argument("--timeout", type=int, default=120)
    ap.add_argument("--api-key", default=os.environ.get("OPPONENT_API_KEY", ""))
    ap.add_argument("--skip-raw", action="store_true", help="only run the SYSTEM arm")
    ap.add_argument("--skip-system", action="store_true", help="only run the RAW arm")
    ap.add_argument("--out", default=None, help="write the full result JSON here")
    ap.add_argument("--tmp", default=None)
    args = ap.parse_args()
    args.tmp = args.tmp or __import__("tempfile").mkdtemp(prefix="h2h-")
    if not args.skip_system and not args.persona_id:
        args.persona_id = resolve_persona(args.cu)

    if not args.skip_raw and (not args.endpoint or not args.model):
        ap.error("RAW arm needs --endpoint and --model (or pass --skip-raw)")

    raw = None if args.skip_raw else run_raw(args)
    system = None if args.skip_system else run_system(args)

    # Δ only exists between two VALID measurements — an excluded arm poisons the
    # comparison, never quietly zero-fills it.
    raw_valid = raw and not raw.get("excluded") and raw.get("pass_rate") is not None
    sys_valid = system and not system.get("excluded")
    delta = None
    if raw_valid and sys_valid:
        delta = system["pass_rate"] - raw["pass_rate"]

    result = {"label": args.label, "benchmark": args.benchmark, "limit": args.limit,
              "model": args.model, "base_model_id": args.base_model_id,
              "raw": raw, "system": system, "delta_pass_rate": delta}
    if args.out:
        json.dump(result, open(args.out, "w"), indent=2)

    # Human-facing matrix
    print("\n" + "=" * 64)
    print(f"HEAD-TO-HEAD — {args.label}  ({args.benchmark}, {args.limit} tasks)")
    print("=" * 64)
    if raw:
        if not raw_valid:
            print(f"  RAW    (one-shot /v1)      — EXCLUDED ({raw.get('endpoint_errors', 0)} endpoint errors — harness, not model)")
        else:
            print(f"  RAW    (one-shot /v1)      {raw['passed']:>3}/{raw['attempted']:<3}  {raw['pass_rate']:.0%}"
                  + (f"   [{raw['endpoint_errors']} endpoint-errs excluded]" if raw.get('endpoint_errors') else ""))
    if system:
        if not sys_valid:
            mo = system.get("mean_output_tokens")
            why = (f"{mo} tok/task — degenerate serving" if mo is not None and mo < 40
                   else f"0/{system['tasks']} in {system.get('elapsed_s', '?')}s — infra")
            print(f"  SYSTEM (full Continuum)    — EXCLUDED ({why}, not model)")
        else:
            print(f"  SYSTEM (full Continuum)    {system['passed']:>3}/{system['tasks']:<3}  {system['pass_rate']:.0%}")
    if delta is not None:
        verdict = "LIFT ✅" if delta > 0.001 else ("TAX ⚠️" if delta < -0.001 else "neutral")
        print(f"  Δ  SYSTEM − RAW            {delta:+.0%}   {verdict}")
    elif raw or system:
        print("  Δ  SYSTEM − RAW            n/a (an arm was excluded — no comparison claimed)")
    print("=" * 64)
    # Ready-to-paste SCOREBOARD rows — excluded arms render as excluded, never 0%.
    if raw:
        if raw_valid:
            print(f"| {args.label} | {raw['passed']}/{raw['attempted']} | {raw['pass_rate']:.0%} | RAW one-shot /v1 | |")
        else:
            print(f"| {args.label} | — | EXCLUDED | RAW one-shot /v1 | endpoint errors |")
    if system:
        if sys_valid:
            print(f"| {args.label} | {system['passed']}/{system['tasks']} | {system['pass_rate']:.0%} | SYSTEM (full loop) | |")
        else:
            print(f"| {args.label} | — | EXCLUDED | SYSTEM (full loop) | infra failure |")


if __name__ == "__main__":
    main()
