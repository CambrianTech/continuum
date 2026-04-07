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
