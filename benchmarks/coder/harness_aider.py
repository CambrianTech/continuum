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
from rival_integrity import classify_cli, run_battery  # the ONE integrity standard


def run_aider(prompt, ws, model, base_url, timeout):
    # aider edits a file it's given; seed an empty solution.rs and let it implement into it.
    sol = os.path.join(ws, "solution.rs")
    open(sol, "w").close()
    task = _strip_gym_framing(prompt)
    msg = "Implement this Rust function completely in solution.rs (replace the file contents):\n\n" + task
    env = dict(os.environ, OPENAI_API_BASE=base_url, OPENAI_API_KEY="sk-none")
    stdout, infra = classify_cli(
        ["aider", "--model", f"openai/{model}", "--openai-api-base", base_url,
         "--openai-api-key", "sk-none", "--message", msg,
         "--yes-always", "--no-git", "--no-auto-commits", "--no-auto-lint",
         "--no-stream", "--no-check-update", "--no-show-model-warnings", "--no-gitignore",
         "solution.rs"],
        cwd=ws, timeout=timeout, env=env, tool_name="aider",
    )
    body = open(sol).read() if os.path.isfile(sol) else ""
    if body.strip():
        return body, None
    # aider printed the code instead of editing (rare) → fenced fallback, fair either way
    if "```" in stdout:
        return extract_code(stdout), None
    return "", infra


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
    run_battery(
        tasks, a.label, "agentic (aider CLI)",
        lambda prompt, ws: run_aider(prompt, ws, a.model, a.base_url, a.timeout),
        grade,
    )


if __name__ == "__main__":
    main()
