#!/usr/bin/env python3
"""SWE-bench-Verified → gym adapter (benchmarks-are-adapters: import task + oracle,
project onto the round rails; the citizen's cognition is the runner).

Reads YOUR host's gold-gate CSV (instances whose GOLD patch resolved here) plus the
dataset rows, emits a gym jsonl where each task:
  - setup_shell : clones the repo at base_commit into her workspace
  - prompt      : the instance's problem statement + working rules
  - dod_shell   : `continuum benchmark/swe-grade --workspace <tree>` — the OFFICIAL
                  grader IS the definition-of-done, so the verify loop becomes
                  sequential repair driven by the real verdict (JSON summary only;
                  held-out test SOURCE never enters her context).

Usage: make_verified_gym.py <gate.csv> <out.jsonl>
"""
import csv, json, os, sys

gate, out_path = sys.argv[1], sys.argv[2]
rows_path = os.path.expanduser(
    "~/.continuum/benchmarks/swe/princeton-nlp__SWE-bench_Verified__default__test.rows.jsonl")
gold = {r["instance"] for r in csv.DictReader(open(gate)) if r.get("resolved") == "true"}
by_id = {}
for line in open(rows_path):
    r = json.loads(line); r = r.get("row", r)
    if r.get("instance_id") in gold:
        by_id[r["instance_id"]] = r

n = 0
with open(out_path, "w") as out:
    for iid, r in sorted(by_id.items()):
        repo, base = r["repo"], r["base_commit"]
        wdir = f"swe/{iid}"
        task = {
            "id": f"swev-{iid}",
            "lang": "python",
            "expect": "",
            "setup_shell": (
                f"mkdir -p swe && rm -rf {wdir} && "
                f"git clone --quiet https://github.com/{repo} {wdir} && "
                f"git -C {wdir} checkout --quiet {base}"
            ),
            "dod_shell": (
                f"continuum benchmark/swe-grade --dataset princeton-nlp/SWE-bench_Verified "
                f"--instance {iid} --workspace {wdir} | grep -q '\"resolved\": true'"
            ),
            "prompt": (
                f"[SWE-bench Verified · {iid}] A real GitHub issue in {repo}. The repo is "
                f"checked out at {wdir} (the buggy commit). Fix the issue described below by "
                f"editing the repo — a surgical patch, not a rewrite. Your working tree's diff "
                f"is graded by the project's own test suite in a fresh clone, so: reproduce "
                f"first, fix the root cause, run the relevant tests yourself before settling. "
                f"Do not touch test files.\n\n--- ISSUE ---\n{r['problem_statement']}"
            ),
        }
        out.write(json.dumps(task) + "\n"); n += 1
print(f"wrote {n} gold-gated Verified tasks → {out_path}")
