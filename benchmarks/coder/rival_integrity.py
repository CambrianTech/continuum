#!/usr/bin/env python3
"""
rival_integrity.py — the ONE implementation of the rival-arm integrity standard
(benchmarks/agent-solve/README.md § Rival-arm integrity), shared by every
opponent harness (opencode / hermes / aider). A rival CLI's infrastructure
failure is INFRA, not FAIL — identical to our own rule:

  1. SMOKE GATE: the battery only counts if the integration provably works —
     the rival must produce a trivial artifact first. Can't? OUR integration
     problem; the whole run is VOID, never 0%.
  2. Per-task taxonomy: timeout / tool-missing / nonzero-exit-with-nothing =
     rival-INFRA (excluded from the denominator, printed per task). Only a
     clean exit whose artifact is wrong-or-absent counts as their FAIL.
  3. Majority-infra = VOID (the surviving tasks are not a sample).

A VOID row deliberately doesn't parse as a score in matrix.py — it renders as
a blank cell, exactly like our own excluded rows. Their zeros are EARNED, same
as ours. An honest rival number that beats us is worth more than a flattering
zero: the Δ column is only a claim if a skeptic reproducing THEIR side gets
our number.
"""
import shutil
import subprocess
import sys
import tempfile

SMOKE_PROMPT = "Write a Rust file containing exactly `fn main() {}` — nothing else."


def classify_cli(argv, cwd, timeout, env=None, tool_name="rival"):
    """Run a rival CLI once; return (stdout+stderr, infra_reason|None).
    Only INFRA classification lives here — artifact reading stays with the
    harness (each CLI resolves its write target differently)."""
    try:
        r = subprocess.run(
            argv, cwd=cwd, capture_output=True, text=True, timeout=timeout, env=env,
        )
        out = (r.stdout or "") + (r.stderr or "")
        if r.returncode != 0 and not out.strip():
            return out, f"{tool_name} exit {r.returncode}, no output"
        return out, None
    except subprocess.TimeoutExpired:
        return "", f"timeout {timeout}s"
    except FileNotFoundError:
        return "", f"{tool_name} not installed"


def run_battery(tasks, label, arm_note, attempt, grade_fn):
    """The integrity loop: smoke gate → per-task attempt/grade with INFRA
    taxonomy → honest scoreboard row (or VOID). `attempt(prompt, ws)` returns
    `(code, infra_reason|None)`; `grade_fn(code, test, gdir)` returns (ok, _).
    """
    ws = tempfile.mkdtemp()
    try:
        smoke_code, smoke_infra = attempt(SMOKE_PROMPT, ws)
    finally:
        shutil.rmtree(ws, ignore_errors=True)
    if smoke_infra or "fn main" not in (smoke_code or ""):
        reason = smoke_infra or "smoke task produced no artifact"
        print(f"  SMOKE FAILED — run VOID: {reason}", file=sys.stderr)
        print(f"| {label} | VOID | — | rival-INFRA: {reason} | smoke-gated |")
        return

    passed, no_file, infra_n = 0, 0, 0
    for i, t in enumerate(tasks):
        ws = tempfile.mkdtemp()
        gdir = tempfile.mkdtemp()
        try:
            code, infra = attempt(t["prompt"], ws)
            if infra:
                infra_n += 1
                print(f"  [{i+1}/{len(tasks)}] {t.get('id','')} INFRA ({infra})",
                      file=sys.stderr)
                continue
            if not code:
                no_file += 1
                ok = False
            else:
                ok, _ = grade_fn(code, t.get("test", ""), gdir)
            passed += 1 if ok else 0
            print(f"  [{i+1}/{len(tasks)}] {t.get('id','')} "
                  f"{'PASS' if ok else 'fail'}{'' if code else ' (no file written)'}",
                  file=sys.stderr)
        finally:
            shutil.rmtree(ws, ignore_errors=True)
            shutil.rmtree(gdir, ignore_errors=True)
    n = len(tasks)
    effective = n - infra_n
    if effective == 0 or infra_n > n // 2:
        print(f"| {label} | VOID | — | rival-INFRA on {infra_n}/{n} tasks | majority-infra |")
        return
    print(f"| {label} | {passed}/{effective} | {round(100*passed/effective)}% "
          f"| {arm_note} | no-file {no_file}, infra-void {infra_n} |")
