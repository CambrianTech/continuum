#!/usr/bin/env python3
"""
harness_opencode.py — score the opencode AGENTIC harness on the coder gym, same tasks + same
rustc grader as oneshot_opponent.py, so opencode's cell is apples-to-apples with ours.

opencode drives a LOCAL model (Qwen-14B) via the toolcall_shim, which recovers the model's
narrated tool calls into native `tool_calls` — giving opencode its FAIR shot on local weights
(the tool-format gap our own parser also closes). Whatever delta remains vs our system is then
honestly the SYSTEM's, not a tool-format artifact.

Per task: a fresh workspace, `opencode run` the implement-prompt, then grade solution.rs.
Requires: opencode on PATH, the shim on :8094 → a 32K-ctx llama-server, rustc.

  python3 harness_opencode.py --gym docs/genome/humaneval-rs.jsonl --limit 15 --model local/qwen14b
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


def _strip_gym_framing(prompt):
    """The gym prompt is framed for SPOKEN grading ('give your answer as a fenced code block').
    That instruction fights an agentic write-to-file harness — opencode obeys it and prints the
    code instead of writing. Drop the framing paragraph so the ONLY instruction opencode sees is
    the harness's write-to-file one; keep just the function (docstring + signature) to implement."""
    head, _, rest = prompt.partition("\n\n")
    return rest if (rest and "fenced code block" in head.lower()) else prompt


def run_opencode(prompt, ws, model, timeout):
    # opencode `run --pure` (piped stdio) resolves its write-tool base UNPREDICTABLY —
    # observed writing to $TMPDIR/opencode/, to cwd, and to its daemon's cwd (the repo
    # root) across invocations. So we hand it an EXPLICIT ABSOLUTE path and read exactly
    # there. If it still SPEAKS the answer instead of writing (some models ignore the write
    # instruction), fall back to a fenced code block in stdout — fair to opencode either way.
    sol = os.path.join(ws, "solution.rs")
    task = _strip_gym_framing(prompt)
    full = (f"Implement this Rust function and write ONLY the finished code to the file at this "
            f"exact absolute path: {sol}\nUse your write tool with that absolute path.\n\n" + task)
    stdout = ""
    try:
        r = subprocess.run(
            ["opencode", "run", "--pure", "--auto", "-m", model, full],
            cwd=ws, capture_output=True, text=True, timeout=timeout,
        )
        stdout = r.stdout or ""
    except subprocess.TimeoutExpired:
        pass
    if os.path.isfile(sol):
        return open(sol).read()
    return extract_code(stdout) if "```" in stdout else ""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--gym", required=True)
    ap.add_argument("--limit", type=int, default=15)
    ap.add_argument("--model", default="local/qwen14b")
    ap.add_argument("--label", default="opencode+shim")
    ap.add_argument("--timeout", type=int, default=240)
    a = ap.parse_args()
    tasks = [json.loads(l) for l in open(a.gym) if l.strip()][:a.limit]
    passed, no_file = 0, 0
    for i, t in enumerate(tasks):
        ws = tempfile.mkdtemp()
        gdir = tempfile.mkdtemp()
        try:
            code = run_opencode(t["prompt"], ws, a.model, a.timeout)
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
    print(f"| {a.label} | {passed}/{n} | {round(100*passed/n)}% | agentic via shim | no-file {no_file} |")


if __name__ == "__main__":
    main()
