# Benchmark Target Map — our collections ↔ the K3 frontier charts

**Status:** flywheel gap #1 (2026-07-28, BigMama). Maps our benchmark collections onto the named
benches on the Kimi-K3 frontier leaderboard (the [[benchmark-learning-flywheel]] measuring stick), so
K3's local numbers are comparable to the published charts. Companion to
[BENCHMARK-AS-LEARNING-FLYWHEEL](BENCHMARK-AS-LEARNING-FLYWHEEL.md).

## The two states of a benchmark in our catalog
- **RUNNABLE** — has an `eval_set` jsonl + a live grader (`cognition/eval`). 7 today.
- **CATALOGUED** — named in `known_benchmarks()` with `eval_set: None`. A placeholder that reserves
  the name + description; needs a runner + dataset to become runnable. ~18 today.

## Target: ALL 14 chart benches (Joel: "we will want to target all of these")
Coding (6): DeepSWE · Terminal Bench 2.1 · FrontierSWE · Program Bench · Kimi Code Bench 2.0 · SWE Marathon.
General Agents (6): GDPval-AA v2 Elo · JobBench · AA-Briefcase Elo · SpreadsheetBench 2 · Automation Bench · BrowseComp.
Visual Agents (2): CharXiv (RQ) w/ tool · Zerobench w/ tool (Pass@5).
Each row below is one target; "RUNNER TYPE" names the harness class it needs (several benches share one harness → build the harness once, unlock many).

## The map (chart bench → our proxy → runnable target)

| K3 chart bench | What it measures | Our RUNNABLE proxy today | Runnable TARGET to build (catalogued now) |
|---|---|---|---|
| **Program Bench**, LiveCodeBench | competitive-programming, single-file | `frontier-rs` (12), `hard-rs` (8) | `livecodebench`, `apps` |
| DeepSWE, **FrontierSWE**, **SWE Marathon** | repo-level patch that passes tests | `tool-bugfix-rs` (partial: fix-to-green loop) | `swe-bench-lite` → `swe-bench-verified`, `swe-lancer` |
| **Terminal Bench 2.1** | end-to-end tasks in a real shell | — (none) | `terminal-bench` |
| Kimi Code Bench 2.0 | mixed practical coding | `coder-eval` (13), `humaneval-rs` (164) | `bigcodebench`, `evalplus` |
| **Automation Bench**, **BrowseComp**, JobBench | agent driving real apps/web | — (none) | `webarena`, `appworld` |
| SpreadsheetBench 2 | spreadsheet manipulation | — (none) | (no catalog entry — add `spreadsheet-bench`) |
| **CharXiv**, Zerobench (visual) | chart/figure reasoning w/ vision | `webdev-rs` (perception grader, UI-adjacent) | `design2code` (screenshot→HTML) |
| GDPval-AA, AA-Briefcase (general Elo) | broad agentic knowledge work | — (none) | (Elo-style; needs opponent-pool harness) |
| **JobBench** | applying for / doing job tasks | — (none) | agentic-app harness (shares webarena runner) |
| Kimi Code Bench 2.0 | mixed practical coding (internal) | `coder-eval`, `humaneval-rs`, `livecodebench-rs` | — (proxied; no public dataset) |

**Bold** = benches where Kimi K3 tops or near-tops the chart — our highest-value targets, because
that's where reproducing the number locally is the strongest proof.

## Runner-type consolidation (build the harness once → unlock many)
The 14 chart benches need only **5 harness classes** — this is the actual build backlog:
1. **Rust-graded single-file** (LIVE): Program Bench, Kimi Code Bench, partial FrontierSWE-tier →
   `livecodebench-rs` ✓ (built today), `frontier-rs`, `hard-rs`. **Extend by authoring more tasks.**
2. **Repo-patch / SWE harness** (M5 spine #1945, one seam open): DeepSWE, FrontierSWE, SWE Marathon,
   SWE-Lancer → `swe-bench-lite/verified`. **Highest leverage: 3 chart benches from one runner.**
3. **Real-shell harness**: Terminal Bench 2.1 → `terminal-bench`. The persona's tool-executor already
   runs shell; the runner is the task-loader + pass/fail on final state.
4. **Agentic-app harness** (heaviest): Automation Bench, BrowseComp, JobBench, AppWorld, GDPval,
   AA-Briefcase → `webarena`, `appworld`. Needs real app servers + the doer-toolset (beta plan).
   Elo variants (GDPval/Briefcase) add an opponent-pool scorer on top.
5. **Vision-tool harness**: CharXiv, Zerobench, SpreadsheetBench 2, Design2Code → the eye-node
   (`perception/observe`) already grades `webdev-rs`; extend to chart/figure QA + spreadsheet ops.
   K3 is vision-native (the fork's K3 processor strips vision today; re-enabling it is a later lane).

## Priority to make runnable (highest proof-value first)
1. **`swe-bench-lite`** — the SWE-family anchors THREE chart benches (DeepSWE/FrontierSWE/SWE-Marathon)
   and K3 wins SWE-Marathon. M5 has the runner spine (PR #1945: gold RESOLVED, hermetic filter; the
   workspace-root seam is the one open bug). **Closing that seam makes the single highest-leverage
   target runnable.** → M5's lane; BigMama consumes it as a K3 serving target.
2. **`livecodebench`** — proxies Program Bench (K3 #1). Contamination-free, single-file → reuses the
   existing Rust `test_grade` path; needs the dataset loader, not a new grader. Achievable in-tree.
3. **`terminal-bench`** — K3 #2, and the purest "doer" bench. Needs a real-shell harness (the persona's
   tool-executor already runs shell) — the tool loop IS most of the runner.
4. **`webarena` / `appworld`** — Automation/BrowseComp (K3 #1). Heaviest (real app servers); defer
   until the doer-toolset (beta plan) is hardened.

## What ships now vs later
- **Now (this doc):** the map is the deliverable — every future K3 measurement states which chart
  bench it proxies, so our local ladder is legible against the published charts.
- **Now (our ladder, honest):** on the RUNNABLE proxies, the 2026-07-28 head-to-head established the
  local scale — compacted-19b vs Kimi-48B: humaneval-rs 75/85, hard-rs 25/50, frontier-rs 33/67. When
  K3 lands, the SAME three proxies give the first frontier-vs-frontier local number.
- **Later:** the priority list above makes the named chart benches runnable, one at a time, M5 owning
  the SWE/harness runners (her framework lane) and BigMama owning the K3 serving target + the
  single-file Rust-graded ones (livecodebench/apps).

## The discipline (keep the instrument honest)
Every runnable bench added inherits the merged proof discipline: warm-gate, same-model control
(base_model_id), held-out split (flywheel gap #3 — train on train-shard, chart on held-out), fail-loud
VOID cells. A chart number we can't reproduce under these controls is a claim we don't make.
