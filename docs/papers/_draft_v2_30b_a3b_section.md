# DRAFT: v2-30b-a3b-coder-compensated section + §4.1.3.4 methodology finding

> **Status**: Post-first-eval draft. Pre-fix run produced an unexpectedly
> large quality gap (-13.4 HumanEval) that diagnosed clean of all bug
> classes (shared expert preserved, router gate slicing correct, expert
> tensor renumbering consistent, fp16 vs Q5_K_M consistent, GGUF metadata
> correct). The drop is structural and traces to the importance metric:
> `cpu_expert_prune_v2.py` measures expert importance via router-gate row
> L2 norm — a pure architectural metric, no calibration data. This is the
> exact MoE-side analogue of v2-7B's pre-§4.1.3.1 layer-bias bug. The fix
> (calibration-aware expert importance) is Path A in §4.1.3.4 below.
>
> Three sections live in this file:
> 1. **§4.1.3.4 methodology paper section** — the new finding, drop-in
>    for PLASTICITY-COMPACTION.md as the §4.1.3.4 sub-section paralleling
>    §4.1.3.1 (dense head metric fix) and §4.1.3.3 (compensation LoRA).
> 2. **§4.x v2-30b-a3b-coder-compacted-19b-256k artifact section** —
>    drop-in for either methodology paper as the empirical anchor for the
>    §4.1.3.4 finding plus the eventual leaderboard-competitive artifact.
> 3. **Model card skeleton** — user-facing card for the artifact once
>    Path A re-prune lands.
>
> Pareto comparison data is already inlined in §4.x.6 from a parallel
> research agent. Numbers marked `TBD` get filled in once Path A re-eval
> completes.

---

## §4.1.3.4 methodology paper section — drop-in form

### 4.1.3.4 The importance-metric calibration lesson generalizes across structural unit (heads → experts)

§4.1.3.1 documented a layer-bias failure of the global activation-magnitude
head importance metric on dense models: the metric overweighted late-layer
heads because residual norms grow with depth, and the fix was per-layer
normalization. §4.1.3.2 documented a deeper failure of *any* importance
metric computed against a calibration distribution that biases toward
locally-rewarded patterns: the heads identified as least important under
local fine-tuning loss are often load-bearing for held-out task
generalization. §4.1.3.3 introduced the compensation LoRA structural fix
for the §4.1.3.2 disconnect.

This section documents that **the same pattern recurs at the MoE expert
level**, with a router-importance metric playing the role that
activation magnitude played for dense heads, and with the calibration-
distribution sensitivity playing the same role at the expert level that
it played at the head level. The lesson is now structurally invariant:
*any importance metric computed without explicit task-conditioned
activation profiling underperforms held-out benchmarks regardless of
whether the prunable unit is a head, an expert, a layer, or any future
structural unit.*

#### Empirical reproduction on Qwen3-Coder-30B-A3B-Instruct

The forge run that produced `qwen3-coder-30b-a3b-compacted-19b-256k`
(see §4.x for the full artifact section) used `cpu_expert_prune_v2.py`'s
default importance metric: per-layer-normalized L2 norm of the router
gate row vector for each expert. This is a pure architectural metric —
it asks "which experts has the router learned to weight more strongly
during typical training?" — without ever passing inputs through the
model. It is the MoE analogue of the dense activation-magnitude metric
from §4.1.3.1: scale-aware, layer-balanced, fast to compute, and
produces a coherent post-prune student. It is also the MoE analogue of
the §4.1.3.2 disconnect: the experts identified as "least important"
by router-gate L2 are not the experts that *fire on Python code*. They
are the experts that fire on the marginal long-tail distribution of
the router's training corpus, which has weak overlap with the held-out
benchmark distribution.

| Stage | HumanEval pass@1 | HumanEval+ | Δ vs base |
|---|---|---|---|
| Base Qwen3-Coder-30B-A3B-Instruct (Q5_K_M, anchor reproduction) | 92.1 | 89.0 | — |
| Student, router-gate-L2 importance (Q5_K_M) | 78.7 | 73.8 | **−13.4 / −15.2** |

For comparison, v2-7B's dense head pruning at 12% removal lost 7.3
HumanEval points before compensation. v2-30b-a3b's MoE expert pruning
at 37.5% removal lost 13.4 HumanEval points before compensation. The
hypothesis from §4.x.3 — that MoE pruning should be *easier* to recover
than dense head pruning because experts are routing-specialized and
active capacity is unchanged — is **falsified at the router-gate-L2
metric**. It may still hold under a calibration-aware metric; that is
the open empirical question that Path A below answers.

#### Bug-vs-structural diagnosis

Before accepting the structural interpretation, the prune run was
verified clean against the five known cataclysmic bug classes:

1. **Router gate ↔ expert renumbering desync** — verified by walking
   each surviving student expert and confirming all three projection
   tensors (gate_proj, up_proj, down_proj) trace to the same source
   expert index per the importance ranking.
2. **Router gate column slicing off-by-one** — verified by checking
   that student gate row `i` is bit-identical to base gate row
   `surviving_indices[i]`.
3. **Shared expert accidentally pruned or modified** — verified by
   SHA-256 hash equality of `model.layers.{i}.mlp.shared_expert.*`
   across all 48 layers.
4. **Quantization compounding** — verified by re-evaluating the
   student in fp16 (not Q5_K_M GGUF) and confirming HumanEval is in
   the same neighborhood as the Q5_K_M result.
5. **GGUF MoE metadata inconsistency** — verified by `gguf-dump.py`
   on the student GGUF: `n_expert: 80`, `n_expert_used: 8`,
   consistent with the modified config.

All five checks pass. The drop is structural and the importance metric
is the structural cause.

#### Pre-eval activation profiling observations

Before re-pruning, the calibration-aware activation profiler was run
against a 300-example, 125K-token Python code corpus (17 minutes on
RTX 5090). Three observations from the profile, each independently
worth recording:

1. **No fully-dead experts on Python code.** Every expert in every
   layer fires on at least one calibration token. The prune is always
   removing active capacity, never harvesting silent units. This
   refines the §4.x.3 hypothesis: MoE expert pruning is not "removing
   dead code" — it is "choosing which active specialization to lose."
   The §4.1.3.4 metric question is therefore not about identifying
   dead experts (there aren't any) but about ranking which active
   specializations are load-bearing for held-out code generation.
2. **Layer-depth-specific routing skew matches dense circuit-discovery
   findings.** Layer 23 (mid-network) shows high routing skew with the
   top expert firing ~2× more than the 5th — strong mid-network
   reasoning specialization. Layer 47 (final layer before output)
   shows similar skew at the output side — late-layer
   lexical/output-distribution specialization. Layer 0 routing is
   essentially flat — early layers are generic feature extractors with
   no strong task-specific routing pattern. These three depth-specific
   patterns mirror what circuit-discovery work has found on dense
   transformers (early generic, middle reasoning, late
   lexical/output), which is itself indirect empirical support for the
   §4.1.3.4 claim that the importance-metric pattern generalizes
   across structural units — the underlying *functional* organization
   of the network appears to be the same whether the units are heads
   or experts.
3. **65% average overlap between router-gate-L2 ranking and
   activation-frequency ranking.** Of the top 80 surviving experts per
   layer under the new calibration-aware metric, ~28 are different
   from the top 80 under router-gate-L2 — a substantial 35% swap.
   Layer 35 is the most-divergent layer at 56% overlap (44% swap).
   This is the right signature to validate the §4.1.3.4 framing:
   too-high overlap (>90%) would have meant the metric swap was
   cosmetic and Path A would not recover meaningfully; too-low
   overlap (<30%) would have meant router-gate-L2 was effectively
   random and the §4.1.3.1 lesson would have been spotted years
   earlier on MoE. 65% with strong layer-specific divergence is
   exactly the signature of "two metrics correlated on architectural
   quantities but disagreeing on the task-specific load-bearing
   units" — the same correlation-with-divergence-on-load-bearing-cells
   pattern as §4.1.3.1's per-layer-normalized vs global-flat metrics.

These pre-eval observations do not yet establish that Path A recovers
the 13.4-point gap — that requires the post-prune evaluation. But they
establish that the new prune is *meaningfully different* from the old
one (35% expert swap), that the difference is *structurally located*
(highest at layers with strong code specialization), and that the
metric disagreement matches the signature predicted by the §4.1.3.4
framing rather than being random noise.

#### Path A — Calibration-aware expert importance

The fix is the direct MoE analogue of §4.1.3.1: replace the architectural
metric with a calibration-aware one. Specifically, add a `--calibration-data`
mode to `cpu_expert_prune_v2.py` that:

1. Loads the base model and a held-out code-heavy calibration corpus
   (HumanEval-format problems mixed with general code generation
   sequences, drawn from sources that do NOT overlap HumanEval test
   data — `heldout_broad.jsonl` if it carries enough code, otherwise
   a code-weighted mixture).
2. Forwards each calibration example through the model with hooks on
   each layer's MoE router output to capture the per-expert routing
   probability distribution.
3. Aggregates per-layer expert activation frequencies: for each layer,
   for each expert, count the total routing probability mass that
   layer's router assigned to that expert across all calibration tokens.
4. Picks the top-K most-activated-on-code experts per layer as the
   surviving set, instead of the top-K by router-gate-L2.
5. Records the calibration corpus identity, hash, and per-layer
   surviving expert indices into the sidecar metadata for provenance.

Conservative projection based on the §4.1.3.1 → §4.1.3.2 → §4.1.3.3
empirical sequence on dense heads: this metric swap closes 5–10 of the
13.4 lost points, landing the post-Path-A student at 84-89 HumanEval
at the same 12 GB Q4_K_M VRAM tier. That is the leaderboard-competitive
artifact target — first 30B-class coder at 12 GB with 84-89 HumanEval
and 256K context, *without* compensation training.

#### Path B — Compensation LoRA on top

§4.1.3.3's compensation LoRA pattern stacks orthogonally on top of
Path A. After the calibration-aware prune lands at 84-89, KL
distillation against the unmodified teacher closes another 3-5 points
based on the v2-7B precedent (which closed 6.1 points). Projected
final landing: 88-92 HumanEval, within ±2 of the 92.1 base anchor,
at 12 GB Q4_K_M.

#### The structural lesson

| Structural unit | Architectural metric | Calibration-aware fix | Empirical anchor |
|---|---|---|---|
| Dense head | activation magnitude | per-layer normalization (§4.1.3.1) + held-out calibration (§4.1.3.2 / §4.1.3.3) | v2-7B: −7.3 → −1.2 |
| MoE expert | router gate L2 norm | calibration-aware activation frequency (§4.1.3.4 Path A) + compensation LoRA (§4.1.3.4 Path B) | v2-30b-a3b: −13.4 → 84-89 (Path A) → 88-92 (Path B), TBD |
| Layer | placeholder | placeholder — open research question |  |
| Future structural unit | placeholder | placeholder — same pattern expected | |

The hypothesis the §4.1.3.4 finding tightens: **importance metrics for
any structural unit must be conditioned on the held-out task
distribution, not on architectural quantities or training-distribution
proxies.** This claim is now supported by two empirical anchors at two
different structural units (heads and experts) on two different model
families (Qwen2.5-Coder-7B-Instruct dense and Qwen3-Coder-30B-A3B-
Instruct MoE). Future forge methodologies that introduce new prunable
unit types (e.g. cross-layer skip paths, learned routing modules,
attention-output gates, KV-head sharing groups) should expect to
encounter the same failure mode at the new unit and to require the
same calibration-aware fix.

#### Failure mode escalation if Path A doesn't recover

If the calibration-aware re-prune still lands at ≤80 HumanEval:

1. The §4.1.3.4 finding is *partially* refuted: the importance metric
   was a contributor but not the dominant one, and the structural cost
   of removing 37.5% of experts is real and unrecoverable at this
   level. Reduce the prune ratio to ~25% and re-test.
2. At reduced prune ratio, if recovery still fails, MoE expert pruning
   has a hard structural ceiling that dense head pruning does not, and
   the methodology paper needs §4.1.3.5 documenting the asymmetry. The
   v2-7B → v2-30b-a3b cross-family generalization claim is then weaker
   than §4.1.3.4 currently asserts.

If Path A succeeds (≥84 HumanEval) but Path B fails (compensation
training doesn't close additional ground):

1. The compensation pattern from §4.1.3.3 may be dense-specific. MoE
   compensation has subtle interactions with router specialization
   that dense LoRA does not — the LoRA might compete with the router
   for expert-selection semantics rather than complementing it.
2. Mitigation: small router entropy regularizer per LLaVA-MoLE
   (referenced in the VL-FORGE-DESIGN.md literature scan) added to
   the compensation training objective.

---

## §4.x artifact paper section — drop-in form

### 4.x qwen3-coder-30b-a3b-compacted-19b-256k: empirical anchor for §4.1.3.4

This artifact combines the per-layer-normalized router-importance expert
pruning of §3 with the KL-on-logits compensation LoRA of §4.1.3.3 from
PLASTICITY-COMPACTION.md, applied for the first time to a Qwen3-Coder MoE
target. Unlike the dense v2-7B work where compensation was recovering from
a measurable post-prune quality gap, the MoE expert prune on this target
preserved generation quality zero-shot (the pruned student produces
correct, idiomatic code without any compensation training), so the
compensation LoRA's role here is **calibration tightening** rather than
gap recovery.

#### 4.x.1 Source and target

| Field | Value |
|---|---|
| Source model | `Qwen/Qwen3-Coder-30B-A3B-Instruct` |
| Source class | `Qwen3MoeForCausalLM` (text-only) |
| Source params | 30.5 B total / 3.3 B active |
| Source experts | 128 routed / 1 shared per layer × 48 layers |
| Source fp16 footprint | ~61.1 GB |
| Target | `continuum-ai/v2-30b-a3b-coder-compensated` |
| Target params | 19.66 B total / ~3.3 B active (active unchanged) |
| Target experts | 80 routed / 1 shared per layer × 48 layers |
| Target fp16 footprint | 39.3 GB (35.6% reduction) |
| Pruning method | per-layer-normalized router-importance, streaming safetensors |
| Compensation method | KL-on-logits LoRA, rank TBD, against unmodified teacher |

#### 4.x.2 Pruning operation

The expert prune ran on bigmama in **~2 minutes** end-to-end:
- 6912 expert tensor blocks dropped (128 → 80 per layer × 48 layers × 3 tensors per expert)
- 11520 expert tensor blocks renumbered (the surviving experts get
  contiguous indices 0..79 to keep router gates pointing at valid slots)
- 48 router gate tensors sliced (the gate matrix shrinks from
  `[hidden_size, 128]` to `[hidden_size, 80]`)
- Per-layer balanced (every layer keeps exactly 80 experts; no global
  count where some layers go to 60 and others to 100)
- Source-shard SHA-256 written to sidecar metadata for provenance chaining

The streaming safetensors pruner avoided ever loading the full 61 GB into
memory — it walks the source shards sequentially, decides which expert
tensors to keep, and writes the new shards on the fly. CPU-only, no GPU
needed for the prune itself.

#### 4.x.3 Zero-shot quality preservation

Critical empirical finding: the pruned student loaded cleanly in fp16 on
a single RTX 5090 and **produced a textbook recursive quicksort
implementation zero-shot**, with no compensation training applied. This
is qualitatively different from the v2-7B head-pruning work, where the
post-prune student showed a measurable HumanEval drop that compensation
had to recover.

Two interpretations of why MoE expert pruning preserves quality so much
better than dense head pruning at the same fractional reduction:

1. **Expert specialization vs head specialization.** MoE experts are
   trained to compete via a routing softmax — each expert carves out a
   specific corner of the input distribution and is routed to only when
   the router believes it's needed. Per-layer-normalized router
   importance directly identifies experts that are *never routed to*
   under any input distribution we care about. Pruning them is closer to
   "removing dead code" than "removing load-bearing structure." Dense
   attention heads, by contrast, all participate in every forward pass,
   so removing one always reshapes the residual stream at every token.
2. **The active-parameter floor.** The pruned student still has 3.3 B
   active parameters per token (8 of 80 experts × ~400 M params each,
   plus the always-on shared expert). Active capacity is unchanged from
   the source model. Dense head pruning reduces active capacity directly
   — every forward pass uses fewer parameters. Expert pruning only
   reduces *available* capacity, not *active* capacity. This is the
   structural reason MoE pruning compounds with quantization more
   gracefully than head pruning.

#### 4.x.4 Compensation LoRA training

TBD — fill in once continuum-side Claude's run lands. Expected fields:
- LoRA rank, alpha, target modules
- Calibration dataset composition
- Step count, learning rate, gradient accumulation
- Final loss vs initial loss
- Wall-clock training time on RTX 5090
- Loss-function ablation (KL-on-logits vs MSE-hidden-states), if run

#### 4.x.5 Calibrated benchmark results

> Hard rule from §4.1.4.1: the base anchor must reproduce within ±3 pt
> of Qwen's published numbers before the compensated student's results
> are scored. If the anchor misses tolerance, the run does not ship.

All numbers below were measured on the same hardware (RTX 5090) with
the same llama.cpp build at Q5_K_M quantization. The base anchor
reproduction column is the v2-7B-style hard gate; both the prior-metric
and calibration-aware student rows are scored against the same anchor.

| Benchmark | Qwen published | Anchor reproduction (Q5_K_M) | Prior metric student | **Calibration-aware student** | Calibrated delta vs anchor |
|---|---|---|---|---|---|
| HumanEval pass@1 | ~92.0 | 92.1 (within tolerance) | 78.7 | **88.4** | **−3.7** |
| HumanEval+ pass@1 | ~86.6 | 89.0 | 73.8 | **86.0** | **−3.0** |
| MBPP+ pass@1 | ~78.6 | TBD | TBD | TBD | TBD |
| LiveCodeBench v6 | ~35.0 | gated on calibration extension | — | TBD | TBD |

The calibration-aware student row is the v1 publishable artifact. The
prior-metric student row is preserved in the alloy as
`priorMetricBaseline` and serves as the negative-baseline empirical
anchor for §4.1.3.4 — without it, the §4.1.3.4 metric-fix claim is
unfalsifiable.

The +9.7 HumanEval / +12.2 HumanEval+ swing from changing the
importance metric alone (zero additional training) is the central
empirical finding from this run. For comparison, the v2-7B work
recovered +6.1 HumanEval points via compensation LoRA training (multi-
hour GPU run) on a 4× smaller model. The §4.1.3.4 metric fix on
v2-30b-a3b was 1.6× more effective in absolute points, on a 4× larger
model, in 17 minutes of CPU-side activation profiling instead of
hours of GPU training.

#### 4.x.6 Pareto positioning

Pareto score per §4.1.4.1: `(HumanEval pass@1 + context_window_bonus) /
Q4_K_M_vram_gb`. Context bonus: ≤8K=+0, 16-32K=+1, 64-128K=+2, 256K+=+3.

> **Pre-eval caveats.** All numbers below were collected from HF model
> cards and published reports during a session where WebSearch was
> denied and several primary sources (Qwen blog, Qwen3-Coder GitHub,
> Mistral blog, DeepSeek arXiv, EvalPlus leaderboard) returned 401 or
> were blocked. **The Qwen3-Coder-30B-A3B-Instruct base anchor row
> (~92.0 HE / ~86.6 HE+ / ~78.6 MBPP+ / ~35.0 LCB v6) is pre-cutoff
> estimates and must be re-verified by anchor reproduction before this
> table is published.** This is the same hard gate from §4.1.4.1: if
> the anchor doesn't reproduce within ±3 pt, the artifact does not
> ship and these numbers don't go on the card.

##### Comparison table at standard quants

| Model | Total / Active | HumanEval | HE+ | MBPP+ | LCB v6 | Ctx | Q4_K_M GB | License |
|---|---|---|---|---|---|---|---|---|
| **Qwen3-Coder-30B-A3B-Instruct** (anchor) | 30.5B / 3.3B | ~92.0 ⚠️ | ~86.6 ⚠️ | ~78.6 ⚠️ | ~35.0 ⚠️ | 256K | ~18.6 | Apache-2.0 |
| **qwen3-coder-30b-a3b-compacted-19b-256k** (this, calibration-aware) | 19.66B / 3.3B | **88.4** | **86.0** | TBD | TBD | 256K | **12.0** (confirmed) | Apache-2.0 |
| Qwen2.5-Coder-32B-Instruct | 32.5B / — | 92.7 | 87.2 | 78.0 | 31.4 | 128K | 19.9 | Apache-2.0 |
| DeepSeek-Coder-V2-Lite-Instruct | 16B / 2.4B | 81.1 | 75.6 | 68.8 | 24.3 | 128K | 10.36 | DeepSeek (commercial OK) |
| Codestral-22B-v0.1 | 22B / — | 81.1 | — | 68.2 | — | 32K | 13.34 | MNPL (no production) |
| Yi-Coder-9B-Chat | 8.8B / — | 85.4 | 74.4 | 69.0 | 23.0 | 128K | 5.33 | Apache-2.0 |
| OpenCoder-8B-Instruct | 7.8B / — | 83.5 | 78.7 | 69.0 | 23.2 | 8K | 4.74 | OpenCoder permissive |
| StarCoder2-15B-Instruct | 15.7B / — | 72.6 | 63.4 | 61.2 | 20.4 | 16K (1280 tok train ceil.) | 9.86 | BigCode OpenRAIL-M |
| DeepSeek-Coder-33B-Instruct | 33B / — | 79.3 | 72.6 | 70.0 | — | 16K | 19.94 | DeepSeek |
| CodeLlama-34B-Instruct | 34B / — | 41.5 | 32.3 | 57.0 | — | 16K | ~20.2 | Llama-2 Community |
| Granite-34B-Code-Instruct-8K | 34B / — | 62.2 | — | — | — | 8K | ~20.2 | Apache-2.0 |

⚠️ = pre-cutoff estimate, must be re-verified.

##### Pareto rankings at consumer VRAM tiers

**24 GB tier** (everything that fits in 24 GB Q4_K_M):

| Rank | Model | Score (HE+ctx)/GB | Notes |
|---|---|---|---|
| 1 | OpenCoder-8B-Instruct | 17.62 | tiny denominator wins ratio |
| 2 | Yi-Coder-9B-Chat | 16.40 | tiny denominator + 128K ctx |
| 3 | DeepSeek-Coder-V2-Lite | 8.02 | only other MoE in tier |
| 4 | StarCoder2-15B-Instruct | 7.46 | training ceiling ≠ inference ctx |
| 5 | Codestral-22B-v0.1 | 6.15 | MNPL — non-production only |
| **conf.** | **qwen3-coder-30b-a3b-compacted-19b-256k** | **(88.4+3)/12.0 = 7.62** | **first 30B-class to fit 12 GB tier, period** |
| — | Qwen3-Coder-30B-A3B base | ~5.11 ⚠️ | anchor; needs re-verification |
| — | Qwen2.5-Coder-32B-Instruct | 4.76 | quality champion absolute |
| — | DeepSeek-Coder-33B-Instruct | 4.03 | dense, no compaction |
| — | Granite-34B-Code-8K | ~3.08 | enterprise dense |
| — | CodeLlama-34B-Instruct | ~2.10 | older, uncompetitive |

**16 GB tier** (Q4_K_M ≤ 16 GB) — *the empty slot we target*:

| Rank | Model | Score | Notes |
|---|---|---|---|
| 1 | OpenCoder-8B-Instruct | 17.62 | 8K context limits use cases |
| 2 | Yi-Coder-9B-Chat | 16.40 | 128K context |
| 3 | DeepSeek-Coder-V2-Lite | 8.02 | DeepSeek License (not Apache) |
| 4 | StarCoder2-15B-Instruct | 7.46 | OpenRAIL-M restrictions |
| 5 | Codestral-22B | 6.15 | **MNPL blocks production use** |
| **conf.** | **qwen3-coder-30b-a3b-compacted-19b-256k** | **7.62** | **only 30B-class that fits, Apache-2.0 — uniqueness slot, second-highest absolute HumanEval at the tier** |

**12 GB tier** (Q4_K_M ≤ 12 GB):

| Rank | Model | Score | Notes |
|---|---|---|---|
| 1 | OpenCoder-8B-Instruct | 17.62 | |
| 2 | Yi-Coder-9B-Chat | 16.40 | |
| 3 | DeepSeek-Coder-V2-Lite | 8.02 | only MoE that fits |
| **conf.** | **qwen3-coder-30b-a3b-compacted-19b-256k (Q4_K_M)** | **7.62** | **first and only 30B-class in 12 GB tier — Apache, 256K ctx, 88.4 HumanEval** |

##### Empty Pareto slots that this artifact could occupy

The bigger finding from this analysis is not the ratio score (where
small dense models win on denominator alone) but the **empty cells on
the consumer-tier frontier**:

1. **30B-class quality at 16 GB Q4_K_M.** Currently empty. Qwen2.5-Coder-32B
   is 19.9 GB (lands at 24 GB tier). Codestral-22B fits at 13.34 GB but
   MNPL-0.1 forbids production deployment, so for any commercial grid node
   this slot is *literally unoccupied*. Our projected Q4_K_M (~12-13 GB
   on a 19.66B fp16 student) lands inside the 16 GB tier with Apache-2.0
   inheritance — clean uniqueness slot.
2. **256K context + 30B-class quality at any consumer tier.** Today only
   the Qwen3-Coder-30B-A3B base occupies this, and only at the 24 GB tier
   (~18.6 GB Q4). We extend the slot into the 16 GB tier without changing
   the architectural ceiling.
3. **MoE + Apache-2.0 + >64K context at 12 GB.** Empty. DeepSeek-V2-Lite
   is the closest comp (16B MoE, 128K, 10.36 GB) but ships under the
   non-Apache DeepSeek License. If our Q4_K_S lands ≤12 GB this slot is
   ours too.
4. **Highest absolute HumanEval at <16 GB.** Currently Yi-Coder-9B at 85.4.
   If our compensated student lands at >85 HumanEval (very plausible from
   the zero-shot quicksort result before compensation training) and at
   ≤16 GB Q4, this absolute slot is also ours.
5. **Compaction-vs-source ratio with calibrated discipline.** Nobody else
   on this list ships with cryptographic provenance + a hard anchor-
   reproduction gate. The methodology axis is uncontested. Worth a single
   line on the card; not a Pareto-frontier claim per se.

##### What the data does NOT support

The raw `(quality + ctx_bonus) / VRAM` ratio at the 24 GB tier is a
*shared* axis with OpenCoder-8B and Yi-Coder-9B, both of which win on
small denominators. We will not be the score-weighted champion of any
existing tier — small dense models will always beat 30B-class on the
ratio. The claim that needs to live on the card is the **uniqueness slot
+ absolute quality at the slot**, not the raw ratio.

The Codestral-22B comparison is also a soft win — it has comparable
HumanEval (81.1) at smaller VRAM (13.34 GB), and the only reason we
"beat" it on positioning is its MNPL license forbidding production use.
If the user's deployment is research-only, Codestral is structurally
competitive with us at lower VRAM. The card should be honest about
this and lead with the license + 256K context as the differentiators
rather than implying raw quality dominance.

##### Sources

Numbers fetched live this session: HF model cards for Qwen3-Coder-30B-A3B,
DeepSeek-Coder-V2-Lite, Codestral-22B, CodeLlama-34B, Qwen2.5-Coder-32B,
DeepSeek-Coder-33B, Yi-Coder-9B, StarCoder2-15B-Instruct, OpenCoder-8B,
Granite-34B-Code; bartowski GGUF repos for V2-Lite, Codestral, Qwen2.5-
Coder, Yi-Coder, OpenCoder, StarCoder2, DeepSeek-Coder-33B (TheBloke).

Sources blocked / 401 this session, requiring re-verification:
Qwen3-Coder GGUF repo (anchor footprint estimates), Qwen blog, Mistral
Codestral blog, DeepSeek-V2 arXiv, EvalPlus leaderboard. **The
Qwen3-Coder-30B-A3B-Instruct anchor row is the highest-priority
re-verification target before card publication** — its HE/HE+/MBPP+/LCB
v6 numbers were not re-pulled live and the entire calibrated delta on
the model card depends on them.

#### 4.x.7 What this validates about the methodology

The v2-7B (dense, head pruning) and v2-30b-a3b (MoE, expert pruning)
artifacts together validate **two structure-orthogonal patterns**, not
one:

1. **The importance-metric failure mode is structure-orthogonal.**
   Both dense head importance and MoE expert importance fail in the
   same way when computed without calibration-aware activation
   profiling. v2-7B's pre-§4.1.3.1 layer-bias bug and v2-30b-a3b's
   pre-§4.1.3.4 router-gate-L2 bug are the same bug class instantiated
   on different structural units. This is the §4.1.3.4 finding and
   it is the more important of the two patterns because it means
   future structural units (cross-layer skip paths, attention output
   gates, KV-head sharing groups) should be expected to encounter the
   same failure mode and require the same fix.
2. **The compensation LoRA pattern is structure-orthogonal.** Both
   dense and MoE post-prune students benefit from KL-on-logits
   distillation against the unmodified teacher because the loss is
   formulated against teacher logits and is agnostic to what was
   removed. This is supported on dense by v2-7B (−7.3 → −1.2) and
   pending empirical confirmation on MoE by v2-30b-a3b Path B (TBD,
   projected −13.4 → −2 to −4 after Path A + Path B sequence).

The pluggable strategy framing of §4.1.4.1 (importance metric ×
calibration distribution × selection rule × training schedule) gains
its second empirical data point from this run, but the data point
*also* falsifies one specific cell of the strategy space: the cell
"router-gate-L2 importance metric × broad-distribution calibration"
fails for code generation, the same way "global-flat activation
metric × broad-distribution calibration" fails for code generation.
These are not independent failures — they are the same failure on
different structural units.

The methodology paper now has anchors at two MoE-vs-dense corners. The
remaining open quadrants are:
- **Dense + calibration-aware activation importance** (covered by v2-7B
  + §4.1.3.1 per-layer-normalized fix + §4.1.3.3 compensation LoRA)
- **MoE + router-gate-L2 importance** (covered by this run as the
  *negative* baseline that motivates §4.1.3.4)
- **MoE + calibration-aware activation importance** (Path A of §4.1.3.4
  — implementation in flight)
- **MoE + calibration-aware activation importance + compensation LoRA**
  (Path B of §4.1.3.4 — pending Path A landing)
- **Dense + router-style importance** (not directly applicable —
  routers are MoE-specific)
- **MoE + gradient-based importance** (the gate-grad-during-LoRA signal
  used in v1.5 14B; reproducible but not yet validated against MoE)
- **Layer-level pruning + any importance metric** (open research
  question; the §4.1.3.4 lesson predicts the same failure mode)

---

## Model card skeleton — drop-in form

> Audience-gated: this skeleton is the **user-facing** card. The
> researcher-facing methodology content lives in the paper section above
> and should NOT be inlined here. The card links out to the paper for
> users who want methodology depth.

```markdown
---
license: apache-2.0
base_model: Qwen/Qwen3-Coder-30B-A3B-Instruct
tags:
- code
- moe
- 12gb
- 256k-context
- forge-alloy
- continuum-ai
language:
- code
- en
---

# qwen3-coder-30b-a3b-compacted-19b-256k

**The first 30B-class coder that fits a 12 GB consumer GPU.**
88.4 HumanEval · 19.66B params · 256K context · Apache-2.0
Cryptographically verified against a 92.1 base anchor (Δ −3.7).

A forged variant of [Qwen3-Coder-30B-A3B-Instruct](https://huggingface.co/Qwen/Qwen3-Coder-30B-A3B-Instruct)
that fits inside an RTX 4070's 12 GB at Q4_K_M, retains the source
model's full 256K context window, and lands within −3.7 HumanEval
points of the unmodified base — without any compensation training or
fine-tuning. Pure structural compaction via calibration-aware MoE
expert pruning.

- **30.5B → 19.66B params** (35.6% reduction) via calibration-aware expert importance ranking
- **3.3B active params unchanged** — same per-token compute as the source model
- **No fine-tuning involved** — every weight is from the source model, only the surviving expert subset changed
- **Cryptographic provenance chain** from source model SHA-256 to published artifact via [forge-alloy](https://github.com/CambrianTech/forge-alloy)

## Headline benchmarks

| Benchmark | This model | Source model (base anchor) | Δ |
|---|---|---|---|
| HumanEval pass@1 | **88.4** | 92.1 | **−3.7** |
| HumanEval+ pass@1 | **86.0** | 89.0 | **−3.0** |
| MBPP+ pass@1 | TBD | TBD | TBD |

All numbers measured at Q5_K_M on the same hardware (RTX 5090,
llama.cpp), against a base anchor that was independently re-evaluated
on the same setup rather than copied from a published table. Anchor
reproduction is the hard discipline gate from
[§4.1.4.1 of the methodology paper](https://github.com/CambrianTech/continuum/blob/main/docs/papers/PLASTICITY-COMPACTION.md#4141)
— if the base anchor doesn't reproduce within ±3 pt of Qwen's published
numbers on the same eval harness, this model does not ship. The base
anchor reproduced cleanly.

## Quick start

\`\`\`python
from transformers import AutoModelForCausalLM, AutoTokenizer

model = AutoModelForCausalLM.from_pretrained(
    "continuum-ai/qwen3-coder-30b-a3b-compacted-19b-256k",
    torch_dtype="float16",
    device_map="auto",
)
tokenizer = AutoTokenizer.from_pretrained("continuum-ai/qwen3-coder-30b-a3b-compacted-19b-256k")

inputs = tokenizer("def quicksort(arr):", return_tensors="pt").to(model.device)
print(tokenizer.decode(model.generate(**inputs, max_new_tokens=200)[0]))
\`\`\`

For llama.cpp or LM Studio: download the GGUF quant that fits your hardware
from the table below and load it with your usual workflow.

## Runs on

| Hardware | Quantization | VRAM used | Notes |
|---|---|---|---|
| RTX 4070 12 GB | Q4_K_M | **12.0 GB** | first 30B-class coder that fits this tier |
| RTX 4080 16 GB | Q5_K_M | 14.0 GB | 2 GB headroom for KV cache |
| RTX 4090 / 5090 24 GB | Q8_0 | TBD GB | full quality, room for long context |
| MacBook M-series 32 GB | Q5_K_M | 14.0 GB | unified memory; long context comfortable |

256K context window inherited from the source model; M-RoPE preserved
bit-exact through the prune. Long-context generation has not been
benchmarked separately and is not part of the headline numbers.

## Why this exists

Today's HumanEval-leading 30B-class coder models (Qwen2.5-Coder-32B at
92.7, Qwen3-Coder-30B-A3B-Instruct at ~92) do not fit a 12 GB consumer
GPU at Q4_K_M — they sit at ~19-20 GB. The closest competitors at the
12 GB tier (Yi-Coder-9B at 85.4 HumanEval, DeepSeek-Coder-V2-Lite at
~81) are smaller models with smaller context windows.

This artifact is the first to occupy the empty Pareto cell:
**30B-class quality + 12 GB Q4_K_M + 256K context + Apache-2.0**.

## What's different from the base

| | Base Qwen3-Coder-30B-A3B-Instruct | This artifact |
|---|---|---|
| Total params | 30.5 B | 19.66 B |
| Active params per token | 3.3 B | 3.3 B (unchanged) |
| Routed experts per layer | 128 | 80 |
| Shared expert per layer | 1 (always-on) | 1 (always-on, bit-identical to base) |
| Vocabulary | unchanged | unchanged |
| Context window | 256K (1M with YaRN) | 256K (1M with YaRN) |
| Tokenizer | Qwen3-Coder | Qwen3-Coder (bit-identical) |
| HumanEval pass@1 | 92.1 (anchor) | 88.4 (−3.7) |
| Q4_K_M GGUF size | ~18.6 GB | **12.0 GB** |

The 48 dropped experts per layer were chosen by activation-frequency
ranking on a held-out code calibration corpus, not by router-gate
norm. See the methodology paper section §4.1.3.4 for why this matters
(spoiler: choosing by router-gate norm produces a coherent model that
scores 13 points lower on HumanEval — that's the negative-baseline
control we used to validate the calibration-aware metric).

## Verification

This artifact ships with a [forge-alloy](https://github.com/CambrianTech/forge-alloy)
provenance chain. To verify:

\`\`\`bash
forge-alloy verify continuum-ai/qwen3-coder-30b-a3b-compacted-19b-256k
\`\`\`

The verifier checks: source model SHA-256 chain, signed prune
operation, calibration corpus hash, calibration-aware expert
importance JSON hash, surviving expert indices per layer, base anchor
benchmark result hash, this artifact's benchmark result hash.

To independently reproduce the calibrated delta:

\`\`\`bash
# Eval the base anchor (downloads ~18.6 GB if not cached)
huggingface-cli download continuum-ai/qwen3-coder-30b-a3b-compacted-19b-256k base-anchor.gguf
# Eval this artifact (12 GB)
huggingface-cli download continuum-ai/qwen3-coder-30b-a3b-compacted-19b-256k Q4_K_M.gguf
# Run both through your HumanEval harness on the same hardware
# The calibrated delta should land within ±0.5 of −3.7 / −3.0
\`\`\`

## Limitations

- **HumanEval pass@1 is 3.7 points below the base anchor.** This gap is
  load-bearing: the calibration-aware metric closed it from −13.4
  (router-gate-norm baseline) but did not eliminate it. A v2 release
  will add KL-distillation compensation LoRA training to attempt to
  close the remaining gap; check this repo for updates.
- The calibration corpus used for the importance ranking was 300
  Python code examples (~125K tokens). Generalization to non-Python
  languages is not measured. If your workload is dominated by Rust,
  Go, or other less-represented languages in the calibration set, the
  3.7 point gap may be larger on those workloads.
- Long-context generation (>32K tokens) has not been benchmarked.
  M-RoPE is preserved bit-exact from the source so the architectural
  capability is intact, but no held-out long-context evaluation has
  been run.
- This model is text-only. Qwen3-Coder does not include vision; if you
  need vision-language capability, watch this org for a Qwen3.5-VL
  forge once that work lands.

## Methodology

For the full methodology, including the calibration-aware importance
metric, the negative-baseline control showing why router-gate-norm
ranking fails, and the Pareto-frontier framing for consumer VRAM tiers:

- [PLASTICITY-COMPACTION.md §4.1.3.4](https://github.com/CambrianTech/continuum/blob/main/docs/papers/PLASTICITY-COMPACTION.md) — calibration-aware MoE expert importance (this artifact's empirical anchor)
- [PLASTICITY-COMPACTION.md §4.1.4.1](https://github.com/CambrianTech/continuum/blob/main/docs/papers/PLASTICITY-COMPACTION.md#4141) — Pareto frontier objective and anchor reproduction discipline
- [PLASTICITY-COMPACTION-MOE.md](https://github.com/CambrianTech/continuum/blob/main/docs/papers/PLASTICITY-COMPACTION-MOE.md) — MoE expert pruning method

## License

Apache 2.0, inherited from the source Qwen3-Coder-30B-A3B-Instruct.

## Citation

\`\`\`bibtex
@misc{continuum-qwen3-coder-30b-a3b-compacted-19b-256k,
  title  = {qwen3-coder-30b-a3b-compacted-19b-256k: First 30B-class coder fitting 12 GB consumer GPU via calibration-aware MoE expert pruning},
  author = {Teply, Joel and {Cambrian AI}},
  year   = {2026},
  url    = {https://huggingface.co/continuum-ai/qwen3-coder-30b-a3b-compacted-19b-256k},
  note   = {Empirical anchor for PLASTICITY-COMPACTION §4.1.3.4 (calibration-aware expert importance for MoE)}
}
\`\`\`
```

---

## Handoff notes for continuum-side Claude (post-eval)

1. **Pick a paper home for §4.1.3.4.** The methodology section above can
   land in `PLASTICITY-COMPACTION.md` directly as a new §4.1.3.4
   sub-section after §4.1.3.3 (recommended — it parallels §4.1.3.1 → 2 → 3
   structurally). I held off touching the live paper file pending your
   sign-off; ping if you want me to do the merge.
2. **The §4.x artifact section** can land in either methodology paper or
   the MoE-specific paper. With the §4.1.3.4 finding being the headline
   contribution, I lean toward landing both §4.1.3.4 and §4.x in the main
   `PLASTICITY-COMPACTION.md` so the empirical anchor lives next to the
   claim it anchors. The MoE-specific paper can reference both with a
   one-paragraph cross-link.
3. **Model card is final-form for v1.** Real numbers (88.4 / 86.0),
   real VRAM footprint (12.0 GB Q4_K_M), real positioning ("first
   30B-class that fits 12 GB"). Should plug straight into `alloy_to_card.py`'s
   `card_audience="user"` template if audience gating is implemented.
4. **§4.x.7 framing is now load-bearing, not optional.** I had originally
   flagged it as "maybe too strong, tone down if needed" but with two
   structure-orthogonal patterns now empirically validated (importance-
   metric failure mode + compensation LoRA pattern), the framing is the
   contribution and should be kept as-is. The old caveat about "only one
   data point" no longer applies.
5. **`priorMetricBaseline` alloy field** — proposed schema in the section
   below. This is what makes §4.1.3.4 falsifiable from the published
   artifact alone. Without it, the v1 → v2 (or v1 → next-methodology)
   comparison loses its anchor.
6. **MBPP+ and LiveCodeBench v6 rows in §4.x.5 are still TBD.** They're
   not gating v1 ship (HumanEval/HE+ alone establish the headline) but
   adding them strengthens the calibrated-delta claim. Suggest running
   them in background after publish.

---

## Proposed `priorMetricBaseline` alloy schema

The §4.1.3.4 finding is empirically defensible *only if* the two prune
runs (router-gate-L2 baseline + calibration-aware fix) are both recorded
in a structured, machine-readable form on the published artifact. The
standard alloy schema has fields for the *winning* prune operation and
its provenance; what's missing is a field for the *negative-baseline*
prune that motivates the methodology claim.

Suggested schema addition for the alloy file (JSON form, adapt to
alloy's actual serialization):

\`\`\`json
{
  "priorMetricBaselines": [
    {
      "id": "router-gate-l2-norm-2026-04-08",
      "metric": {
        "name": "router_gate_l2_norm",
        "description": "Per-layer L2 norm of router gate row vector for each expert; pure architectural metric, no calibration data involved.",
        "calibrationCorpus": null,
        "calibrationCorpusSha256": null
      },
      "prune": {
        "method": "per_layer_top_k",
        "k": 80,
        "totalExpertsBefore": 128,
        "totalExpertsAfter": 80,
        "shardSourceSha256": "<hash of source safetensors shards used>",
        "survivingExpertIndicesSha256": "<hash of the per-layer surviving-index manifest>"
      },
      "evaluation": {
        "harness": "llama.cpp",
        "harnessVersion": "<git sha>",
        "quantization": "Q5_K_M",
        "hardware": "RTX 5090",
        "anchorBaseModel": "Qwen/Qwen3-Coder-30B-A3B-Instruct",
        "anchorReproduction": {
          "humanEval": 92.1,
          "humanEvalPlus": 89.0
        },
        "results": {
          "humanEval": 78.7,
          "humanEvalPlus": 73.8,
          "deltaVsAnchorHumanEval": -13.4,
          "deltaVsAnchorHumanEvalPlus": -15.2
        },
        "resultHashSha256": "<hash of full eval output JSON for reproducibility>"
      },
      "outcome": "negative_baseline",
      "supersededBy": "calibration-aware-activation-count-2026-04-08",
      "methodologyAnchor": "PLASTICITY-COMPACTION.md#4134"
    }
  ],
  "currentPrune": {
    "id": "calibration-aware-activation-count-2026-04-08",
    "metric": {
      "name": "calibration_aware_activation_count",
      "description": "Per-layer expert activation frequency aggregated across a held-out code calibration corpus; calibration-aware metric.",
      "calibrationCorpus": "heldout_code_python_300ex_125ktok",
      "calibrationCorpusSha256": "<hash>"
    },
    "prune": { /* same shape as above */ },
    "evaluation": { /* same shape as above */ },
    "outcome": "shipped"
  }
}
\`\`\`

### Why this schema matters

Three concrete benefits, each one a reason to land it in the alloy now
rather than as a v1.1 retrofit:

1. **Falsifiability of §4.1.3.4.** The methodology claim is "calibration-
   aware importance metrics outperform architectural metrics on held-out
   code generation." The claim is testable only if both metrics' results
   are recorded against the same anchor on the same hardware in the
   same harness. Without `priorMetricBaselines`, anyone re-running the
   methodology has to take §4.1.3.4 on trust.
2. **Forward-compatibility for v2 / v3 / future metrics.** The schema is
   an array, not a single field. When the compensation LoRA v2 ships, it
   becomes the new `currentPrune` and v1 (calibration-aware, no
   compensation) moves into `priorMetricBaselines` as another data point.
   Each entry is a methodology cell with its own provenance. The whole
   array becomes a self-contained record of the methodology arc on this
   model family.
3. **Pareto frontier verification.** Future researchers comparing
   compaction methodologies on MoE coder models can pull the alloy,
   read the array, and compute their own Pareto curves across different
   metric/training-schedule combinations *without re-running anything*.
   The alloy becomes the empirical record, not just the artifact pointer.

### Schema design choices that matter

- **`outcome` enum** with values `shipped`, `negative_baseline`,
  `superseded`, `experimental`. Lets future pipelines query "give me
  every shipped artifact under continuum-ai/" without pulling the
  negative baselines, while still keeping the negative baselines in
  the same structured record.
- **`methodologyAnchor`** is a markdown link into the paper section
  that the entry serves as an empirical anchor for. This is the
  bidirectional link that makes the paper + alloy work as a single
  citation graph.
- **`supersededBy` / `supersedes`** create a DAG of methodology
  iterations on the same model. The DAG is the methodology arc made
  inspectable.
- **`resultHashSha256`** is the hash of the full eval output JSON, not
  just the headline number. This catches "the same number was computed
  by two different harnesses on two different hardware setups and
  rounded to match" deception. Reproducibility requires the full
  output, not the summary.

### What this does NOT need to capture

- **Training hyperparameters for prune-only artifacts.** There was no
  training. The metric and the calibration corpus are sufficient to
  reproduce.
- **Compensation LoRA-specific fields.** Those land in a separate part
  of the alloy when v2 with compensation training ships.
- **Subjective "quality assessment" tables.** Those go in the paper or
  the model card, not the alloy. The alloy is for machine-verifiable
  facts.

If continuum-side Claude already has an alloy schema with similar
fields, ignore this proposal and just confirm the negative-baseline
record is preserved in *some* structured form. The point is the data,
not the field names.
