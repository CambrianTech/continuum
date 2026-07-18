#!/usr/bin/env python3
"""
oneshot_opponent.py — score an EXTERNAL model on the Rust coder gym via its OpenAI-compatible
/v1 endpoint. One-shot: the model gets each task once, its answer is compiled + run against the
task's hidden test, pass = exit 0.

ZERO DEPENDENCY, BY DESIGN. This harness imports NOTHING from the Continuum product and requires
no Continuum runtime. The opponent is just a URL you bring (a local llama-server, an unsloth
gateway, ollama, a cloud API, or an airc node exposing /v1). We never depend on Hermes or unsloth
or any opponent — ever; they are optional, external, and reached only through this standalone tool.
Stdlib + `rustc` on PATH is all it needs.

Usage:
  python3 oneshot_opponent.py \
      --endpoint http://127.0.0.1:8080/v1 \
      --model hermes-3-8b \
      --label "Hermes-3-8B" \
      --gym ../../docs/genome/humaneval-rs.jsonl \
      --limit 40

Emits a JSON result and prints a one-line scoreboard row. Compare against OURS, produced by
`run_ours.sh` (which runs the SAME gym through the Continuum system). Same tasks, same grader.
"""
import argparse, http.client, json, os, re, socket, subprocess, sys, tempfile, time, urllib.request, urllib.error

def chat(endpoint, model, prompt, api_key, max_tokens, timeout, retries=3):
    body = json.dumps({
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        "temperature": 0.0,          # greedy — a deterministic, reproducible measurement
        "max_tokens": max_tokens,
    }).encode()
    headers = {"Content-Type": "application/json"}
    if api_key:
        headers["Authorization"] = f"Bearer {api_key}"
    req = urllib.request.Request(endpoint.rstrip("/") + "/chat/completions", data=body, headers=headers)
    # Retry TRANSIENT serving hiccups (a shared local llama-server under load drops
    # connections — RemoteDisconnected — or momentarily 503s while a slot frees). One
    # such blip must not fail an otherwise-answerable task, or (worse) abort the whole
    # arm. The measurement stays honest: same greedy prompt, we just give the endpoint
    # a couple more chances to answer it. A HARD failure after retries still raises.
    # NOTE socket.timeout: on Python 3.9 it is NOT a subclass of TimeoutError (that
    # alias arrived in 3.10), and urlopen's read timeout raises it raw — so it must be
    # named explicitly or a slow hard task aborts the whole arm.
    transient = (http.client.RemoteDisconnected, ConnectionError, TimeoutError,
                 socket.timeout, urllib.error.URLError)
    last = None
    for attempt in range(retries):
        try:
            with urllib.request.urlopen(req, timeout=timeout) as r:
                d = json.load(r)
            return d["choices"][0]["message"]["content"] or ""
        except transient as e:
            last = e
            if attempt < retries - 1:
                time.sleep(2.0 * (attempt + 1))  # linear backoff: 2s, 4s
    raise last

def extract_code(text):
    """The model's answer → compilable Rust. Concatenate ALL ```rust fences (a model
    commonly splits imports and logic across fences; grading only the first drops real
    code and fails on E0432/E0425 — the same fairness fix applied to the SYSTEM grader,
    so both arms are measured identically). Falls back to the first fence, then the
    whole text, for models that fence inconsistently."""
    blocks = re.findall(r"```([^\n]*)\n(.*?)```", text, re.S)
    if blocks:
        rust = [b.strip() for lang, b in blocks if lang.strip().lower() in ("rust", "rs")]
        if rust:
            return "\n\n".join(rust)
        return blocks[0][1].strip()  # no rust-tagged fence → first block (any tag)
    return text.strip()

def grade(answer_code, test_body, workdir):
    """Mirror the Continuum grader: answer + `fn main() { <test> }` → rustc → run. Pass = exit 0."""
    prog = f"{answer_code}\n\nfn main() {{\n{test_body}\n}}\n"
    src = os.path.join(workdir, "prog.rs")
    binp = os.path.join(workdir, "prog_bin")
    with open(src, "w") as f:
        f.write(prog)
    try:
        c = subprocess.run(["rustc", "--edition", "2021", src, "-o", binp],
                           capture_output=True, text=True, timeout=60)
    except subprocess.TimeoutExpired:
        return False, "compile timeout (>60s)"
    if c.returncode != 0:
        return False, "compile error: " + (c.stderr.strip().splitlines() or [""])[0][:120]
    # A solution that hangs (infinite loop) is a FAIL, not a harness crash — kill it and
    # score it wrong, so one bad answer never aborts the whole benchmark run.
    try:
        r = subprocess.run([binp], capture_output=True, text=True, timeout=30)
    except subprocess.TimeoutExpired:
        return False, "runtime timeout (>30s — likely infinite loop)"
    return (r.returncode == 0), ("tests passed" if r.returncode == 0
                                 else "test failed: " + (r.stderr.strip()[:120] or "assertion"))

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--endpoint", required=True, help="OpenAI-compatible base, e.g. http://127.0.0.1:8080/v1")
    ap.add_argument("--model", required=True, help="model name the endpoint expects")
    ap.add_argument("--label", required=True, help="scoreboard label, e.g. 'Hermes-3-8B'")
    ap.add_argument("--gym", default=os.path.join(os.path.dirname(__file__), "..", "..", "docs", "genome", "humaneval-rs.jsonl"))
    ap.add_argument("--limit", type=int, default=40)
    ap.add_argument("--max-tokens", type=int, default=1024)
    ap.add_argument("--timeout", type=int, default=120)
    ap.add_argument("--api-key", default=os.environ.get("OPPONENT_API_KEY", ""))
    ap.add_argument("--out", default=None)
    args = ap.parse_args()

    tasks = [json.loads(l) for l in open(args.gym) if l.strip()][:args.limit]
    results, passed, infra_err = [], 0, 0
    for i, t in enumerate(tasks):
        try:
            answer = chat(args.endpoint, args.model, t["prompt"], args.api_key, args.max_tokens, args.timeout)
        except (urllib.error.URLError, http.client.HTTPException, ConnectionError,
                TimeoutError, socket.timeout, KeyError) as e:
            # A hard endpoint failure on ONE task (after chat()'s own retries) is an
            # infra error for THAT task — score it over attempted tasks, never let it
            # abort the whole arm (the 2026-07-14 RemoteDisconnected that killed a run).
            infra_err += 1
            results.append({"id": t["id"], "ok": False, "grade": f"endpoint error: {e}"})
            print(f"  {t['id'][:30]:30} ENDPOINT-ERR", file=sys.stderr)
            continue
        with tempfile.TemporaryDirectory() as wd:
            ok, g = grade(extract_code(answer), t.get("test", ""), wd)
        passed += ok
        results.append({"id": t["id"], "ok": ok, "grade": g})
        print(f"  {t['id'][:30]:30} {'PASS' if ok else 'fail'}  {g[:50]}", file=sys.stderr)

    # A 0% is a claim about the MODEL; an endpoint error is a claim about the
    # HARNESS. Score over ATTEMPTED tasks only — an arm where nothing was attempted
    # is EXCLUDED, never a zero (the 14B hard-rs cell 2026-07-10: 8/8 ENDPOINT-ERR
    # against a down endpoint rendered as 0%, a false claim headed for the README).
    attempted = len(tasks) - infra_err
    pass_rate = (passed / attempted) if attempted else None
    out = {"label": args.label, "endpoint": args.endpoint, "model": args.model,
           "tasks": len(tasks), "attempted": attempted, "passed": passed,
           "pass_rate": pass_rate, "endpoint_errors": infra_err, "results": results,
           "excluded": attempted == 0}
    if args.out:
        json.dump(out, open(args.out, "w"), indent=2)
    if attempted == 0:
        print(f"\n| {args.label} | — | EXCLUDED | one-shot /v1 | endpoint down: {infra_err}/{len(tasks)} errs |")
    else:
        print(f"\n| {args.label} | {passed}/{attempted} | {pass_rate:.0%} | one-shot /v1 | endpoint-errs {infra_err} |")

if __name__ == "__main__":
    main()
