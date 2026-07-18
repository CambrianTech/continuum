# Benchmark matrix — the methodical plan

Goal: beat Hermes AND opencode across many models and benchmarks, every number reproducible,
publishable on the README. Re-runnable on a bigger machine (more models) with the same commands.

**The matrix is `models × benchmarks × harnesses`.** Harnesses: RAW (one-shot /v1), OURS
(full Continuum loop), opencode (agentic, fairness-shimmed). Hermes is a MODEL row, not a column.

## The one hard rule: no cell from a mis-configured harness

A number is only allowed into the board once its harness passes a **fairness gate**. No hacking a
cell to look good; if the setup is wrong, fix the setup, then measure. Each harness gets its model
its BEST shot (its native tool format, its real context window) — then whatever delta remains is
honestly the harness's, not a config artifact.

## Phase 0 — set the three harnesses up right (preconditions)

| harness | gate | status |
|---|---|---|
| Continuum (OURS) | speak-only for spoken exams; tools kept for `-acted`/workspace tasks (both paths proven) | speak path ✅ (Devstral 0→100); tool path gate pending |
| opencode | drives the model via shim, writes to an explicit path, no gym-framing conflict; proven on ≥2 models | 14B ✅ (90/75); 2nd-model gate pending |
| Hermes | a real `ModelSpec` catalog row so it flows through OURS + opencode like any model | row added; serve+run validation pending |

## Phase 1 — fill the model axis (benchmark = humaneval-rs, the one validated grader)

Models (all pulled, in catalog): Hermes-8B, Qwen2.5-Coder-1.5B/3B/14B/32B, qwen3.5-4b-forged,
Devstral-24B. Each × {RAW, OURS, opencode where a lane exists}. `matrix.py --limit 40`.

## Phase 2 — add the outlier-B benchmark: SWE-bench Lite (repo-level, agentic)

The gold standard — real GitHub bugs, official Docker grader. `run_ours.py` has the spine
(`--solver gold` validated); the workspace-root seam is the known open bug. Run the SAME three
harnesses on the SAME instances (ours = persona hands on the clone; opencode = its agentic loop;
Hermes = one of the models). This proves the harnesses generalize from function-level → repo-level.

## Phase 3 — scale

Bigger machine, more models, full 40/300-task N. Identical commands; only `models.json` grows.

## Reproduce

`python3 benchmarks/coder/matrix.py --models benchmarks/coder/models.json --benchmark humaneval-rs --limit 40`
(+ the opencode lane/shim for that column — see `SCOREBOARD.md`).
