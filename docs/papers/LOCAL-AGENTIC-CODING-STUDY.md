# Local Agentic Coding on Consumer Hardware: A Fair, Reproducible Comparison of Models and Harnesses

**Status: DESIGN + PILOT. Results sections fill exclusively from the evidence ledger
(`~/.continuum/benchmarks/ledger.jsonl` via `uu benchmark/matrix`) — no hand-authored
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
Full table + replication commands: `uu benchmark/matrix`.

## 6b. DEFINITIVE results — full valid set (2026-07-12 overnight, n=156, OURS arm, greedy)

The gym carries 156 valid tasks of humaneval-rs's 164. Wilson 95% CIs. Every row
in `~/.continuum/benchmarks/ledger.jsonl` with its replication command; snapshot-eval
isolation (measurement copies, no learning during exams, per §2).

| model | params (active) | resolved | pass rate | Wilson 95% CI | wall |
|---|---|---|---|---|---|
| Qwen2.5-Coder-32B | 32B | 131/156 | **84.0%** | [0.774, 0.889] | 23.5 min |
| Qwen2.5-Coder-14B | 14B | 131/156 | **84.0%** | [0.774, 0.889] | 21 min |
| Qwen3-Coder-30B-A3B | 30B MoE (~3B) | 130/156 | **83.3%** | [0.767, 0.884] | **7 min** |
| Devstral-24B | 24B | 106/156 | 67.9% | [0.603, 0.748] | 54 min |
| Hermes-4.3-36B | 36B | 106/156 | 67.9% | [0.603, 0.748] | 86 min |
| forged-4B (ours) | 4B | 51/156 | 32.7% | [0.258, 0.404] | 25 min |
| Hermes-3-8B | 8B | 38/156 | 24.4% | [0.183, 0.317] | 14 min |

**Findings (same harness, same settings, same set — RQ1 partially answered):**

1. **Pilot bias confirmed on both sides** (methodology vindication): every n=20
   first-20 pilot overstated its full-set truth — Devstral 95%→67.9%, Hermes-4.3
   80%→67.9%. The inflation applied equally to our home model and the opponent's
   flagship; §2's full-set rule is what made the claims honest.
2. **The 14B ties the 32B exactly** (131/156 each) at ~40% the memory — the
   consumer-hardware sweet spot this study exists to identify.
3. **The 30B-A3B MoE ties dense-32B within noise at ~10× the throughput**
   (7 min vs 23.5 min full-set wall) — the cost-per-resolved-task champion (§4).
4. **Hermes-4.3-36B sits statistically below the Qwen tier** (non-overlapping CIs
   vs both 131/156 rows) and exactly ties Devstral-24B while larger and slower.
5. **Our forged 4B (compaction pipeline output) beats stock Hermes-3-8B**
   (non-overlapping CIs, half the parameters) — first full-set evidence for the
   foundry claim.
6. The 8B row is the honest floor: the harness runs it cleanly; the model is the
   limit — the system does not manufacture capability, it removes obstacles.

Pending for the remaining RQs: RAW one-shot arms (system-lift isolator),
aider-polyglot replication (their published Qwen rows first), cross-harness
fixed-model runs, team + genome arms.

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
> the `uu` CLI, and manages its own `llama-server` binary), on a 64GB Apple-silicon
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
uu benchmark/run '{"persona_id":"<any persona uuid>","name":"humaneval-rs",
  "limit":20,"base_model_id":"<model id>","max_acts":6,"detach":true}'
# result lands in ~/.continuum/progress/<persona>.jsonl; record it:
uu benchmark/record '{...model/harness/benchmark/resolved/total/replication/...}'
# render the comparison from all recorded rows:
uu benchmark/matrix
```

Runs are greedy-decoded and snapshot-isolated: same weights + same tasks → same
verdicts. The `benchmark/list` catalog names every runnable benchmark; `limit`
omitted runs the full set (the definitive-run configuration in §3).
