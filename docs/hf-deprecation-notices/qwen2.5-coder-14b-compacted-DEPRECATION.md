# DEPRECATED: continuum-ai/qwen2.5-coder-14b-compacted

> **Status**: Deprecated for production use as of 2026-04-07. This model is left visible as a public record of the bugs it surfaced and the methodology revisions it caused. **Use the v1.5 reconstruction (research-only) or wait for v2 (in progress) — see below.**

## Five failure modes documented in this artifact

This was the first model published from the Cambrian forge pipeline. Tonight's attempt to evaluate it surfaced five distinct failure modes, all of which the validation harness in [VALIDATED-TENSOR-SURGERY](https://github.com/CambrianTech/continuum/blob/main/docs/papers/VALIDATED-TENSOR-SURGERY.md) is designed to make impossible. We are leaving this model card visible because the failure modes are themselves the empirical evidence that motivates the [PLASTICITY-COMPACTION](https://github.com/CambrianTech/continuum/blob/main/docs/papers/PLASTICITY-COMPACTION.md) §4.1 case study.

### Failure 1 — unrunnable in `llama.cpp`

The published GGUF refuses to load in `llama.cpp` (and therefore `Ollama`, `LM Studio`, `text-generation-webui`, and every other consumer-hardware runtime that uses the GGUF format):

```
llama_model_load: error loading model: check_tensor_dims:
  tensor 'blk.0.attn_q.weight' has wrong shape;
  expected 5120,5120, got 5120,3200
```

**Root cause**: the v1 forge pipeline reduced the number of Q heads from 40 to 25 (via L2-weight-norm-ranked structured pruning + slice-mode defrag). The resulting `q_proj` projection has shape `[3200, 5120]` (a bottleneck Q projection — 5120 → 3200 → ... → 5120). This is a perfectly valid transformer that loads in `transformers` and `vLLM`. But `llama.cpp`'s GGUF loader hardcodes the assumption `q_proj.shape[0] == hidden_size`, which is satisfied by the standard layout `num_q_heads × head_dim == hidden_size` and is violated by ANY pruning that doesn't preserve `num_q_heads * head_dim == hidden_size`. For `hidden_size=5120, head_dim=128`, the only `num_q_heads` value that satisfies the invariant is exactly `40` — the unpruned base. **Any non-trivial structured pruning of this model violates llama.cpp's invariant.**

**Why we didn't catch it before publication**: pre-publication validation tested only `transformers.save_pretrained → AutoModelForCausalLM.from_pretrained` round-trip. That path works fine on the bottleneck-Q layout because `transformers` handles it natively. We never tested in `llama.cpp`. The validation framework documented in VALIDATED-TENSOR-SURGERY now requires every pre-publication model to pass a Layer 7 `llama.cpp` load test, which would have caught this.

### Failure 2 — no safetensors fallback in this repository

A user who hits Failure 1 cannot work around it by selecting the safetensors version, because no safetensors version exists in this repository. The HF repo contains only `qwen14b-compacted-q5ks.gguf` plus a config and tokenizer. The decision to publish only a quantized format meant that a single runtime-compatibility bug rendered the entire artifact inaccessible to every consumer.

### Failure 3 — the lab's own forge pipeline did not preserve intermediate weights

When we tried to recover from Failures 1+2 by re-deriving a runnable artifact from our own internal forge state, **the source weights were gone**. The forge run that produced this published model did not persist its intermediate checkpoints, and the final fp16 weights had been overwritten. The producer of this model — us — could no longer reproduce the model's pre-defrag state from our own infrastructure.

This is a substrate-level reproducibility failure. In the absence of content-addressed retention guarantees on intermediate forge stages (see [forge-alloy#11](https://github.com/CambrianTech/forge-alloy/issues/11)), the only path to a runnable artifact was to dequantize the published GGUF and treat its weights as the canonical state. **The original fp16 weights are now permanently lost.** Any future evaluation of this model is, by necessity, an evaluation of the dequantized GGUF. A 60GB streaming GPU dequantize via `sentinel-ai/scripts/stream_dequant.py` recovered an evaluation-grade safetensors artifact from this GGUF.

### Failure 4 — the importance metric the v1 forge used was structurally biased

The v1 pipeline used L2 weight norm of `q_proj` rows as the per-head importance metric. Subsequent investigation ([sentinel-ai#155](https://github.com/CambrianTech/sentinel-ai/issues/155), [VALIDATED-TENSOR-SURGERY](https://github.com/CambrianTech/continuum/blob/main/docs/papers/VALIDATED-TENSOR-SURGERY.md) Finding 4) found this metric to be **anti-correlated with actual head importance** for models in the Qwen2.5 family. On Qwen2.5-0.5B specifically, removing the heads ranked lowest by L2 weight norm produced ~105× worse perplexity than removing heads ranked lowest by activation magnitude (the corrected metric). The magnitude of the error on the 14B is documented in PLASTICITY-COMPACTION §4.1 and tonight's calibration measurements show the v1 model lost approximately 58% relative HumanEval coding capability vs the unmodified Qwen2.5-Coder-14B base.

### Failure 5 — `tokenizer_config.json` was dropped during publication

Inspection of the published HF snapshot directory shows `config.json` and `tokenizer.json` are present, but `tokenizer_config.json` is missing entirely. As a result, `AutoTokenizer.from_pretrained("continuum-ai/qwen2.5-coder-14b-compacted").chat_template` returns `None`, and any user invoking `apply_chat_template()` on this model would get a broken result. This is a publication-step bug in the v1 forge pipeline, and is independently documented as the third nested finding in [sentinel-ai#160](https://github.com/CambrianTech/sentinel-ai/issues/160).

## What replaces this model

### v1.5 (research-only, available now in a separate repo)

A bit-identical reconstruction of this model's compute, in a wire format that satisfies the `llama.cpp` invariant. The reconstruction uses pad-mode defrag — physically zero the Q rows and O columns of dead head positions, but preserve the `[hidden_size, hidden_size]` shape of `q_proj` so the artifact loads in every runtime. The `torch.equal(v1_logits, v1.5_logits)` check passes at fp16, mathematically guaranteeing v1.5 produces the same output as v1 on every input.

**v1.5 is a research artifact**, not a production replacement. It has the same coding capability as v1 (per HumanEval calibration: 26.8 / 25.0 pass@1, vs unmodified Qwen2.5-Coder-14B base 64.0 / 57.9). It's only useful as the empirical anchor for the §4.1 deprecation case study.

### v2 (in progress)

A re-derivation of the 14B compacted model using the corrected methodology:
1. **Activation-magnitude importance metric** instead of L2 weight norm (Finding 4 fix)
2. **Per-layer prune budget** instead of global flat ranking (sentinel-ai#165 fix — global flat concentrates all pruning into early layers because residual stream norms grow through the network)
3. **Pad-mode defrag** preserves `q_proj_out == hidden_size` so the artifact loads in `llama.cpp` (Finding 6 fix)
4. **Layer 7 deployment-runtime gate** runs `llama.cpp` on every release candidate before publication (sentinel-ai#160)
5. **Content-addressed forge stage retention** so intermediate checkpoints survive for re-derivation (forge-alloy#11)

v2 will be published as `continuum-ai/qwen2.5-coder-14b-compacted-v2` when it lands. The v2 forge pipeline produces both safetensors AND llama.cpp-validated GGUF in one artifact, with full provenance chain via [forge-alloy](https://github.com/CambrianTech/forge-alloy).

## Why we are publishing this deprecation notice instead of silently fixing it

The straightforward thing to do would be to quietly re-export the artifact, replace the published model, and never mention the broken intermediate version. We're not doing that, because the discovery process that produced the five failures above is itself the strongest empirical evidence we have for the validation framework in [VALIDATED-TENSOR-SURGERY](https://github.com/CambrianTech/continuum/blob/main/docs/papers/VALIDATED-TENSOR-SURGERY.md). The framework's central thesis — *validation against the training framework is not validation; the contract between a model and its consumers includes invariants that are not in the standard config interface* — is exactly what we just learned the hard way by attempting to validate our own published model. **Our own first failure is the strongest evidence we could publish for the framework**, and a model card that omitted it in favor of a clean re-publication would be making the same mistake every other lab in the field is currently making.

## References

- [PLASTICITY-COMPACTION §4.1](https://github.com/CambrianTech/continuum/blob/main/docs/papers/PLASTICITY-COMPACTION.md) — the case study that names this model
- [VALIDATED-TENSOR-SURGERY](https://github.com/CambrianTech/continuum/blob/main/docs/papers/VALIDATED-TENSOR-SURGERY.md) — the validation harness that would have caught all 5 failures
- [sentinel-ai PR #161](https://github.com/CambrianTech/sentinel-ai/pull/161) — the substrate work that ships the v2 forge pipeline
- [sentinel-ai#155](https://github.com/CambrianTech/sentinel-ai/issues/155) — Finding 4 (importance metric)
- [sentinel-ai#160](https://github.com/CambrianTech/sentinel-ai/issues/160) — Finding 6 (Layer 7 runtime gate)
- [sentinel-ai#165](https://github.com/CambrianTech/sentinel-ai/issues/165) — early-layer bias fix
- [forge-alloy#10](https://github.com/CambrianTech/forge-alloy/issues/10) — AlloyTarget runtime declaration
- [forge-alloy#11](https://github.com/CambrianTech/forge-alloy/issues/11) — content-addressed intermediate retention
