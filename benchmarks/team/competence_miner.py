#!/usr/bin/env python3
"""
competence_miner.py — per-(persona, task-kind) track records mined from the run
ledgers (EMERGENT-TEAM-ORGANIZATION.md § Speciation, item 3: competence is
visible, never prescriptive).

Sources (all already-durable byproducts of work — nothing new is recorded):
  ~/.continuum/progress/agent-solve-*.json   — every solve/review run: acts,
                                               failed, files, run_id (kind prefix)
  benchmarks/RESULTS.jsonl                   — scored benchmark rows

Output: a JSON track record per persona per kind — attempts, completed, failure
rate, mean acts — written to ~/.continuum/team/track-record.json. This file is
the read surface for (a) the future Rust RagSource that lets a persona KNOW her
own record ("my last four reviews landed") and (b) the roster surface teammates
see. Descriptive only: nothing routes on it; preferential attachment is the
personas' own move.

Usage: python3 benchmarks/team/competence_miner.py [--print]
"""
import argparse
import glob
import json
import os
import re
from collections import defaultdict

PROGRESS = os.path.expanduser("~/.continuum/progress")
OUT = os.path.expanduser("~/.continuum/team/track-record.json")


def kind_of(run_id: str) -> str:
    """Task kind from the run-id convention (swe-*, proj-*, *-review, …).
    Unknown shapes → 'work' (honest bucket, never dropped)."""
    if run_id.endswith("-review"):
        return "review"
    for prefix, kind in (("swe-", "swe"), ("proj-website", "website"),
                        ("proj-", "project"), ("teach-", "teach")):
        if run_id.startswith(prefix):
            return kind
    return "work"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--print", action="store_true")
    args = ap.parse_args()

    rec = defaultdict(lambda: defaultdict(lambda: {
        "attempts": 0, "completed": 0, "failed": 0, "acts_total": 0}))

    for path in glob.glob(os.path.join(PROGRESS, "agent-solve-*.json")):
        try:
            d = json.load(open(path))
        except Exception:
            continue
        persona = d.get("persona_id") or d.get("personaId") or "?"
        run_id = re.sub(r"^agent-solve-", "", os.path.basename(path)[:-5])
        kind = kind_of(d.get("run_id") or run_id)
        row = rec[persona][kind]
        row["attempts"] += 1
        if d.get("failed"):
            row["failed"] += 1
        else:
            row["completed"] += 1
        row["acts_total"] += int(d.get("acts") or 0)

    out = {}
    for persona, kinds in rec.items():
        out[persona] = {}
        for kind, r in kinds.items():
            out[persona][kind] = {
                "attempts": r["attempts"],
                "completed": r["completed"],
                "failed": r["failed"],
                "mean_acts": round(r["acts_total"] / r["attempts"], 1) if r["attempts"] else 0,
            }

    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "w") as f:
        json.dump(out, f, indent=1, sort_keys=True)
    print(f"track record → {OUT} ({len(out)} personas)")
    if args.print:
        print(json.dumps(out, indent=1, sort_keys=True))


if __name__ == "__main__":
    main()
