#!/usr/bin/env python3
"""
harness_hermes.py — score the Hermes Agent CLI (Nous Research) on the coder gym, same tasks +
same rustc grader as harness_opencode.py, so Hermes's cell is apples-to-apples with opencode
and OURS.

Hermes is a popular local AI coding CLI (like opencode). It drives a LOCAL model through its
own agentic loop + tool-calling; here we point it at the SAME served llama-server the opencode
arm uses (provider `custom`, base_url set in ~/.hermes/config.yaml by the sweep). Whatever delta
remains vs our system is the SYSTEM's, on identical weights — not a model or tool-format artifact.

Per task: a fresh workspace, `hermes -z <implement-prompt> -m <model> --provider custom --yolo
--cli`, then grade solution.rs (fenced-stdout fallback if it speaks instead of writes).
Requires: hermes on PATH, a served /v1 endpoint (base_url in hermes config), rustc.

  python3 harness_hermes.py --gym docs/genome/humaneval-rs.jsonl --limit 15 \
      --model Qwen2.5-Coder-14B-Instruct-Q4_K_M.gguf
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


def run_hermes(prompt, ws, model, timeout):
    # Hand Hermes an EXPLICIT ABSOLUTE path to write to (same tactic as the opencode arm — an
    # agentic write tool resolves its base unpredictably across CLIs). Read exactly there; if it
    # SPEAKS the answer instead of writing, fall back to a fenced code block in stdout.
    sol = os.path.join(ws, "solution.rs")
    task = _strip_gym_framing(prompt)
    full = (f"Implement this Rust function and write ONLY the finished code to the file at this "
            f"exact absolute path: {sol}\nUse your write/edit tool with that absolute path.\n\n" + task)
    stdout = ""
    try:
        r = subprocess.run(
            ["hermes", "-z", full, "-m", model, "--provider", "custom",
             "--yolo", "--cli", "--safe-mode"],
            cwd=ws, capture_output=True, text=True, timeout=timeout,
        )
        stdout = (r.stdout or "") + (r.stderr or "")
    except subprocess.TimeoutExpired:
        pass
    if os.path.isfile(sol):
        return open(sol).read()
    return extract_code(stdout) if "```" in stdout else ""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--gym", required=True)
    ap.add_argument("--limit", type=int, default=15)
    ap.add_argument("--model", default="Qwen2.5-Coder-14B-Instruct-Q4_K_M.gguf")
    ap.add_argument("--label", default="hermes")
    ap.add_argument("--timeout", type=int, default=240)
    a = ap.parse_args()
    tasks = [json.loads(l) for l in open(a.gym) if l.strip()][:a.limit]
    passed, no_file = 0, 0
    for i, t in enumerate(tasks):
        ws = tempfile.mkdtemp()
        gdir = tempfile.mkdtemp()
        try:
            code = run_hermes(t["prompt"], ws, a.model, a.timeout)
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
    print(f"| {a.label} | {passed}/{n} | {round(100*passed/n)}% | agentic (Hermes CLI) | no-file {no_file} |")


if __name__ == "__main__":
    main()
