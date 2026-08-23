# Frontier Benchmark Landscape — researched 2026-08-23

Web-researched (direct fetches of leaderboards, model cards, repos, tracker
sites, all dated 2026-08-23) after Joel's standing rule landed: **no toys, no
contaminated sets** ([[humaneval-is-prohibited-contaminated-benchmarks-carry-no-signal]]).
The question answered: *what are the standards frontier labs are actually
measured on mid-2026, which of them are frontier-HARD (frontier models < ~60%),
execution-graded, and importable under our adapter doctrine (task + oracle only,
the ROOM is the runner)?*

## The headline

**SWE-bench Verified is dead as a frontier signal** — Claude Opus 5 scores
97.0%, seven models ≥95% (vals.ai, 2026-08-19). LiveCodeBench classic is
saturating (Fable 5 89.8%). Anything our showcase records on those reads as a
solved-problem row. The frontier moved to the suites below.

## The ranked shortlist (frontier-hard × execution-graded × importable × prestige)

| # | Benchmark | Frontier ceiling (8/2026) | Oracle | Import path |
|---|---|---|---|---|
| 1 | **SWE-bench Pro** (Scale AI) — 731 public of 1,865 tasks, 41 pro repos | Muse Spark 1.1 61.5%, GPT-5.4 59.1%, Opus 4.6 51.9%, Gemini 3.1 Pro 46.1% | execution: F2P/P2P suites, per-instance Docker (`jefzda/sweap-images`) | HF `ScaleAI/SWE-bench_Pro`, MIT eval code, standalone |
| 2 | **Terminal-Bench 3.0 / Frontier Bench** (Stanford × Laude) — 74 tasks, 7 domains | GPT-5.6 Sol 34.4%, Fable 5 33.8%, Opus 4.8 21.1% — hardest widely-cited suite | execution: separate agent + verifier containers, artifacts re-gradeable | public registry `terminal-bench-core` (tbench.ai); task = container + verifier script |
| 3 | **SWE-rebench** (Nebius) — rolling window (111 problems May–Jul window), 21k+ corpus | Fable 5 64.5%, Grok 4.5 63.8%, Opus 5 63.4% | execution: F2P/P2P in per-instance Docker (7,500 prebuilt images) | HF `nebius/SWE-rebench` CC-BY-4.0 — **identical schema to SWE-bench** = our cheapest import; rolling window kills the contamination objection |
| 4 | **MirrorCode** (Epoch × METR) — reimplement whole programs from behavior, 30 task-configs | Fable 5 64%, GPT-5.6 Sol **20%** | execution: exact-match visible + ~34% held-out tests, 100% required | MIT, github.com/epoch-research; declare a smaller token budget honestly (official is 10B tok/attempt) |
| 5 | **DeepSWE** (Datacurve) — 113 from-scratch long-horizon tasks, contamination-free by construction | Opus 5 74%, GPT-5.6 Sol 73%, Fable 5 70% | execution: hand-written per-task program verifiers, sandboxed | public subset 49/62 configs — verify coverage first |
| 6 | **MLE-bench** (OpenAI) — 75 Kaggle comps; the **High split (~42% any-medal)** is the hard part | best agents ~60% overall; High ~42% | execution: `mlebench grade-sample` vs held-out keys — cleanest standalone grader on the list | github.com/openai/mle-bench; heavy (158GB Lite, GPU-hours) |
| 7 | **SWE-Lancer IC-Diamond** (OpenAI) — 198 real-dollar Expensify freelance tasks | no maintained 2026 leaderboard (aging) | execution: **full-app Playwright e2e user-flow tests** — the only prestige suite whose oracle is "the feature works in the app" | github.com/openai/preparedness; per-task Docker ~14GB |
| 8 | **ProgramBench** — 200 from-scratch rebuilds | Opus 5 82.3% raw but **6/200 fully resolved** | execution: hidden behavioral tests | verify the hidden test sets actually ship before committing |

## Excluded, with reasons

SWE-bench Verified (saturated — floor/sanity check only) · LiveCodeBench
(saturating; contest ≠ project) · Vibe Code Bench (exactly the one-shot-app
category but proprietary AND near-saturated: Fable 5 90.35%) · FrontierCode
(rubric-judged — fails the execution bar) · CursorBench (closed) · WebDev Arena
(human-pref Elo) · Design2Code (judge-scored, inactive) · Aider Polyglot (stale,
drifting toy) · AppWorld (great oracle, but API-orchestration not SWE; optional
breadth add) · Commit0 (superseded by MirrorCode/ProgramBench) · OSWorld 2.0
(computer-use, out of scope) · METR time-horizons (methodology, not a task set —
cite, don't import).

## What this means for the showcase (the adapter queue)

1. **SWE-rebench first** — same schema as the SWE-bench adapters we already
   have, so it's the fastest path from "toy rows" to "frontier-hard rows with a
   public leaderboard that already includes open models for direct comparison."
   The rolling window is the honest answer to "your local model trained on it."
2. **SWE-bench Pro public second** — the prestige successor; identical task
   shape, prebuilt per-instance images, frontier at 46-61%.
3. **Terminal-Bench 2.1 + 3.0 third** — terminal-native fits our citizens'
   shell-first hands exactly; 3.0's separate-verifier-container design matches
   our glass-box doctrine. 2.1 is the mid-rung (frontier 74-84%), 3.0 the
   summit (frontier ~34%).
4. **MirrorCode public** as the from-scratch tier (exact-match oracle, MIT,
   tiny import) — declare our token budget on the row.
5. MLE-bench High / SWE-Lancer Diamond as the long-horizon e2e tier when the
   disk + hours budget allows.

Env note: the per-instance Docker images are x86-first; on Apple Silicon we
rebuild envs our own way (the SWE-bench era-venv precedent) — adapter doctrine
imports task + oracle, never the maintainer's harness, so this is the designed
path, not a workaround.

One-shot app challenges (the viral fluid-sim shape) have a formal home:
Vibe Code Bench — proprietary for now. Our in-house import of the circulating
prompt (docs/genome/design/fluid-sim-task.md, §6b of the design-bench doc) is
the right stand-in until it opens.
