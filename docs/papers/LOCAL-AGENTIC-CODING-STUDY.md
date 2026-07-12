# Local Agentic Coding on Consumer Hardware: A Fair, Reproducible Comparison of Models and Harnesses

**Status: DESIGN + PILOT. Results sections fill exclusively from the evidence ledger
(`~/.continuum/benchmarks/ledger.jsonl` via `cu benchmark/matrix`) — no hand-authored
numbers, same recipe→artifact doctrine as the forge alloy.**

## Abstract (to be finalized from results)

We compare open-weight language models (4B–36B) and open-source agentic harnesses
(this system, aider, opencode, one-shot baseline) on software-engineering benchmarks,
entirely on one consumer machine (Apple M-series, 64GB). We additionally measure the
effect of (a) multi-agent review and (b) LoRA "genome" adaptation trained from the
system's own usage, on the same weights. Every reported cell carries the exact command
that reproduces it.

## 1. Research questions

- **RQ1 (models):** on identical harness + hardware, how do common open models rank,
  and do smaller models beat larger fine-tunes (incl. Hermes-4.3-36B)?
- **RQ2 (harnesses):** does an agentic loop with recovery/verification lift a FIXED
  model over its one-shot self and over aider/opencode with the same weights?
- **RQ3 (adaptation):** does a LoRA trained from the system's own successful acts lift
  the same weights (before/after, no other change)?
- **RQ4 (teaming):** does a reviewer persona lift a fixed model+harness?
- **RQ5 (cost):** what do the wins cost — wall-clock and $/resolved-task on named
  consumer hardware?

## 2. Fairness protocol (binding)

1. **Structural-first symmetric**: a 0% or degenerate cell in ANY arm (ours included)
   is glass-boxed before it is reported — serving config, template mismatch, output
   floor (mean tokens/task) — short of patching the competitor's repo. Degenerate
   cells are flagged "serving suspect," re-run once, and never reported as a loss.
2. **Champions, not strays**: competitor arms run the models their communities
   recommend (documented per round with source links).
3. **Replicate-then-compare**: before any cross-harness comparison ships, we reproduce
   the competitor's own published number for that model+benchmark on our hardware,
   within stated tolerance. Both rows appear.
4. **No learning during exams**: models sit exams as fresh snapshots in isolated
   lanes; no memory or gradient from exam content persists (proctored-exam protocol;
   post-hoc amnesia flash where a living persona is examined).
5. **Best-known config per model**: template from the GGUF itself, thinking mode per
   the model card, per-model sampling where the card specifies it.
6. **Everything ships**: honest zeros, suspect flags, and losses stay in the ledger
   and the paper.

## 3. Statistical plan

- **Primary metric**: resolve/pass rate per (model × harness × benchmark) cell, with
  **Wilson 95% intervals**. Pilot n=20 explicitly CANNOT separate 95% vs 80%
  (intervals [76,99.9] vs [58,92] overlap) — pilots select candidates only.
- **Definitive runs**: the FULL task set per benchmark (humaneval-rs: 164; polyglot:
  225; SWE-lite slice: ≥25 instances), greedy decoding (temperature 0) so task count
  is the sample size and runs are deterministic-reproducible; any stochastic arm gets
  3 seeds with mean ± range.
- **Paired comparison**: same task set for every arm → McNemar's test on paired
  pass/fail for model-vs-model and harness-vs-harness claims; report discordant
  counts, p-values, and effect sizes, not just rates.
- **Multiple comparisons**: Holm-Bonferroni across the headline claims.
- **Diversity**: ≥3 benchmark families (function-level, repo-edit/polyglot,
  agentic/SWE) before any general claim; per-family results never pooled.

## 4. Hardware & cost accounting

Primary rig: MacBook (Apple M-series Pro, 64GB unified, named exactly in results).
Secondary (planned): RTX 3090/5090-class. Per cell: wall-clock, generated tokens,
tok/s, and $/resolved-task derived from measured watts × local energy price (and
cloud-API list price for any cloud reference rows).

## 5. Arms

| arm | description |
|---|---|
| raw | one-shot against the model's /v1, no tools (oneshot runner) |
| ours | this system's full act→observe loop, snapshot lane |
| ours+team | + reviewer persona (RQ4) |
| ours+genome | + LoRA from own usage, same weights baseline printed beside (RQ3) |
| aider | aider CLI, its recommended config per model |
| opencode | opencode, native-tool-call endpoint provided (fairness note) |

## 6. Pilot results (2026-07-11/12, n=20 humaneval-rs, OURS arm only — candidates, NOT claims)

Devstral-24B 19/20; Qwen2.5-Coder-14B 18/20 (67s); Qwen3-Coder-30B-A3B 18/20;
Hermes-4.3-36B 16/20 (1129s); Hermes-3-8B 9/20; forged-4B flagged serving-suspect.
Full table + replication commands: `cu benchmark/matrix`.

## 7. Threats to validity (running list)

- Single-machine, single-quantization (Q4_K_M) — quant sensitivity unmeasured.
- humaneval family contamination risk in pretraining — mitigated by weighting
  polyglot/SWE/mined-fresh tasks for headline claims.
- Our harness co-evolved with Devstral (persona base) — home-field advantage named;
  RQ2's fixed-model cross-harness design is the control.
- Grader is rustc/pytest outcome-based — partial credit invisible.

## 8. Exhibits — annotated interaction traces

Quantitative cells say WHAT happened; exhibits show HOW. Each exhibit is a verbatim
act→observe trace from the capture record (provenance: capture file + timestamp),
annotated, never edited beyond truncation marks. Planned set, one per claim:

- **E1 Recovery**: a model receives a rustc error mid-exam and returns the corrected
  solution — the loop mechanism behind RQ2's lift.
- **E2 Self-verification**: write → compile → run → posts REAL program output (the
  wordstats chain, live room, 2026-07-11).
- **E3 Team review** (RQ4): reviewer persona catches a defect the solver missed;
  both turns shown.
- **E4 Honest failure**: a full trace of a zero — the search-loop exam texture —
  because showing the failure mode is what makes the successes credible.
- **E5 Before/after genome** (RQ3): the same task, same weights, gene off vs on.

## Appendix A — Reproduction procedure

The system was built so that reproduction is the SAME path we use ourselves — no
bespoke lab scripts. Everything below assumes one prerequisite, stated once:

> **With continuum installed** (one-line installer from the README; installs the core,
> the `cu` CLI, and manages its own `llama-server` binary), on a 64GB Apple-silicon
> Mac or equivalent.

**Out-of-repo dependencies (the complete list):**
- Model weights: pulled by HF repo id (`hf download <repo> --include "*Q4_K_M*"
  --local-dir ~/.continuum/genome/models/<short-name>`); the registry resolves GGUFs
  by id-token derivation — no config edits. Exact repo ids per model are in each
  ledger row's replication field.
- Training arms only (RQ3): a Python venv with `mlx_lm` (Apple silicon). The core
  drives it; you never invoke Python directly.
- Competitor arms: aider / opencode installed per their own docs, versions pinned in
  the results tables.
- Nothing else. No API keys for the local arms; no Docker for the OURS arm
  (SWE-bench's official grader uses its own Docker per its docs).

**Per-cell reproduction:** every ledger row carries its exact command. The general
shapes:

```bash
# OURS arm, any registered model:
cu benchmark/run '{"persona_id":"<any persona uuid>","name":"humaneval-rs",
  "limit":20,"base_model_id":"<model id>","max_acts":6,"detach":true}'
# result lands in ~/.continuum/progress/<persona>.jsonl; record it:
cu benchmark/record '{...model/harness/benchmark/resolved/total/replication/...}'
# render the comparison from all recorded rows:
cu benchmark/matrix
```

Runs are greedy-decoded and snapshot-isolated: same weights + same tasks → same
verdicts. The `benchmark/list` catalog names every runnable benchmark; `limit`
omitted runs the full set (the definitive-run configuration in §3).
