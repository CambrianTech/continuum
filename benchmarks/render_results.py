#!/usr/bin/env python3
"""
render_results.py — the ONE renderer for the benchmark evidence.

Reads the durable, append-only ledger `benchmarks/RESULTS.jsonl` (the single source of
truth — committed, so a number can never be lost or hand-fudged) and rewrites the
canonical evidence into the root README between the markers:

    <!-- BENCHMARKS:START -->  …generated…  <!-- BENCHMARKS:END -->

It emits TWO things from the same ledger:
  1. a committed SVG bar chart (`benchmarks/charts/coder-headline.svg`) — the visual
     "same weights, our loop beats the standard local harness" proof, embedded in the
     README as an image so it renders on GitHub without any external service; and
  2. the per-benchmark tables, with a Δ(OURS−opencode) column — the number that IS the
     sell: how much better the identical weights code inside continuum vs opencode.

Idempotent and repeatable: `python3 benchmarks/render_results.py` regenerates from the
ledger every time. A sweep appends rows to RESULTS.jsonl (matrix.py / benchmark runs do
this), then this renders them. No hand-editing the README's evidence — edit the data,
re-render.

Each ledger row: {benchmark, model, arm, score, total, pass_rate, mean_output_tokens,
excluded, captured, git_sha, machine, note}. arm ∈ {RAW, OURS, opencode, Hermes}.
"""
import json, os, sys
from collections import defaultdict

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LEDGER = os.path.join(ROOT, "benchmarks", "RESULTS.jsonl")
README = os.path.join(ROOT, "README.md")
CHART = os.path.join(ROOT, "benchmarks", "charts", "coder-headline.svg")
CHART_REL = "benchmarks/charts/coder-headline.svg"
START, END = "<!-- BENCHMARKS:START -->", "<!-- BENCHMARKS:END -->"

# Benchmark display order + one-line framing. INNER = fast verifiable gyms; OUTER = lab-grade.
BENCH_META = {
    # RETIRED (2026-08-23 doctrine): HumanEval derivatives are contaminated — public
    # sets measure memorization, not capability. History stays in the ledger and the
    # ALL-RESULTS page; the README face no longer carries the tier.
    "humaneval-rs": ("HumanEval-Rust", "function-level, rustc compile+run graded — RETIRED: contaminated public set", "retired"),
    "terminal-bench-2.1": ("Terminal-Bench 2.1", "real terminal tasks, official oracles, GOLD-GATED subset (official solution must pass on this host before a task counts — env-fail is named, never scored as a model 0)", "outer"),
    "hard-rs":      ("Hard-Rust", "expression evaluators + algorithmics", "inner"),
    "frontier-rs":  ("Frontier-Rust", "Dijkstra · Levenshtein · LIS · topo-sort · bignum · calc · regex", "inner"),
    "games-rs":     ("Games-Rust", "buildable game logic — Conway · win-checkers · 2048 merge · knight moves", "inner"),
    "swe-bench-lite":("SWE-bench Lite", "real GitHub issues in real repos, official swebench scorer", "outer"),
    # The whole-being battery (benchmarks/agent-solve/): the persona's COMPLETE self —
    # memory ON, genome loaded, tools ON, never stripped — dropped into seeded git repos
    # via agent/solve, one task at a time, graded by the repo's own asserts. These rows
    # are the LEARNING-CAPACITY curve: the same persona re-measured as the mind improves.
    "agent-solve-t1": ("Agent-Solve Tier 1", "whole-being seeded-repo bug fixes — single-file", "being"),
    "agent-solve-t2": ("Agent-Solve Tier 2", "whole-being — multi-file root-cause, invariants, implement-from-spec", "being"),
}
# The competing local coding CLIs people actually use. The Δ "sell" column is OURS minus the
# BEST of these (same weights) — the strongest honest single claim: we beat the best rival CLI.
OPPONENTS = ["opencode", "hermes", "aider", "mini-swe", "mini-swe-stock"]
# Arm → (display, bar color that reads on both light + dark GitHub canvases).
ARM = {
    "OURS":     ("OURS (Continuum)", "#2ea043"),
    "RAW":      ("RAW one-shot",     "#6e7681"),
    "opencode": ("opencode CLI",     "#d29922"),
    "hermes":   ("Hermes CLI",       "#a371f7"),
    "aider":    ("aider CLI",        "#58a6ff"),
    # Harness-vs-harness cells (2026-08-24): mini-SWE-agent is the top open harness
    # on SWE-bench-Verified. "stock" = ggml-org upstream llama-server, default flags
    # — THEIR WHOLE WORLD, no Continuum serving advancements — the honest full-stack
    # rival. "mini-swe" (no suffix) = same harness on OUR fork: isolates the
    # cognition layer with serving held equal.
    "mini-swe":       ("mini-SWE (our server)",   "#db61a2"),
    "mini-swe-stock": ("mini-SWE (stock llama)",  "#f85149"),
}
ARM_TABLE_ORDER = ["RAW", "OURS", "opencode", "hermes", "aider", "mini-swe", "mini-swe-stock"]


def _pr(row):
    """Real pass-rate float, or None if pending/excluded/absent."""
    if not row or row.get("excluded") or row.get("pass_rate") is None:
        return None
    return row["pass_rate"]


def cell(row):
    if row is None: return "—"
    if row.get("excluded"): return "*excluded¹*"
    if row.get("pass_rate") is None: return "*pending*"
    return f"{row['pass_rate']*100:.0f}% ({row['score']}/{row['total']})"


def delta(a, b):
    """Signed points a−b, or '—' when either arm has no real number."""
    pa, pb = _pr(a), _pr(b)
    if pa is None or pb is None: return "—"
    d = round((pa - pb) * 100)
    return f"**+{d}**" if d > 0 else ("±0" if d == 0 else str(d))


def best_rival_delta(arms):
    """OURS minus the BEST competing CLI (opencode/hermes/aider) on this model — the strongest
    honest single claim. '—' when OURS or every rival lacks a real number."""
    ours = _pr(arms.get("OURS"))
    rivals = [(_pr(arms.get(o)), o) for o in OPPONENTS if _pr(arms.get(o)) is not None]
    if ours is None or not rivals:
        return "—"
    best_pr, best = max(rivals)
    d = round((ours - best_pr) * 100)
    tag = {"opencode": "opencode", "hermes": "Hermes", "aider": "aider",
           "mini-swe": "mini-SWE", "mini-swe-stock": "mini-SWE(stock)"}[best]
    return (f"**+{d}** vs {tag}" if d > 0 else (f"±0 vs {tag}" if d == 0 else f"{d} vs {tag}"))


# ── SVG headline chart ──────────────────────────────────────────────────────
def build_svg(by_bench):
    """Grouped horizontal bar chart over the benchmarks that carry ≥1 real OURS number.
    One group per (benchmark, model), one bar per arm with a real pass_rate. Transparent
    background + mid-tone text so it reads on both README themes. Returns SVG text, or
    None when there is nothing real to plot yet."""
    groups = []  # (label, [(arm, pass_rate, score, total)])
    for b in BENCH_META:
        if b not in by_bench: continue
        disp = BENCH_META[b][0]
        for m in sorted(by_bench[b], key=lambda m: -( _pr(by_bench[b][m].get("OURS")) or -1)):
            arms = by_bench[b][m]
            bars = [(arm, _pr(arms.get(arm)), (arms.get(arm) or {}).get("score"),
                     (arms.get(arm) or {}).get("total"))
                    for arm in ARM_TABLE_ORDER if _pr(arms.get(arm)) is not None]
            if not bars: continue
            groups.append((f"{disp} · {m}", bars))
    if not groups:
        return None

    L, Rgap, W = 200, 108, 820         # left label col, right gutter (fits "100% (18/20)"), total width
    bar_h, bar_gap, grp_gap, hdr_h, top = 20, 5, 20, 20, 64
    plot_w = W - L - Rgap
    # height — each group is a header row + one row per bar + a gap
    y = top
    for _, bars in groups:
        y += hdr_h + len(bars) * (bar_h + bar_gap) + grp_gap
    H = y + 20
    txt = "#768390"

    s = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" '
         f'viewBox="0 0 {W} {H}" font-family="-apple-system,Segoe UI,Roboto,sans-serif">']
    s.append(f'<text x="20" y="26" font-size="17" font-weight="700" fill="{txt}">'
             f'Same weights, different harness — pass-rate (higher is better)</text>')
    s.append(f'<text x="20" y="46" font-size="12" fill="{txt}">rustc compile+run graded · '
             f'RAW = model one-shot · OURS = full Continuum loop · opencode = standard local agentic harness</text>')
    # gridlines 25/50/75/100
    for pct in (25, 50, 75, 100):
        gx = L + plot_w * pct / 100
        s.append(f'<line x1="{gx:.0f}" y1="{top-4}" x2="{gx:.0f}" y2="{H-16}" '
                 f'stroke="#8b949e" stroke-opacity="0.18" stroke-width="1"/>')
        s.append(f'<text x="{gx:.0f}" y="{H-4}" font-size="10" fill="{txt}" text-anchor="middle">{pct}%</text>')

    y = top
    for label, bars in groups:
        # group header on its OWN row (full width, no collision with arm labels)
        s.append(f'<text x="20" y="{y+14}" font-size="12" font-weight="700" fill="{txt}">{label}</text>')
        y += hdr_h
        for arm, pr, sc, tot in bars:
            disp, color = ARM[arm]
            bw = max(2, plot_w * pr)
            s.append(f'<rect x="{L}" y="{y}" width="{bw:.1f}" height="{bar_h}" rx="3" fill="{color}"/>')
            s.append(f'<text x="{L-8}" y="{y+14}" font-size="10.5" fill="{txt}" text-anchor="end">{disp}</text>')
            s.append(f'<text x="{L+bw+6:.1f}" y="{y+14}" font-size="10.5" font-weight="600" '
                     f'fill="{color}">{pr*100:.0f}% ({sc}/{tot})</text>')
            y += bar_h + bar_gap
        y += grp_gap
    s.append('</svg>')
    return "\n".join(s)


# ── README section ──────────────────────────────────────────────────────────
def render(by_bench, has_chart):
    out = []
    out.append("## Benchmarks — reproducible, definitive, never lost\n")
    out.append("Every number here is rendered from [`benchmarks/RESULTS.jsonl`](benchmarks/RESULTS.jsonl) — "
               "an append-only, committed ledger. Re-run a sweep, it appends; `python3 benchmarks/render_results.py` "
               "regenerates this section (chart included). No hand-edited claims: **edit the data, re-render.** "
               "Identical model weights across RAW / OURS / opencode, so every delta is an honest system effect, "
               "not a model-fit confound.\n")
    if has_chart:
        out.append(f"![Continuum vs opencode vs raw — coding pass-rate]({CHART_REL})\n")
    out.append("- **RAW** — the model one-shot against its own `/v1`.  ")
    out.append("- **OURS** — the same weights through the full continuum cognition loop (memory, tools, act→observe, recovery).  ")
    out.append("- **opencode / Hermes / aider / mini-SWE** — the same weights driven by the harnesses people actually use, on the same tasks + grader. **mini-SWE (stock)** is their WHOLE world — the top open harness on unmodified upstream llama-server with default flags; no Continuum serving advancements anywhere in that column.  ")
    out.append("- **Δ vs best rival CLI** — points OURS beats the *strongest* competing local coding CLI by, on identical weights. **This is the claim.**\n")

    for tier, title in [("outer", "### Lab-grade (the headline)"),
                        ("being", "### Whole-being battery (the learning-capacity curve)\n\n"
                                  "The persona's COMPLETE self — memory ON, genome loaded, tools ON, "
                                  "**never stripped to fit the benchmark** — dropped into seeded git repos "
                                  "one task at a time ([`benchmarks/agent-solve/`](benchmarks/agent-solve/)). "
                                  "The same persona re-measured over time as the mind improves: these rows "
                                  "are a learning curve, not a leaderboard. Opponent CLIs join on identical "
                                  "tasks as sibling arms."),
                        ("inner", "### Fast verifiable gyms (regression + training signal)")]:
        benches = [b for b in BENCH_META if BENCH_META[b][2] == tier and b in by_bench]
        if not benches: continue
        out.append(title + "\n")
        for b in benches:
            disp, frame, _ = BENCH_META[b]
            out.append(f"**{disp}** — {frame}\n")
            out.append("| model | RAW | OURS | opencode | Hermes | aider | mini-SWE | mini-SWE (stock) | Δ vs best rival |")
            out.append("|---|---|---|---|---|---|---|---|---|")
            models = by_bench[b]
            def key(m):
                pr = _pr(models[m].get("OURS"))
                return (-1 if pr is None else -pr, m)
            for m in sorted(models, key=key):
                a = models[m]
                mark = " *(we forged it)*" if "forged" in m else ""
                out.append(f"| **{m}**{mark} | {cell(a.get('RAW'))} | **{cell(a.get('OURS'))}** | "
                           f"{cell(a.get('opencode'))} | {cell(a.get('hermes'))} | {cell(a.get('aider'))} | "
                           f"{cell(a.get('mini-swe'))} | {cell(a.get('mini-swe-stock'))} | "
                           f"{best_rival_delta(a)} |")
            out.append("")
    out.append("¹ *excluded* = a serving/harness failure (degenerate output under GPU contention, a down endpoint) — "
               "never scored as a model 0%. The harness self-flags these ([`headtohead.py`](benchmarks/coder/headtohead.py)) "
               "so no false zero reaches this table.\n")
    out.append("² A blank **Hermes CLI** cell = Hermes hard-refuses that model: it requires ≥64K context and "
               "won't start below it. Every model here is served at its **real trained context** (read from GGUF "
               "metadata, memory-capped — never clamped down), so a 32K-native model like Qwen2.5-Coder genuinely "
               "cannot be run through Hermes without a quality-degrading rope-overflow. We mark it absent, not 0 — "
               "and note it's a point *for* the local models: Continuum runs the 32K-native coders Hermes turns away.\n")
    out.append("### The axis nobody else reports: cost & energy per solve\n")
    out.append(
        "Raw pass-rate is only half the contest. A metered cloud harness pays per token, "
        "every attempt, forever; a local mesh pays **once for the hardware** and then "
        "**\$0 per attempt** — which is why test-time compute (best-of-k, deep research, "
        "retries) is nearly free for us and prohibitive for them. On the axes below, a "
        "\$0-per-attempt local system playing the *same official exams* is not competing "
        "in their category — it defines its own.\n")
    out.append("| system | marginal \$/attempt | who pays the meter | can it retry/forage freely? |")
    out.append("|---|---|---|---|")
    out.append("| **OURS (Continuum, local)** | **\$0.00** | nobody — hardware is a one-time cost | **Yes** — depth, best-of-k, web research all free |")
    out.append("| mini-SWE / opencode on a cloud API | per-token, every call | the user, per run, forever | No — each retry/lookup costs money, so they stay lean |")
    out.append("| a datacenter frontier run | per-token + the grid's power & water | the public (subsidies) + the user | No — economics forbid deep per-task compute at scale |")
    out.append(
        "\n*Score-per-dollar and score-per-watt are computed per row when a run records "
        "`attempt_cost_usd` / `attempt_wh`; a local row is \$0 by construction. The point is "
        "the SHAPE: as the retake + transfer curves climb, our cost-per-solve stays flat at "
        "the hardware, while a metered rival's climbs with every attempt. Ingenuity over "
        "budget — average cards, creative strategy.*\n")
    out.append("**Every row ever recorded** — including retired gyms, excluded runs, and full history — renders to "
               "[`benchmarks/ALL-RESULTS.md`](benchmarks/ALL-RESULTS.md) from the same ledger. The tables above show "
               "each (benchmark, model, arm)'s LATEST row; the full page shows them all.\n")
    out.append("**Reproduce:** `continuum cognition/eval --persona_id <id> --eval_set <gym .jsonl>` runs a gym through a "
               "citizen's LIVE cognition — same model, faculties, and tools she serves with; the gyms ship in-repo, "
               "so `git clone` + a running core is the whole setup. Harness-only paths: "
               "`python3 benchmarks/coder/matrix.py --models benchmarks/coder/models.json --benchmark <name>` "
               "(inner gyms) · `python3 benchmarks/swe/run_ours.py --instance <id> --solver ours` (SWE-bench). "
               "All append to `RESULTS.jsonl`; re-render with `python3 benchmarks/render_results.py`.\n")
    return "\n".join(out)


def render_all_results(rows):
    """The VIEW-ALL page: every ledger row, categorized, newest first — nothing
    hidden, retired tiers included with their retirement reason."""
    tiers = {"outer": "## Lab-grade (external)", "being": "## Whole-being battery",
             "inner": "## Fast verifiable gyms", "retired": "## Retired gyms (history preserved; no longer on the README face)"}
    out = ["# All benchmark results — the complete ledger\n",
           "Rendered from [`RESULTS.jsonl`](RESULTS.jsonl) by `render_results.py`. "
           "Append-only: every run ever recorded is here, newest first. The README "
           "shows the latest row per cell; this page shows them all.\n"]
    by_tier = defaultdict(list)
    for r in rows:
        tier = BENCH_META.get(r["benchmark"], (r["benchmark"], "", "inner"))[2]
        by_tier[tier].append(r)
    for tier in ["outer", "being", "inner", "retired"]:
        if tier not in by_tier: continue
        out.append(tiers[tier] + "\n")
        out.append("| captured | benchmark | model | arm | result | machine | git sha | note |")
        out.append("|---|---|---|---|---|---|---|---|")
        for r in sorted(by_tier[tier], key=lambda r: r.get("captured", "") or "", reverse=True):
            res = "*excluded*" if r.get("excluded") else (
                "*pending*" if r.get("pass_rate") is None
                else f"{r['pass_rate']*100:.0f}% ({r.get('score')}/{r.get('total')})")
            disp = BENCH_META.get(r["benchmark"], (r["benchmark"],))[0]
            out.append(f"| {r.get('captured','—')} | {disp} | {r.get('model','—')} | {r.get('arm','—')} | "
                       f"{res} | {r.get('machine','—')} | `{str(r.get('git_sha','—'))[:9]}` | {r.get('note','')} |")
        out.append("")
    path = os.path.join(ROOT, "benchmarks", "ALL-RESULTS.md")
    open(path, "w").write("\n".join(out))
    print(f"[render_results] wrote benchmarks/ALL-RESULTS.md ({len(rows)} rows)")


SECRET_SHAPES = __import__("re").compile(r"hf_[A-Za-z0-9]{30}|sk-[A-Za-z0-9]{30}|xoxb-|ghp_[A-Za-z0-9]{30}|AKIA[A-Z0-9]{16}")

def refuse_secrets(text, where):
    """Published data may NEVER carry a credential (Joel 2026-08-25: 'idgaf unless
    it makes it into published data'). Refuse the whole render rather than ship one."""
    if SECRET_SHAPES.search(text):
        raise SystemExit(f"REFUSED: token-shaped string in {where} — scrub it, then re-render.")

def main():
    ledger_text = open(LEDGER).read()
    refuse_secrets(ledger_text, "benchmarks/RESULTS.jsonl")
    rows = [json.loads(l) for l in ledger_text.splitlines() if l.strip()]
    render_all_results(rows)
    by_bench = defaultdict(lambda: defaultdict(dict))  # bench -> model -> arm -> row (latest wins)
    for r in rows:
        by_bench[r["benchmark"]][r["model"]][r["arm"]] = r

    svg = build_svg(by_bench)
    has_chart = svg is not None
    if has_chart:
        os.makedirs(os.path.dirname(CHART), exist_ok=True)
        open(CHART, "w").write(svg)
        print(f"[render_results] wrote chart {CHART_REL}")

    section = render(by_bench, has_chart)
    text = open(README).read()
    block = f"{START}\n{section}\n{END}"
    if START in text and END in text:
        pre = text[:text.index(START)]
        post = text[text.index(END)+len(END):]
        open(README, "w").write(pre + block + post)
        print(f"[render_results] regenerated README benchmarks section from {len(rows)} ledger rows")
    else:
        print(section)
        print(f"\n[render_results] No {START}/{END} markers in README.md — add them where the evidence "
              f"should live, then re-run.", file=sys.stderr)


if __name__ == "__main__":
    main()
