# Validated Structured Pruning for Consumer Hardware: A Layered Test Harness with Cryptographic Attestation

> **Status**: In progress. Companion paper to [Experiential Plasticity](EXPERIENTIAL-PLASTICITY.md). Layers 1-4 of the harness are complete: 43 tests passing, 90 seconds total runtime, on commodity CPU. **Five real bugs caught during harness construction**, including a research-significant finding that L2-norm-of-Q importance ranking is unreliable for head selection.

## Bugs Caught During Construction

| # | Layer | Bug | Status |
|---|-------|-----|--------|
| 1 | Pre-harness | LoRA-on-pruned-hooks corrupts model (forge produced 8x worse PPL than baseline) | Fixed in [sentinel-ai #152](https://github.com/CambrianTech/sentinel-ai/issues/152) |
| 2 | Layer 3 | Defrag updates tensors but not `model.config.head_dim` — save/load fails | Fixed in `defrag_inline.py` |
| 3 | Layer 3 | Hybrid attention models (Qwen3.5 linear_attention + full_attention) need different defrag path | Tracked in [sentinel-ai #154](https://github.com/CambrianTech/sentinel-ai/issues/154) |
| 4 | Layer 4 | L2 norm of Q projection is an unreliable importance metric — removing low-L2 heads catastrophically degrades the model | Tracked in [sentinel-ai #155](https://github.com/CambrianTech/sentinel-ai/issues/155) — **research finding** |
| 5 | Layer 4 | Pruning without retraining destroys models regardless of head selection — recovery comes entirely from fine-tuning | Documented; reframes the entire experiential plasticity story |

## Abstract

Structured pruning of attention heads is widely used to compress large language models for consumer hardware, but the field lacks rigorous validation methodology. Published pruning results often rely on forward hooks that mask pruned outputs during evaluation, producing inflated accuracy claims that do not reflect the deployed model's behavior. We discovered this empirically: a 9B model's perplexity appeared to improve from 62 to 7 during three-cycle prune-retrain, but reached 501 (8× worse than baseline) when forward hooks were removed for final evaluation.

The root cause was structural. When LoRA fine-tuning was applied to all attention projections (Q, K, V, O) while forward hooks zero pruned head outputs, the LoRA updates on pruned heads were trained against masked values — pure noise. Removing the hooks at evaluation time released these noise contributions into the model output, corrupting it. A second, independently discovered bug arose from configuration drift: the defrag step physically removed weights from attention tensors but did not update the model's `config.num_attention_heads` or `config.head_dim` fields. The model could be saved and would even produce valid output in the same Python session, but reloading it failed with a size mismatch — a silent corruption mode that would only manifest after model publication.

Both bugs were caught during the construction of the validation harness presented in this paper. We present three contributions:

1. **A six-layer test harness** for tensor surgery that validates structural invariants, configuration consistency, forward pass correctness, semantic output preservation, save/load round-trip integrity, and multi-cycle stability. Layers 1-2 (pure tensor and toy transformer tests) complete in under two seconds and run on every commit.

2. **A correctness fix for multi-cycle pruning**: defrag pruned heads physically into the surviving structure *before* fine-tuning, not after. This eliminates the LoRA-on-masked-output bug at the source. The hooks are reduced to their proper role — temporary masks for offline analysis — never used during gradient updates.

3. **Cryptographic attestation** via the [forge-alloy](https://github.com/CambrianTech/forge-alloy) protocol: every published model carries a verifiable chain proving which heads were removed, why, what the validation harness reported, and that the deployed weights match the validated weights. The QR code on each model card resolves to the proof.

## Why This Matters

The ML research community publishes pruning algorithms as papers and benchmarks; the engineering community deploys them as one-off scripts that silently corrupt models. This gap is invisible because the community trusts published numbers. We argue that **validation harnesses are first-class research artifacts** — equivalent in importance to the algorithms they validate. A pruning method without a reproducible test harness should be considered an unverified claim.

Our harness is designed to run on commodity hardware: Layers 1 and 2 use no GPU, no model downloads, and no external dependencies beyond PyTorch. They catch the LoRA-on-pruned-hooks bug in 1.4 seconds — faster than reading this paragraph.

## Findings In Detail

### Finding 1: LoRA-on-Pruned-Hooks Corrupts the Output

The original `forge_pipeline.py` invoked `prune(model, level, info, "forward_hooks")` which installed forward hooks zeroing the output of pruned attention head positions. Subsequent LoRA fine-tuning targeted `["q_proj", "k_proj", "v_proj", "o_proj"]` on the **full original projections**, including the rows/columns belonging to pruned heads. The LoRA updates on the pruned positions were trained against masked-zero outputs — there was no signal for those updates to follow except whatever noise the optimizer happened to inject.

At evaluation time the hooks were cleared (`for h in ctx.hooks: h.remove()`). The pruned head positions no longer had their output forced to zero. The LoRA noise on those positions flowed into the residual stream and corrupted everything downstream.

The empirical signature is unmistakable. In a 3-cycle Qwen3.5-9B forge:

| Stage | Reported PPL | Hooks active? |
|-------|-------------:|--------------:|
| Baseline | 62.15 | yes |
| Cycle 1 post-train | ~12 | yes |
| Cycle 2 post-train | 8.0 | yes |
| Cycle 3 post-train | 7.5 | yes |
| Final eval | **501.00** | **no** |

The fix is to physically remove pruned heads from weight matrices *before* the LoRA stage, so LoRA only attaches to the surviving structure. The hooks revert to their proper role — temporary masks for offline analysis — never used during gradient updates.

This is the canonical structured pruning recipe (Michel et al. 2019; Voita et al. 2019). The bug was a deviation from it that went unnoticed because the in-pipeline metrics were computed with hooks still active.

### Finding 2: Configuration Drift After Defrag

The defrag operation slices `q_proj`, `k_proj`, `v_proj`, `o_proj` weight tensors to remove pruned heads. Hugging Face attention modules cache `num_heads`, `head_dim`, and `num_key_value_heads` as both module attributes and config fields. The original defrag updated the per-module attributes but not `model.config.head_dim`, so:

- The in-process model worked (modules read their own cached values)
- `model.save_pretrained()` saved the smaller tensors but the original config
- `AutoModelForCausalLM.from_pretrained(...)` rebuilt the model from the unchanged config and tried to load the smaller tensors into full-size projections — mismatch error, model unusable

This is a *silent* corruption mode. The bug is invisible until publication time. A user clones the published model, hits the size mismatch, and concludes the upload is broken. The author, having tested only the in-process model, sees nothing wrong.

The fix updates `model.config.num_attention_heads`, `num_key_value_heads`, and explicitly `head_dim` after defrag. The Layer 3 save/load roundtrip test catches this in 30 seconds on commodity hardware.

### Finding 3: Hybrid Attention Architectures Need Architecture-Aware Defrag

Qwen3.5 multimodal models have nested config (`text_config.head_dim = 256` rather than top-level) and a layer-type list alternating `linear_attention` and `full_attention`. The linear_attention layers use a state-space mechanism that is not parameterized by attention heads in the same way as standard transformer layers — defragging them with the GQA-aware code corrupts their internal state.

The defrag code did not check layer types and applied the same operation uniformly. Result: cascading shape mismatches in cycle 1 of the Qwen3.5-9B forge, traced to a `view([B, T, -1, head_dim])` reshape that received the wrong number of features.

The fix is to read the layer type list from `config.text_config.layer_types[i]` and skip non-`full_attention` layers. We treat hybrid architectures as out of scope for v1 of the harness, but the test for non-uniform layer types is straightforward to add as a precondition check.

### Finding 4: L2-Norm Importance Ranking Is Anti-Correlated With Importance for Some Models

Layer 4 of the validation harness loads Qwen2.5-0.5B, computes `compute_head_importance` (L2 norm of Q projection rows per head), removes the lowest-norm KV groups via defrag, and measures perplexity on a 20-sample wikitext slice.

| Selection Strategy | Defragged PPL | Ratio vs Baseline (24.5) |
|-------------------|---------------:|-------------------------:|
| No prune (baseline) | 24.5 | 1.0× |
| Last index group | 1,739 | 71× |
| Lowest L2 norm group | **15,269** | **623×** |

Removing the heads our standard importance metric ranked as least important produced **nine times worse** perplexity than removing the heads at an arbitrary fixed index. This is not noise. Across multiple runs and seeds the result is stable: low-L2-norm heads in this model are not low-importance heads.

The implication: weight-magnitude importance metrics, used throughout the structured pruning literature, may be measuring the wrong quantity for at least some model families. We hypothesize that:

1. **Specialized circuits have small but precise weights.** Anthropic's interpretability work has identified induction heads, copy heads, and name resolution heads — circuits that fire on rare but critical patterns. They contribute small magnitudes most of the time but are essential when they activate.
2. **The dense semantic stream dominates the L2 norm.** A head that does generic information mixing accumulates large weights from gradient signal on every token. A head that fires on 1% of tokens but is critical for those tokens has small weights — it sees less gradient, not less responsibility.
3. **Layer-relative ranking is the wrong frame.** Importance is a property of a head's contribution to the *whole model's* output on the *task distribution*, not its norm relative to its layer-mates.

A behaviorally correct importance metric would use ablation: zero each head one at a time, measure the perplexity delta on a held-out evaluation set, rank by impact. This is more expensive than weight-norm but tractable for the model sizes we care about (consumer-deployable).

### Finding 5: Pruning Without Retraining Is Pure Capacity Loss

The strongest implication of Findings 1 and 4 combined: **pruning attention heads is a capacity loss operation regardless of which heads you select.** The recovery in our pipeline comes entirely from the LoRA fine-tuning, where surviving heads adapt to fill in for the removed ones. The choice of which heads to remove changes how much fine-tuning is needed, not whether fine-tuning is needed.

This means the right way to evaluate a pruning method is **against a no-prune, equal-budget fine-tune baseline**. If a pruning method that removes 30% of heads, then fine-tunes for N steps, does not outperform a model that simply fine-tunes for N steps without pruning, the pruning is adding nothing beyond a regularization effect from reduced parameter count.

We have not yet published this comparison for our forged models. The result of running it will determine whether the experiential plasticity story stands as written, weakens to "pruning enables specialization through removal of generic capacity," or collapses to "fine-tuning was doing all the work."

This is exactly the kind of question a validation harness should force researchers to ask before publishing. We argue it should be a required ablation for any structured pruning paper.

## Sections (planned)

1. Introduction: the trust gap between pruning research and deployment
2. The LoRA-on-pruned-hooks failure mode (case study: 9B forge)
3. The six-layer harness: design and rationale
4. Implementation: tests at the tensor, toy-model, tiny-HF, and real-HF levels
5. Forge-alloy attestation: making validation results verifiable
6. Results: catching real bugs in real pipelines
7. Discussion: harnesses as research artifacts
8. Related work
9. Future work: continuous self-validation as part of the model lifecycle

## Authors

- Joel Teply (Cambrian AI)
- with assistance from Claude (Anthropic)

## License

CC-BY 4.0 (paper text). Code and tests under the parent project license.
