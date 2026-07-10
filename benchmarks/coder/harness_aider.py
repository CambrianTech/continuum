#!/usr/bin/env python3
"""
harness_aider.py — score the aider CLI (the most-used AI pair-programming CLI) on the coder gym,
same tasks + same rustc grader as the opencode/hermes harnesses, so aider's cell is apples-to-
apples with every other arm.

aider edits files in place through its own agentic loop. We point it at the SAME served
llama-server the other opponent arms use (OpenAI-compatible: --openai-api-base + --model
openai/<alias>), seed an empty solution.rs, and let aider implement into it. Whatever delta
remains vs OURS is the SYSTEM's, on identical weights.

Per task: a fresh workspace with an empty solution.rs, one headless `aider --message` run, then
grade solution.rs. Requires: aider on PATH, a served /v1 endpoint, rustc.

  OPENAI_API_BASE=http://127.0.0.1:8093/v1 python3 harness_aider.py \
      --gym docs/genome/humaneval-rs.jsonl --limit 15 --model Qwen2.5-Coder-14B-Instruct-Q4_K_M.gguf
"""
import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from oneshot_opponent import grade, extract_code  # identical grader + fenced-block extractor
from harness_opencode import _strip_gym_framing     # identical spoken-framing strip


def run_aider(prompt, ws, model, base_url, timeout):
    # aider edits a file it's given; seed an empty solution.rs and let it implement into it.
    sol = os.path.join(ws, "solution.rs")
    open(sol, "w").close()
    task = _strip_gym_framing(prompt)
    msg = "Implement this Rust function completely in solution.rs (replace the file contents):\n\n" + task
    env = dict(os.environ, OPENAI_API_BASE=base_url, OPENAI_API_KEY="sk-none")
    stdout = ""
    try:
        r = subprocess.run(
            ["aider", "--model", f"openai/{model}", "--openai-api-base", base_url,
             "--openai-api-key", "sk-none", "--message", msg,
             "--yes-always", "--no-git", "--no-auto-commits", "--no-auto-lint",
             "--no-stream", "--no-check-update", "--no-show-model-warnings", "--no-gitignore",
             "solution.rs"],
            cwd=ws, capture_output=True, text=True, timeout=timeout, env=env,
        )
        stdout = (r.stdout or "") + (r.stderr or "")
    except subprocess.TimeoutExpired:
        pass
    body = open(sol).read() if os.path.isfile(sol) else ""
    if body.strip():
        return body
    # aider printed the code instead of editing (rare) → fenced fallback, fair either way
    return extract_code(stdout) if "```" in stdout else ""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--gym", required=True)
    ap.add_argument("--limit", type=int, default=15)
    ap.add_argument("--model", default="Qwen2.5-Coder-14B-Instruct-Q4_K_M.gguf")
    ap.add_argument("--base-url", default=os.environ.get("OPENAI_API_BASE", "http://127.0.0.1:8093/v1"))
    ap.add_argument("--label", default="aider")
    ap.add_argument("--timeout", type=int, default=240)
    a = ap.parse_args()
    tasks = [json.loads(l) for l in open(a.gym) if l.strip()][:a.limit]
    passed, no_file = 0, 0
    for i, t in enumerate(tasks):
        ws = tempfile.mkdtemp()
        gdir = tempfile.mkdtemp()
        try:
            code = run_aider(t["prompt"], ws, a.model, a.base_url, a.timeout)
            if not code:
                no_file += 1
                ok = False
            else:
                ok, _ = grade(code, t.get("test", ""), gdir)
            passed += 1 if ok else 0
            print(f"  [{i+1}/{len(tasks)}] {t.get('id','')} "
                  f"{'PASS' if ok else 'fail'}{'' if code else ' (no file written)'}", file=sys.stderr)
        finally:
            shutil.rmtree(ws, ignore_errors=True)
            shutil.rmtree(gdir, ignore_errors=True)
    n = len(tasks)
    print(f"| {a.label} | {passed}/{n} | {round(100*passed/n)}% | agentic (aider CLI) | no-file {no_file} |")


if __name__ == "__main__":
    main()
