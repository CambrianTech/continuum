#!/usr/bin/env python3
"""
render_results.py — the ONE renderer for the benchmark evidence table.

Reads the durable, append-only ledger `benchmarks/RESULTS.jsonl` (the single source of
truth — committed, so a number can never be lost or hand-fudged) and rewrites the
canonical evidence tables into the root README between the markers:

    <!-- BENCHMARKS:START -->  …generated…  <!-- BENCHMARKS:END -->

Idempotent and repeatable: `python3 benchmarks/render_results.py` regenerates from the
ledger every time. A new sweep appends rows to RESULTS.jsonl (matrix.py / benchmark
runs do this), then this renders them. No hand-editing the README's evidence — edit the
data, re-render.

Each ledger row: {benchmark, model, arm, score, total, pass_rate, mean_output_tokens,
excluded, captured, git_sha, machine, note}. arm ∈ {RAW, OURS, opencode, Hermes}.
"""
import json, os, sys
from collections import defaultdict

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LEDGER = os.path.join(ROOT, "benchmarks", "RESULTS.jsonl")
README = os.path.join(ROOT, "README.md")
START, END = "<!-- BENCHMARKS:START -->", "<!-- BENCHMARKS:END -->"

# Benchmark display order + one-line framing. INNER = fast verifiable gyms; OUTER = lab-grade.
BENCH_META = {
    "humaneval-rs": ("HumanEval-Rust", "function-level, rustc compile+run graded", "inner"),
    "hard-rs":      ("Hard-Rust", "expression evaluators + algorithmics", "inner"),
    "frontier-rs":  ("Frontier-Rust", "Dijkstra · Levenshtein · LIS · topo-sort · bignum · calc · regex", "inner"),
    "swe-bench-lite":("SWE-bench Lite", "real GitHub issues in real repos, official swebench scorer", "outer"),
}
ARM_ORDER = ["RAW", "OURS", "opencode", "Hermes-3-Llama-3.1-8B"]

def cell(row):
    if row is None: return "—"
    if row.get("excluded"): return "*excluded¹*"
    if row.get("pass_rate") is None: return "*pending*"
    return f"{row['pass_rate']*100:.0f}% ({row['score']}/{row['total']})"

def render():
    rows = [json.loads(l) for l in open(LEDGER) if l.strip()]
    by_bench = defaultdict(lambda: defaultdict(dict))  # bench -> model -> arm -> row (latest wins)
    for r in rows:
        by_bench[r["benchmark"]][r["model"]][r["arm"]] = r

    out = []
    out.append("## Benchmarks — reproducible, definitive, never lost\n")
    out.append("Every number here is rendered from [`benchmarks/RESULTS.jsonl`](benchmarks/RESULTS.jsonl) — "
               "an append-only, committed ledger. Re-run a sweep, it appends; `python3 benchmarks/render_results.py` "
               "regenerates this section. No hand-edited claims: **edit the data, re-render.** Identical model weights "
               "across RAW / OURS / opencode, so every delta is an honest system effect, not a model-fit confound.\n")
    out.append("- **RAW** — the model one-shot against its own `/v1`.  ")
    out.append("- **OURS** — the same weights through the full continuum cognition loop (memory, tools, act→observe, recovery).  ")
    out.append("- **opencode** — the same weights through the opencode agentic harness (fair narrated-tool-call shim).  ")
    out.append("- **Hermes-3-8B** — a fixed opponent baseline.\n")

    for tier, title in [("outer", "### Lab-grade (the headline)"), ("inner", "### Fast verifiable gyms (regression + training signal)")]:
        benches = [b for b in BENCH_META if BENCH_META[b][2] == tier and b in by_bench]
        if not benches: continue
        out.append(title + "\n")
        for b in benches:
            disp, frame, _ = BENCH_META[b]
            out.append(f"**{disp}** — {frame}\n")
            out.append("| model | RAW | OURS | opencode | Hermes-3-8B |")
            out.append("|---|---|---|---|---|")
            models = by_bench[b]
            # order: biggest OURS pass_rate first, pending/excluded last
            def key(m):
                o = models[m].get("OURS", {})
                pr = o.get("pass_rate")
                return (-1 if pr is None else -pr, m)
            for m in sorted(models, key=key):
                a = models[m]
                herm = a.get("Hermes-3-Llama-3.1-8B")
                mark = " *(we forged it)*" if "forged" in m else ""
                out.append(f"| **{m}**{mark} | {cell(a.get('RAW'))} | **{cell(a.get('OURS'))}** | "
                           f"{cell(a.get('opencode'))} | {cell(herm)} |")
            out.append("")
    out.append("¹ *excluded* = a serving/harness failure (degenerate output under GPU contention, a down endpoint) — "
               "never scored as a model 0%. The harness self-flags these ([`headtohead.py`](benchmarks/coder/headtohead.py)) "
               "so no false zero reaches this table.\n")
    out.append("**Reproduce:** `python3 benchmarks/coder/matrix.py --models benchmarks/coder/models.json --benchmark <name>` "
               "(inner gyms) · `python3 benchmarks/swe/run_ours.py --instance <id> --solver ours` (SWE-bench). "
               "Both append to `RESULTS.jsonl`; re-render with `benchmarks/render_results.py`.\n")
    return "\n".join(out)

def main():
    section = render()
    text = open(README).read()
    block = f"{START}\n{section}\n{END}"
    if START in text and END in text:
        pre = text[:text.index(START)]
        post = text[text.index(END)+len(END):]
        text = pre + block + post
    else:
        # No markers yet — print the block for manual placement (don't guess where to inject).
        print(section)
        print(f"\n[render_results] No {START}/{END} markers in README.md — add them where the evidence table should live, then re-run.", file=sys.stderr)
        return
    open(README, "w").write(text)
    print(f"[render_results] regenerated README benchmarks section from {len(open(LEDGER).readlines())} ledger rows")

if __name__ == "__main__":
    main()
