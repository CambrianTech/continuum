#!/usr/bin/env python3
"""
matrix.py — reproducible benchmark MATRIX: runners × benchmarks → a chart.

Runs every (runner, benchmark) pair from a config, collects pass@1, and emits CHART.md
(a markdown table = the chart) + results.json. This replaces the manual one-off runs:
one command reproduces the whole board.

Runners (a runner is anything that can attempt a benchmark):
  • "opponent" — an EXTERNAL OpenAI-compatible /v1 endpoint you bring up (a local
    llama-server, an unsloth gateway, ollama, a cloud API, or an airc node). One-shot.
    ZERO dependency on us — we never require it.
  • "ours"     — a benchmark run THROUGH the Continuum core (full system: RAG + tools +
    PX + act→observe), via `cu cognition/eval`. Warm-gated by the eval itself.
  • (future)   — "team": a coordinated set of personas working one shared plan/kanban.
    Same interface; drops into the matrix with no runner-specific charting code.

Published claims: put a model's own published number for a benchmark in the config and it
renders in the chart next to what WE measured — so an "amazing claim" meets a real, common
benchmark we reproduced identically. Take them on their numbers.

Requirements: python3 (stdlib), plus each opponent's /v1 up and (for "ours") a running core.
`rustc` for the coder gym grader. No pip installs.

Usage:
  python3 benchmarks/matrix.py benchmarks/config.example.json
"""
import argparse, json, os, re, subprocess, sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)


def _resolve(p):
    return p if os.path.isabs(p) else os.path.join(REPO, p)


def run_opponent(r, bench):
    """Score an external /v1 one-shot via the standalone harness."""
    cmd = [
        sys.executable, os.path.join(HERE, "coder", "oneshot_opponent.py"),
        "--endpoint", r["endpoint"], "--model", r["model"], "--label", r["label"],
        "--gym", _resolve(bench["gym"]), "--limit", str(bench.get("limit", 40)),
    ]
    if r.get("api_key"):
        cmd += ["--api-key", r["api_key"]]
    out = subprocess.run(cmd, capture_output=True, text=True, timeout=bench.get("timeout", 5400))
    m = re.search(r"\|[^|]*\|\s*(\d+)/(\d+)\s*\|\s*(\d+)%", out.stdout)
    if m:
        return {"passed": int(m.group(1)), "total": int(m.group(2)), "pass_rate": int(m.group(3)) / 100}
    return {"error": (out.stdout + "\n" + out.stderr).strip()[-240:]}


def run_ours(r, bench):
    """Score a benchmark THROUGH the Continuum core (full system), via cu cognition/eval."""
    cu = r.get("cu", os.path.expanduser("~/.continuum/cache/cargo-target/debug/cu"))
    gym = _resolve(bench["gym"])
    limit = bench.get("limit", 40)
    slice_path = f"/tmp/mtx_{os.getpid()}_{bench['name']}.jsonl"
    with open(gym) as f, open(slice_path, "w") as o:
        for i, line in enumerate(f):
            if i >= limit:
                break
            o.write(line)
    cmd = [
        cu, "cognition/eval", "--persona_id", r["persona"], "--eval_set", slice_path,
        "--max_acts", str(bench.get("max_acts", 6)), "--max_retries", str(bench.get("max_retries", 0)),
    ]
    out = subprocess.run(cmd, capture_output=True, text=True, timeout=bench.get("timeout", 5400))
    try:
        os.remove(slice_path)
    except OSError:
        pass
    m = re.search(r"\{.*\}", out.stdout, re.S)
    if not m:
        return {"error": (out.stdout + "\n" + out.stderr).strip()[-240:]}
    d = json.loads(m.group(0))
    res = d.get("results") or []
    inf = sum(1 for x in res if "inference failed" in str(x.get("grade", "")))
    if inf:
        return {"error": f"{inf} inference errors (cold model? the eval warm-gate should prevent this)"}
    return {"passed": sum(1 for x in res if x.get("ok")), "total": len(res), "pass_rate": d.get("pass_rate", 0.0)}


RUNNERS = {"opponent": run_opponent, "ours": run_ours}


def cell(result, published):
    if result is None:
        return "—"
    if "error" in result:
        return f"ERR"
    s = f"{result['pass_rate']:.0%}"
    if published is not None:
        s += f" _(claim {published:.0%})_"
    return s


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("config")
    ap.add_argument("--out", default=os.path.join(HERE, "CHART.md"))
    ap.add_argument("--json", default=os.path.join(HERE, "results.json"))
    args = ap.parse_args()

    cfg = json.load(open(_resolve(args.config)))
    benches = cfg["benchmarks"]
    runners = cfg["runners"]

    results = {}  # (runner_label, bench_name) -> result
    for b in benches:
        for r in runners:
            fn = RUNNERS.get(r["kind"])
            if not fn:
                results[(r["label"], b["name"])] = {"error": f"unknown runner kind {r['kind']}"}
                continue
            print(f"→ {r['label']} × {b['name']} …", file=sys.stderr)
            try:
                results[(r["label"], b["name"])] = fn(r, b)
            except Exception as e:  # a runner failing must not sink the whole matrix
                results[(r["label"], b["name"])] = {"error": str(e)[-240:]}
            print(f"   {cell(results[(r['label'], b['name'])], None)}", file=sys.stderr)

    # Chart: rows = runners, cols = benchmarks.
    lines = ["# Benchmark matrix", "",
             "Reproduce: `python3 benchmarks/matrix.py <config.json>`. `_(claim …)_` = the "
             "model's own published number for that benchmark, shown next to what we measured "
             "identically. Opponents are external /v1 (zero dependency).", ""]
    header = "| runner | " + " | ".join(b["name"] for b in benches) + " |"
    sep = "|" + "---|" * (len(benches) + 1)
    lines += [header, sep]
    for r in runners:
        row = [r["label"]]
        for b in benches:
            res = results.get((r["label"], b["name"]))
            pub = (r.get("published") or {}).get(b["name"])
            row.append(cell(res, pub))
        lines.append("| " + " | ".join(row) + " |")
    open(args.out, "w").write("\n".join(lines) + "\n")

    json.dump(
        {f"{k[0]} × {k[1]}": v for k, v in results.items()},
        open(args.json, "w"), indent=2,
    )
    print("\n".join(lines))


if __name__ == "__main__":
    main()
