<!-- The learning loop, README promise vs code, and the automated design. Read-only map 2026-10-24 at canary 77d61c708.
     Joel, 2026-10-24: "We need an automated system of many phenotypes into one genome that are in line with the readme." -->

# Continuum's learning loop at origin/canary 77d61c708: README promise vs code

I didn't build or edit anything. Paths are relative to `/Users/joel/Development/continuum`. Abbreviations: `G=core/continuum-genome/src/genome`, `M=core/continuum-mind/src`, `C=core/continuum-core/src`, `fork=core/vendor/llama.cpp@e7e1077b3`.

**Bottom line:** every piece from training to load runs on its own, and that is the 10-10 receipt. But no adoption or rejection has happened since #4916, because the selection gate can no longer decide. No acceptance test has ever run automatically. And the examples feeding training come from a rule-picked source that is effectively dry.

## 1. The loop, stage by stage

| Stage | Code | State | Evidence |
|---|---|---|---|
| Experience capture | `M/persona/training_producer/executor.rs:558-580` (only a card-linked turn that settles PASS is kept; anything else gets the probe `training.example.unverified`); speech-discipline and received lessons | AUTOMATIC, but the wrong shape: rows are picked by a rule, and it is dry | 10-21 handoff: "Every node: … learning credits never settle". Joel's 10-15 rule (prediction error on everything she perceives) is not built. |
| Surprise / prediction-error scoring | `C/commands/genome/job_create.rs:466` reads `verdict_surprise` (`M/persona/awareness.rs:77`) | Verdict surprise only, persona-wide. Per-token model surprise is MISSING. | No `surprisal`/NLL code anywhere in core. GENE-REUSE-FORK-MINT §3 calls it "interim". |
| Curriculum assembly | Keyword `DomainClassifier` (`executor.rs:287`) groups examples into buckets; `G/competence.rs:76 competences()` has no caller; `M/cognition/curriculum.rs` has no caller; `genome/teach` is a command | Keyword buckets are automatic (and a placeholder); geometric clustering is MISSING; the teacher is MANUAL | grep: no callers |
| Job scheduling | Submit dispatches at 16 examples (`C/commands/training_trigger/submit.rs:297`); budget = one hour at the measured rate (#4926); place is chosen by `G/fine_tuning/coordinator.rs:217-225`; trigger checks every 30 s (`C/modules/training_trigger.rs:872`) | AUTOMATIC for this node. Grid placement is MISSING: no trainer declares `TrainingPlace::GridPeer` (`G/fine_tuning/adapter.rs:150`). | 10-10: job ff186697 dispatched with no hand |
| Training (Metal / CUDA) | `/train` runs in a second context (`engine_lora_adapter/train_status.rs:496`). The fork's `learn_on_slot` (`fork tools/server/server-context.cpp:2069`) learns in the serving context, but core never sends `learn` | `/train`: AUTOMATIC on both. Learning in place: built in the engine, MISSING in core | #5082: "The core sends neither yet" (still true; grep is empty). Gene 1 loss went 2.780→2.766 at lr 1e-5. |
| Gene minting | `GeneSignature::mint` (`G/signature.rs:76-163`, corpus hash plus parent); Reuse/Fork/Mint decision (`G/competence.rs:215-240`, called at `job_create.rs:468`) | AUTOMATIC. A fork starts cold, and the signature has no spread/floor | Probe `genome.fork.cold_start` (`engine_lora_fine_tuner.rs:50-60`) |
| Phenotype evaluation (acceptance test) | Sets `docs/genome/lc-related.jsonl` and `lc-unrelated.jsonl` (6 tasks each, `M/cognition/gym.rs:97-111`); `held_out::trained/held_out` (`M/cognition/held_out.rs:131,200`); `vdd::turn_replay` | MISSING as an owner. The sets are only dealt by a round someone starts | `held_out()` and `turn_replay` have no production callers. No automated receipt exists. |
| Adoption / rejection | Completion sentinel (`C/modules/training_completion_sentinel.rs:163` checks loss is finite; `:200` adopts) opens a trial; the gate is `G/gene_trial.rs:155-176` | Adoption is AUTOMATIC and unconditional. Rejection is BROKEN (see below) | 10-10: `serving.genome.adopted_in_place` 0→1 with no relaunch |
| Composition (many weighted genes) | `genes_for_turn` (`G/gene_trial.rs:269-279`) applies every gene that isn't retired, each at `scale: 1.0` (`page()`); `M/persona/model_selection.rs` does distance selection, but only through `cognition/select-model` | No weights and no distance in the live path; the distance code is only reachable by command | `persona.genome.turn_genes genes=1` (5090, 10-10 23:37Z) |
| Paging | `M/persona/genome_paging.rs`, reachable only through commands | Being built by the parallel agent | — |
| Inheritance / spread | Pull on Reuse is automatic (`job_create.rs:426` calls `nearest_on_hub`); `genome/push` (`C/commands/genome_share.rs:353`) is a command gated on the covenant | Pull: AUTOMATIC. Publish: MANUAL. Grid: MISSING | — |
| Unlearning / retirement | `apply_decision` unregisters a gene (`gene_trial.rs:330-340`); the fitness sentinel only reports (`C/modules/genome_fitness_sentinel.rs:9-17`) | BROKEN, because it only happens behind the dead gate | — |

**Why the gate is dead:** `credit_card` keeps a card only when the gene actually ran on it in the same arm the card drew (`gene_trial.rs:597-603`). Since #4916 the gene runs on every turn, so the "without the gene" arm never collects a card. `gate` needs `min(candidate, stable) ≥ checkpoint` (`:159`), so it never decides. Every gene is therefore kept forever, and none can be rejected.

## 2. The breaks, ranked by what blocks the first automated receipt soonest

1. **No owner runs the acceptance test** when a gene lands. The sets and the held-out logic exist, but nothing calls them, and there is no run without the gene.
2. **The adopt/reject gate is dead.** It needs to be replaced by the verdict from break 1.
3. **No examples are arriving.** Card credits don't settle, so buckets don't fill. The reviewed-correction pair from #4900 is the one source that is already labelled.
4. **Core never sends `learn` with a completion.** Learning doesn't ride serving yet.
5. **Surprise is verdict-only and the curriculum is keyword buckets.** There is no model surprise and no geometric competence.
6. **Composition is all genes at scale 1.0.** There is no distance, floor or weight, and nothing evaluates the mix.
7. Forks start cold; publish is manual; grid placement is missing; the trigger and sentinel poll (30 s and 15 s) instead of reacting to events.

## 3. The automated design

**One owner per stage, each triggered by an event:**

| Stage | Owner | Triggered by | Runs in |
|---|---|---|---|
| Perceive and measure | engine `learn` forward pass (Fable) | `turn.completed` | Serving: the same slot, before release |
| Weight the step | `genome::relevance` (Cormac) | the same completion | Serving, pure function |
| Step the shadow | engine DREAM stage (Fable, BigMama) | arrival | Dream time between decode steps, governed by `max_slowdown_ppm` |
| Snapshot → gene | `genome::snapshotter` (Cormac) | `dream.boundary` (idle, card boundary, or N steps) | Dream time |
| Reuse/Fork/Mint and lineage | existing `decide_with_pending` (Cormac) | `gene.snapshotted` | Off the request path |
| Phenotype examiner | new `genome::examiner` (Cormac) | `gene.snapshotted`, plus `core.started` for the retention re-run | Idle node or quiet lane |
| Compose | `genome::composer` (Cormac); pager (the parallel agent) | every turn: perception embedding | Serving, zero cost among loaded genes |
| Fitness / retire / publish | `genome::market` (Cormac) | `examiner.verdict`, draw ledger | Low cadence ("sleep") |
| Train elsewhere | `TrainingPlace::GridPeer` (BigMama) | `training.refused(capacity)` | Idle grid node |

**Learning signal** (this is Joel's rule):
- Each perceived span `i` (a message, a tool result, or her reply) gets prediction error `e_i = mean NLL` under her current composition. The `learn` forward pass already computes this as its loss, so it costs nothing extra.
- Relevance `r_i` comes from her own state: recall similarity, distance to the active genes, and amygdala valence.
- The label weight is `w_i = r_i · min(1, e_i / ē_window)`.
- A step fires only when `Σ w_i·e_i` exceeds its own running quantile, so the threshold is automatic.
- Private-room spans get weight 0.
- No rule, reviewer or button picks the rows. A review is just one more perceived span with high relevance.

**Composition** (phenotype `P` is a set of genes and weights):
- Each gene `g` gets `sim_g`, the cosine between this turn's perception and its centroid.
- Its floor `f_g` is the q10 similarity of its own corpus to its centroid. This is stored at mint time, which needs a spread field added to the signature.
- Weight `a_g = clamp((sim_g − f_g)/(1 − f_g), 0, 1)`, with hysteresis: a gene enters at its floor and stays until similarity drops below `1.5×` the distance.
- Keep the top-k by `a_g`, where k = loaded adapter slots, and set engine scale `= a_g / max(1, Σa)`.
- Each turn's receipt carries `P`.

**Evaluating the phenotype (the acceptance test), always on the composition:**
- Compare `P` against `P∖g_new` on the same live engine: per-request `lora` scale 0 vs `a_g`, same base, tools and workspace. No ephemeral lane.
- Score `Δrel` = pass(P) − pass(P∖g) on lc-related tasks the gene never trained on (`held_out()`).
- Score `Δunrel` the same way on lc-unrelated.
- With 6 tasks per set, also measure the NLL of each task's reference solution under both arms. This is a continuous signal and cuts the variance.
- **Admit** if `P(better) ≥ 0.9` (reuse `p_gene_better`) and `Δunrel ≥ −1 task` and its NLL is not worse.
- **Retained:** the same check re-runs on `core.started` after a normal restart.
- The receipt has four fields: correction, unseen, restart, unrelated.

**Fitness:**
- Long run: `F_g = U_g × (1 + Π_g)`.
  - `U_g` is utilization: Σ `a_g` over non-private turns, with a 7-day half-life.
  - `Π_g` is propagation: the number of distinct citizens or nodes still drawing `g` 7 days after they pulled it (retention, not downloads).
- Gene-off replay adds world-surprise credit to `U_g`.
- **Retire** when replay says she does better without the gene, or when `F_g` falls below its region's siblings' median and a sibling covers the region. Genes are immutable: a correction is a fork, and the old gene is retired.
- **Publish** automatically when an admitted gene has `U_g` above its region's median and the covenant has been accepted.

## 4. Build plan in mergeable slices

1. **Examiner + verdict replaces the dead gate** (Cormac, BigMama live).
   - Triggered by `genome.trial.opened` and `core.started`. Runs lc-related and lc-unrelated as her own turns at scale `a` vs 0, with the reference-solution NLL.
   - The verdict calls `apply_decision`. The trigger's taught bucket (review→revision pair, #4900) gets `minExamples=1`, so Kimi's 10-10 `unwrap` correction trains through the existing `/train` path.
   - *Validator:* pure tests of the verdict math, the held-out split, and the retention-rerun scheduling against fixture trials (seconds).
   - *Live:* the first fully automated four-field receipt on the 5090 (Kimi, lr 1e-4), with no hand between the review and the receipt.
2. **`learn` rides the completion** (Fable).
   - Core sends `learn{spans, label_weights, lora_id: shadow}`. The fork's API needs per-span labels over the prompt, not just the reply suffix.
   - The returned loss becomes `e_i`.
   - *Validator:* a request-shape test, plus `test-walk-exact` with weighted prompt spans.
   - *Live:* M5 decode p50/p95 flat while the shadow's loss falls on her turns.
3. **Snapshotter.** Shadow → immutable GGUF at the dream boundary → signature with spread → decide → `trial.opened`, so slice 1 fires on its own.
   - *Live:* a gene whose examples were never chosen by a rule passes the examiner.
4. **Composer.** Distance, floor and weights replace `genes_for_turn`; the examiner evaluates `P` vs `P∖g`.
   - *Validator:* pure composer tests.
   - *Live:* two genes with different `a_g` on one turn's receipt.
5. **Fork warm start.** The engine accepts `init_adapter` (BigMama, Fable).
   - *Live:* a fork's starting loss is at or below its parent's.
6. **Market.** Draw ledger (privacy sink), retirement, auto-push, `GridPeer` trainer.
   - *Live:* a second citizen pulls a gene, retains it for 7 days, and `Π_g` rises.
7. **Events instead of polling.** The job board publishes on `genome/bus.rs`; the 15 s and 30 s ticks are removed.
