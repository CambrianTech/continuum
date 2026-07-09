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
import argparse, json, os, re, subprocess, sys, tempfile, time, urllib.request, urllib.error

def chat(endpoint, model, prompt, api_key, max_tokens, timeout):
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
    with urllib.request.urlopen(req, timeout=timeout) as r:
        d = json.load(r)
    return d["choices"][0]["message"]["content"] or ""

def extract_code(text):
    """The model's answer → compilable Rust. Prefer a fenced ```rust block; else the whole text."""
    m = re.search(r"```(?:rust)?\s*\n(.*?)```", text, re.S)
    return (m.group(1) if m else text).strip()

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
        except (urllib.error.URLError, TimeoutError, KeyError) as e:
            infra_err += 1
            results.append({"id": t["id"], "ok": False, "grade": f"endpoint error: {e}"})
            print(f"  {t['id'][:30]:30} ENDPOINT-ERR", file=sys.stderr)
            continue
        with tempfile.TemporaryDirectory() as wd:
            ok, g = grade(extract_code(answer), t.get("test", ""), wd)
        passed += ok
        results.append({"id": t["id"], "ok": ok, "grade": g})
        print(f"  {t['id'][:30]:30} {'PASS' if ok else 'fail'}  {g[:50]}", file=sys.stderr)

    n = len(tasks) or 1
    out = {"label": args.label, "endpoint": args.endpoint, "model": args.model,
           "tasks": len(tasks), "passed": passed, "pass_rate": passed / n,
           "endpoint_errors": infra_err, "results": results}
    if args.out:
        json.dump(out, open(args.out, "w"), indent=2)
    print(f"\n| {args.label} | {passed}/{len(tasks)} | {passed/n:.0%} | one-shot /v1 | endpoint-errs {infra_err} |")

if __name__ == "__main__":
    main()
