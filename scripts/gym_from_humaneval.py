#!/usr/bin/env python3
"""
Convert HumanEval (HF datasets) → our gym JSONL format.

Output: docs/genome/humaneval-gym.jsonl
Each line: {"id": "humaneval/N", "prompt": "...", "lang": "python", "test": "..."}

The `test` field is the HumanEval `check(candidate)` function body + a call to
`check({entry_point})`, so our test_grade can do:
    {model_code}\n\n{test}
and run it — exit 0 = pass.

Usage:
    python3 scripts/gym_from_humaneval.py [--limit N] [--out path]
"""
import argparse
import json
import sys
import textwrap
import warnings

warnings.filterwarnings("ignore")

parser = argparse.ArgumentParser()
parser.add_argument("--limit", type=int, default=0, help="Take only first N tasks (0 = all)")
parser.add_argument("--out", default="docs/genome/humaneval-gym.jsonl")
args = parser.parse_args()

try:
    from datasets import load_dataset
except ImportError:
    print("ERROR: pip3 install datasets", file=sys.stderr)
    sys.exit(1)

print("Loading HumanEval …", file=sys.stderr)
ds = load_dataset("openai_humaneval", split="test")
print(f"  {len(ds)} tasks", file=sys.stderr)

tasks = ds if not args.limit else ds.select(range(args.limit))

written = 0
with open(args.out, "w") as f:
    for row in tasks:
        task_id    = row["task_id"]          # "HumanEval/0"
        prompt     = row["prompt"]           # function signature + docstring
        test_body  = row["test"]             # METADATA dict + def check(candidate):
        entry_pt   = row["entry_point"]      # "has_close_elements"

        # Normalise id → "humaneval/0"
        gym_id = task_id.lower().replace("/", "_")

        # Build test: existing check() body + a bare call to check(entry_point).
        # test_grade appends this after the model code, so the execution is:
        #   {model_code}
        #
        #   {check_body}
        #   check({entry_point})
        test = test_body.rstrip() + f"\n\ncheck({entry_pt})\n"

        obj = {
            "id":     gym_id,
            "prompt": prompt,
            "lang":   "python",
            "test":   test,
            "entry_point": entry_pt,
            "source": task_id,
        }
        f.write(json.dumps(obj) + "\n")
        written += 1

print(f"Wrote {written} tasks → {args.out}", file=sys.stderr)
