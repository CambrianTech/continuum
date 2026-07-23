#!/usr/bin/env python3
"""
git_history_miner.py — mine OUR OWN development history into training pairs.

The repo's git log is a corpus of narrated engineering: this project's commit
discipline (symptom → glass-box → root cause → fix, receipts inline) means each
qualifying commit is a (problem statement, fix diff) supervision pair written by
the people who actually debugged it. No teacher inference, no synthesis — the
history IS the curriculum ([[mine-past-work-for-patterns-clever-vs-typical]]).

Quality gates (a commit must earn its way in):
  - message BODY beyond the subject line (rationale, not "fix typo")
  - subject carries a conventional type (fix/feat/perf/refactor — not chore/docs)
  - diff touches source (.rs/.ts/.py), is 5..400 changed lines, no lockfiles
  - body >= 200 chars (the narration is the value)

Emitted shape (mlx chat {messages}, the genome/job-create contract):
  user      = the commit's PROBLEM half (subject + body up to the fix narration)
              + the pre-image excerpts of the touched hunks
  assistant = the unified diff of the fix

Usage:
  python3 git_history_miner.py --repo . --count 400 --out git-fix-v1.jsonl
  python3 git_history_miner.py --repo . --install git-fix-rationale-v1
"""
import argparse
import datetime
import json
import os
import re
import subprocess

SRC_EXT = (".rs", ".ts", ".py", ".sh")
SKIP_PATH = ("Cargo.lock", "package-lock.json", "generated/", "bindings/", ".jsonl")
TYPE_RE = re.compile(r"^(fix|feat|perf|refactor)(\(|:)")


def sh(args, cwd):
    return subprocess.run(args, cwd=cwd, capture_output=True, text=True).stdout


def qualifying_commits(repo, limit):
    out = sh(["git", "log", "--no-merges", f"-{limit}", "--format=%H%x01%s%x01%b%x02"], repo)
    for chunk in out.split("\x02"):
        chunk = chunk.strip()
        if not chunk:
            continue
        sha, _, rest = chunk.partition("\x01")
        subject, _, body = rest.partition("\x01")
        if not TYPE_RE.match(subject):
            continue
        body = body.split("Co-Authored-By:")[0].split("Claude-Session:")[0].strip()
        if len(body) < 200:
            continue
        yield sha.strip(), subject.strip(), body


def commit_diff(repo, sha):
    """Source-only diff, size-gated. None if the commit doesn't qualify."""
    files = sh(["git", "show", "--name-only", "--format=", sha], repo).split()
    src = [f for f in files
           if f.endswith(SRC_EXT) and not any(s in f for s in SKIP_PATH)]
    if not src or len(src) > 6:
        return None
    diff = sh(["git", "show", "--format=", "--unified=3", sha, "--"] + src, repo)
    changed = sum(1 for l in diff.splitlines() if l[:1] in "+-" and l[:3] not in ("+++", "---"))
    if not (5 <= changed <= 400):
        return None
    return diff


def pre_image_excerpts(repo, sha, diff, budget=3000):
    """The buggy-state context: pre-image hunk windows from the parent commit."""
    out, spent = [], 0
    current = None
    for line in diff.splitlines():
        if line.startswith("--- a/"):
            current = line[6:]
        elif line.startswith("@@") and current and spent < budget:
            m = re.match(r"@@ -(\d+)(?:,(\d+))?", line)
            if not m:
                continue
            start, n = int(m.group(1)), int(m.group(2) or 1)
            blob = sh(["git", "show", f"{sha}~1:{current}"], repo)
            lines = blob.splitlines()[max(0, start - 3): start + n + 2]
            snippet = "\n".join(lines)[:800]
            out.append(f"── {current} (around line {start}) ──\n{snippet}")
            spent += len(snippet)
    return "\n\n".join(out)


def make_pair(repo, sha, subject, body, diff):
    pre = pre_image_excerpts(repo, sha, diff)
    user = (
        "You are fixing a real defect in this codebase. Here is the problem "
        "report (written by the engineer who diagnosed it) and the current "
        "state of the relevant code.\n\n"
        f"PROBLEM — {subject}\n{body}\n\n"
        f"CURRENT CODE:\n{pre}\n\n"
        "Produce the fix as a unified diff against these files."
    )
    return {"messages": [{"role": "user", "content": user},
                         {"role": "assistant", "content": f"```diff\n{diff}```"}]}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", default=".")
    ap.add_argument("--count", type=int, default=400, help="commits to scan (newest first)")
    ap.add_argument("--out", default="git-fix.jsonl")
    ap.add_argument("--install", default=None, metavar="DATASET_NAME")
    ap.add_argument("--eval-frac", type=float, default=0.1)
    args = ap.parse_args()

    rows, skipped = [], 0
    for sha, subject, body in qualifying_commits(args.repo, args.count):
        diff = commit_diff(args.repo, sha)
        if diff is None:
            skipped += 1
            continue
        pair = make_pair(args.repo, sha, subject, body, diff)
        size = len(pair["messages"][0]["content"]) + len(pair["messages"][1]["content"])
        if size > 24_000:
            skipped += 1
            continue
        rows.append(pair)
    print(f"mined {len(rows)} pairs ({skipped} commits skipped by gates)")
    if not rows:
        raise SystemExit("nothing mined — loosen gates or scan more commits")

    n_eval = max(1, int(len(rows) * args.eval_frac))
    eval_rows, train_rows = rows[:n_eval], rows[n_eval:]

    if args.install:
        root = os.path.expanduser(f"~/.continuum/datasets/{args.install}")
        os.makedirs(root, exist_ok=True)
        for name, subset in (("train.jsonl", train_rows), ("eval.jsonl", eval_rows)):
            with open(os.path.join(root, name), "w") as f:
                for r in subset:
                    f.write(json.dumps(r) + "\n")
        json.dump({"name": args.install, "version": "1.0",
                   "total_examples": len(rows), "train_examples": len(train_rows),
                   "eval_examples": len(eval_rows), "train_path": "train.jsonl",
                   "eval_path": "eval.jsonl",
                   "imported_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                   "generator": {"script": "benchmarks/design-gym/git_history_miner.py",
                                 "repo": os.path.abspath(args.repo), "scanned": args.count}},
                  open(os.path.join(root, "manifest.json"), "w"), indent=1)
        print(f"installed '{args.install}' → {root} (train={len(train_rows)}, eval={len(eval_rows)})")
    else:
        with open(args.out, "w") as f:
            for r in train_rows:
                f.write(json.dumps(r) + "\n")
        with open(args.out + ".eval.jsonl", "w") as f:
            for r in eval_rows:
                f.write(json.dumps(r) + "\n")
        print(f"→ {args.out} (+ eval)")


if __name__ == "__main__":
    main()
