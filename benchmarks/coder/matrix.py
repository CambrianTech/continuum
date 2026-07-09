#!/usr/bin/env python3
"""
matrix.py — the publishable BENCHMARK MATRIX. Sweep many models × {RAW one-shot, OURS full loop}
on the same gym + grader, alongside opponent agent frameworks (opencode), and emit a
README-ready table + the exact command to reproduce it.

This is the driver on top of the per-cell primitives:
  - headtohead.py       RAW (one-shot /v1) vs SYSTEM (Continuum loop) for ONE model
  - oneshot_opponent.py RAW cell for any /v1 model (Hermes, a cloud API, an airc peer)
  - harness_opencode.py the opencode agentic-harness opponent cell (same gym, same grader)

Config-driven: `--models models.json` is a list of rows. A bigger machine adds models by editing
that file — the runner and the table don't change. Every number is rustc compile+run graded, so
nothing here can be gamed by prose.

models.json row schema (all fields optional except label):
  {
    "label": "Devstral-Small-24B",       # scoreboard name
    "base_model_id": "unsloth/Devstral-Small-2507-GGUF",  # OURS arm: loadable id (own ephemeral lane)
    "raw_endpoint": "http://127.0.0.1:58057/v1",          # RAW arm: a /v1 already serving it (optional)
    "raw_model": "unsloth/Devstral-Small-2507-GGUF",      # the name that endpoint expects (defaults to base_model_id)
    "opponent": "opencode"                # optional: also run this model through opencode's harness
  }

Usage:
  python3 matrix.py --models models.json --benchmark humaneval-rs --limit 40 --out MATRIX.md
"""
import argparse, json, os, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
H2H = os.path.join(HERE, "headtohead.py")
OPENCODE = os.path.join(HERE, "harness_opencode.py")
DEFAULT_GYM = os.path.join(HERE, "..", "..", "docs", "genome", "humaneval-rs.jsonl")


def h2h(row, args, tmp):
    """Run the RAW+SYSTEM head-to-head for one model row; return its result dict."""
    out = os.path.join(tmp, f"{_slug(row['label'])}.json")
    cmd = [sys.executable, H2H, "--label", row["label"], "--benchmark", args.benchmark,
           "--gym", args.gym, "--limit", str(args.limit),
           "--cu", args.cu, "--out", out]
    if args.persona_id:
        cmd += ["--persona-id", args.persona_id]
    if row.get("base_model_id"):
        cmd += ["--base-model-id", row["base_model_id"]]
    else:
        cmd += ["--skip-system"]
    if row.get("raw_endpoint"):
        cmd += ["--endpoint", row["raw_endpoint"],
                "--model", row.get("raw_model", row["base_model_id"])]
    else:
        cmd += ["--skip-raw"]
    print(f"\n### {row['label']} — head-to-head", file=sys.stderr)
    # A refused/failed cell must NEVER kill the board: record the failure honestly and
    # keep going — losing six finished cells' render to one bad row is the real damage
    # (learned live: a fit-gated 32B aborted the whole humaneval board).
    r = subprocess.run(cmd)
    if r.returncode != 0 or not os.path.exists(out):
        print(f"[cell-failed] {row['label']} (exit {r.returncode}) — recorded, continuing",
              file=sys.stderr)
        return {"label": row["label"], "raw": None, "system": None, "failed": True}
    return json.load(open(out))


def opencode_cell(row, args, tmp):
    """Optional opponent cell: the same model through the opencode agentic harness."""
    print(f"\n### {row['label']} — opencode opponent", file=sys.stderr)
    r = subprocess.run(
        [sys.executable, OPENCODE, "--gym", args.gym, "--limit", str(args.limit),
         "--model", row.get("opencode_model", "local/qwen14b"),
         "--label", f"{row['label']} (opencode)"],
        capture_output=True, text=True)
    # harness_opencode prints a scoreboard row; parse the pass count from its last line
    passed = tasks = None
    for line in r.stdout.splitlines():
        if line.strip().startswith("|"):
            parts = [p.strip() for p in line.split("|")]
            for p in parts:
                if "/" in p and p.replace("/", "").isdigit():
                    passed, tasks = (int(x) for x in p.split("/"))
    return {"passed": passed, "tasks": tasks,
            "pass_rate": (passed / tasks) if tasks else None} if passed is not None else None


def _slug(s):
    return "".join(c if c.isalnum() else "-" for c in s.lower())


def _pct(cell):
    if not cell or cell.get("pass_rate") is None:
        return "—"
    return f"{cell['pass_rate']:.0%} ({cell['passed']}/{cell['tasks']})"


def render(results, args):
    lines = []
    lines.append(f"# Coder benchmark matrix — {args.benchmark} ({args.limit} tasks, rustc compile+run graded)\n")
    lines.append("Same tasks, same grader, every number reproducible. RAW = model one-shot against "
                 "its own `/v1`. OURS = the same model through the full Continuum loop. opencode = the "
                 "same class of local model through the opencode agentic harness (fair tool-format shim).\n")
    lines.append("| model | RAW one-shot | OURS (Continuum) | opencode | Δ OURS−RAW |")
    lines.append("|---|---|---|---|---|")
    for r in results:
        raw, sysc, opp = r.get("raw"), r.get("system"), r.get("opencode")
        delta = "—"
        if raw and sysc and raw.get("pass_rate") is not None and sysc.get("pass_rate") is not None:
            d = sysc["pass_rate"] - raw["pass_rate"]
            delta = f"{d:+.0%}"
        note = " ⚠ cell failed (see log)" if r.get("failed") else ""
        lines.append(f"| {r['label']}{note} | {_pct(raw)} | {_pct(sysc)} | {_pct(opp)} | {delta} |")
    lines.append("")
    lines.append("## Reproduce\n")
    lines.append("```bash")
    lines.append("# boot a Continuum core (serves your local model), then:")
    lines.append(f"python3 benchmarks/coder/matrix.py --models benchmarks/coder/models.json \\")
    lines.append(f"    --benchmark {args.benchmark} --limit {args.limit} --out benchmarks/coder/MATRIX.md")
    lines.append("```")
    lines.append("\nAdd a model = one row in `benchmarks/coder/models.json`. A bigger machine with more "
                 "VRAM sweeps more models with the identical command.")
    return "\n".join(lines) + "\n"


def main():
    ap = argparse.ArgumentParser(description="Sweep models × {RAW, OURS} + opponents into a matrix.")
    ap.add_argument("--models", required=True, help="JSON list of model rows")
    ap.add_argument("--benchmark", default="humaneval-rs")
    ap.add_argument("--gym", default=None,
                    help="gym jsonl for RAW arms; omitted -> follows --benchmark "
                         "(docs/genome/<benchmark>.jsonl) so RAW and SYSTEM always "
                         "grade the SAME tasks")
    ap.add_argument("--limit", type=int, default=40)
    ap.add_argument("--persona-id", default=None,
                    help="resident persona UUID; omitted -> headtohead resolves live from the core")
    ap.add_argument("--cu", default=os.path.expanduser("~/.continuum/cache/cargo-target/debug/cu"))
    ap.add_argument("--out", default=None)
    args = ap.parse_args()
    if not args.gym:
        args.gym = os.path.join(HERE, "..", "..", "docs", "genome", f"{args.benchmark}.jsonl")
        if not os.path.exists(args.gym):
            raise SystemExit(f"no gym file for benchmark '{args.benchmark}' at {args.gym} — "
                             "pass --gym explicitly")

    rows = json.load(open(args.models))
    tmp = tempfile.mkdtemp(prefix="matrix-")
    results = []
    for row in rows:
        res = h2h(row, args, tmp)
        if row.get("opponent") == "opencode":
            res["opencode"] = opencode_cell(row, args, tmp)
        results.append(res)

    table = render(results, args)
    if args.out:
        open(args.out, "w").write(table)
        print(f"\n[matrix] wrote {args.out}", file=sys.stderr)
    print("\n" + table)


if __name__ == "__main__":
    main()
