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
from rival_integrity import classify_cli, run_battery  # the ONE integrity standard


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
    stdout, infra = classify_cli(
        ["opencode", "run", "--pure", "--auto", "-m", model, full],
        cwd=ws, timeout=timeout, tool_name="opencode",
    )
    if os.path.isfile(sol):
        return open(sol).read(), None
    if "```" in stdout:
        return extract_code(stdout), None
    # No artifact and no fenced answer: an infra signature voids the task; a
    # clean exit that produced neither is an honest capability miss.
    return "", infra


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--gym", required=True)
    ap.add_argument("--limit", type=int, default=15)
    ap.add_argument("--model", default="local/qwen14b")
    ap.add_argument("--label", default="opencode+shim")
    ap.add_argument("--timeout", type=int, default=240)
    a = ap.parse_args()
    tasks = [json.loads(l) for l in open(a.gym) if l.strip()][:a.limit]
    run_battery(
        tasks, a.label, "agentic via shim",
        lambda prompt, ws: run_opencode(prompt, ws, a.model, a.timeout),
        grade,
    )


if __name__ == "__main__":
    main()
