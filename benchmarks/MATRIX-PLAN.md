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

## The second rule: coached runs are labelled, never laundered

A persona may be helped during a run — by a human, a peer, or another persona. That is doctrine,
not an exception: see [docs/cognition/NEVER-ISOLATED.md](../docs/cognition/NEVER-ISOLATED.md).
Coaching is how we attribute a failure to the right layer (her reasoning vs our substrate), and
every number here is a joint measurement of both.

What that costs the board is light bookkeeping:

- **Published cells are solo unless the cell says otherwise**, and a coached cell records what
  was said and when. Both kinds are legitimate; only conflating them isn't.
- **Every hint means a defect.** A hint says the substrate failed to tell her something it should
  have. File the card, then fix it — a hint is a symptom report, never the repair. This is the
  bullet that actually earns its keep; the rest is filing.

We are not trying to out-lawyer ourselves here. Overfitting and stripping-to-pass were settled
long ago by the charter (*never rig the persona; fix the substrate*), and the online gyms that
will eventually prove our worth are adversarial by construction — she cannot be coached through
those at all. The labelling rule exists so our own boards stay readable, not because anyone here
is trying to cheat.

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

The gold standard — real GitHub bugs, official Docker grader. `benchmark/swe-solve` +
`benchmark/swe-grade` own this end-to-end (the Python spine that used to live in
`benchmarks/swe/run_ours.py` is deleted — Rust commands, handles, events). Run the SAME three
harnesses on the SAME instances (ours = persona hands on the clone; opencode = its agentic loop;
Hermes = one of the models). This proves the harnesses generalize from function-level → repo-level.

## Phase 3 — scale

Bigger machine, more models, full 40/300-task N. Identical commands; only `models.json` grows.

## Reproduce

`python3 benchmarks/coder/matrix.py --models benchmarks/coder/models.json --benchmark humaneval-rs --limit 40`
(+ the opencode lane/shim for that column — see `SCOREBOARD.md`).
