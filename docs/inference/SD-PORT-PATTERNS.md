# SD Port Patterns — MLX vs Candle

**Purpose.** Research notes for continuum#897 (m5-test's MLX adapter planning PR, branch `feature/inference-perf-metal`). Not architecture; reading notes. The question this document answers: **where does the runtime-binding seam attach to a generation loop?** Two real SD ports wrap the same model architecture for different runtimes — comparing them surfaces the pattern we'll need for Qwen3.5-Omni on MLX.

**Sources read:**
- `ml-explore/mlx-examples/stable_diffusion/` — the reference MLX port. Primary files: `stable_diffusion/__init__.py` (public API + generation loop), `model_io.py` (weight loading).
- `huggingface/candle/candle-examples/examples/stable-diffusion/main.rs` — Candle's SD example. Tensor ops + device handling visible in the control flow.

## The two ports in one picture

Both ports wrap the same four components — text encoder, VAE, U-Net, scheduler — around the same denoising control flow. What differs is where the runtime attaches:

```
                 MLX port                         Candle port
                 ────────                         ───────────

  Weight load:   mx.load(file) + key remap        ModelFile::get() + per-module
                 + dtype cast (one-pass)          builders (sd_config.build_*)

  Device mgmt:   IMPLICIT (unified memory)        EXPLICIT (Device threaded
                                                  through every Tensor::new)

  Evaluation:    LAZY (symbolic ops,              EAGER (each op computes
                 mx.eval() materializes)          immediately)

  Generation:    Python generator yielding        Rust for-loop over timesteps,
                 latents; caller does mx.eval()   ?-propagated Tensor ops

  CFG blend:     concat → single UNet →           same shape, but with
                 split → blend                    explicit Tensor::cat + chunk
```

## MLX SD — the pattern

**Public API.**

```python
sd = StableDiffusion(model="...", float16=True)
for latent in sd.generate_latents(prompt, cfg_weight=7.5, n_steps=50):
    mx.eval(latent)       # <-- the seam: generation yields, caller forces compute
image = sd.decode(latent)
```

**What `StableDiffusion` holds:**
```python
self.unet           # U-Net module
self.text_encoder   # CLIP
self.autoencoder    # VAE
self.sampler        # SimpleEulerSampler(diffusion_config)
self.tokenizer
self.dtype = mx.float16 if float16 else mx.float32
```

**Denoising loop (paraphrased):**
```python
# CFG = single UNet call with doubled batch
x_t_unet = mx.concatenate([x_t] * 2, axis=0) if cfg_weight > 1 else x_t
eps_pred = self.unet(x_t_unet, t_unet, encoder_x=conditioning)
if cfg_weight > 1:
    eps_text, eps_neg = eps_pred.split(2)
    eps_pred = eps_neg + cfg_weight * (eps_text - eps_neg)
```

**Weight loading (`model_io.py`):**
- `mx.load(weight_file)` — native safetensors
- Mapper functions rename HF keys → MLX module hierarchy (`"to_q"→"query_proj"`, `"self_attn."→"attention."`, etc.)
- Conv weights transposed `(0,2,3,1)` (MLX's layout convention)
- 1×1 Conv weights squeezed to 2D
- FFN `ff.net.0.proj` split into `linear1` + `linear2`
- Dtype cast in one pass: `v.astype(dtype)` per weight
- No explicit device placement — MLX handles residency

**Quantization.** README advertises 4-bit text encoder + 8-bit U-Net. Weight loader only handles dtype cast; quantization must live either in a separate quantized weight file or at module-construction time, not in `model_io.py`.

**The laziness seam matters.** MLX ops return symbolic arrays. Compute only happens when `mx.eval()` is called — which is at the end of each yielded latent in the generation loop. This means:
- The full U-Net + CFG blend + scheduler step are fused into one compute graph per timestep.
- MLX can batch / reorder / fuse internally before `eval`.
- Unified memory residency is solved by MLX — no `.to_device()` calls in user code.

## Candle SD — the pattern

**Public API.**
Command-line driven, but internally:

```rust
let device = candle_examples::device(cpu)?;
device.set_seed(seed)?;
let sd_config = StableDiffusionConfig::v1_5(sliced_attention_size, None, None);
let vae = sd_config.build_vae(&vae_weights, &device, dtype)?;
let unet = sd_config.build_unet(&unet_weights, &device, 4, use_flash_attn, dtype)?;
let scheduler = sd_config.build_scheduler(n_steps)?;
let text_embeddings = /* tokenize + CLIP encode */ ...;
```

**Denoising loop:**

```rust
for (timestep_index, &timestep) in timesteps.iter().enumerate() {
    let latent_model_input = Tensor::cat(&[&latents, &latents], 0)?;
    let latent_model_input = scheduler.scale_model_input(latent_model_input, timestep)?;
    let noise_pred = unet.forward(&latent_model_input, timestep as f64, &text_embeddings)?;
    let noise_pred = noise_pred.chunk(2, 0)?;
    let (noise_pred_uncond, noise_pred_text) = (&noise_pred[0], &noise_pred[1]);
    let noise_pred = (noise_pred_uncond
        + ((noise_pred_text - noise_pred_uncond)? * guidance_scale)?)?;
    latents = scheduler.step(&noise_pred, timestep, &latents)?;
}
```

**Weight loading.**
- `ModelFile::get()` resolves HF-Hub or local path.
- `sd_config.build_*()` constructs modules from safetensors — explicit `VarBuilder` walks the weight dict against the Rust module tree.
- Device + dtype passed as parameters; every `Tensor::new` / `Tensor::from_vec` takes `&device`.

**Abstraction layering.**
- **Candle-runtime concerns:** `Device::Cpu|Cuda|Metal`, `Tensor` ops (`.cat()`, `.chunk()`, `.to_device()`, `.to_dtype()`, `.interpolate2d()`), `Module` trait (`.forward()`).
- **SD-specific:** scheduler step math, CFG concat+chunk+blend, VAE scaling constants (`0.18215` for v1.x), tokenizer + CLIP encoder, inpainting mask handling.

Clean boundary: `Tensor` and `Module` are generic; `stable_diffusion::*` types encapsulate domain behavior.

## Key differences — the diff that informs the MLX ModelBackend adapter

| Axis | MLX | Candle | Implication for Continuum |
|---|---|---|---|
| **Device** | Implicit (framework handles residency) | Explicit (Device threaded through every Tensor) | MLX adapter in `mlx-rs` won't need per-op device arguments — cleaner Rust call sites than Candle. |
| **Evaluation** | Lazy (symbolic ops, `mx.eval()` forces) | Eager (immediate compute per op) | MLX adapter must expose an `eval()` equivalent as the compute trigger. `ModelBackend::generate_step()` should return a symbolic result; the scheduler loop's end-of-step is where eval happens. |
| **Weight loading** | `mx.load(file)` + Python-side key remap | `VarBuilder` walks module tree + explicit dtype | MLX adapter wraps a `mlx::Array`-returning loader; key remap table for HF→MLX naming is one-time static data. |
| **Control flow** | Python generator yielding latents | Rust for-loop with `?`-propagated Tensor | Both work with our scheduler shape. MLX's generator pattern doesn't translate directly to Rust traits — we'd expose a stepwise API (`backend.denoise_step(...)`) and the loop stays in Rust. |
| **Quantization** | Quantized weight files (4-bit/8-bit variants of the same model) | dtype cast + quantized ops on load | MLX strategy matches our GGUF-per-quant-tier approach. We load a pre-quantized MLX file per device tier, not runtime-quantize. |

## Where the MLX bindings "attach" to a generation loop

This is the seam m5-test asked to map. The answer is: **at the module-callable boundary.**

```
Generic generation loop       MLX adapter layer              MLX runtime
(in Rust, in continuum)       (mlx-rs crate via FFI)         (Apple mlx-c)
─────────────────────         ──────────────────             ────────────

for step in timesteps:
    x = scale(x, t)
    
    ε = backend.forward(        → mlx::Array → Python     → U-Net ops
        x, t, cond)                  module.__call__()         (lazy)
    
    ε = cfg_blend(ε, ...)
    
    x = scheduler.step(ε, t, x)
    
    backend.eval(x)             → mx.eval(x)               → materialize
                                                              (compute graph
                                                               dispatched to
                                                               Apple GPU)
```

The **generic generation loop stays in Rust**. The `ModelBackend::forward(...)` and `ModelBackend::eval(...)` methods are where MLX-ness lives — everything else (CFG blend arithmetic, scheduler stepping, guidance scale application) is just tensor math against the backend's array type.

For an LLM (not SD) workload the analog is even simpler — the "generation loop" is the token-by-token autoregressive loop, `forward()` returns next-token logits, and `eval()` is the materialization before sampling.

## Translation to autoregressive LLMs (Qwen3.5 — what we actually need)

The doc leads with SD because both ports are mature reference implementations of the same architecture pair. The actual MLX adapter target in #897 is an autoregressive LLM, not diffusion. Mapping the SD pattern across:

| SD concept | LLM analog |
|---|---|
| Denoising loop over timesteps | Autoregressive loop over tokens |
| `unet.forward(latent, t, cond)` | `model.forward(input_ids, kv_cache)` |
| Symbolic latent → `mx.eval(latent)` | Symbolic logits → `mx.eval(logits)` before sampling |
| `scheduler.step(noise_pred, t, latents)` | `sampler.sample(logits)` → next token id |
| VAE decode at end | Detokenize at end |
| Classifier-free guidance (concat→forward→split→blend) | NOT present in LLM. But the SHAPE — batched-parallel forward then demux outputs — is exactly what our continuous-batching scheduler does for multi-persona concurrent inference: concat N requests into one forward, split outputs back to N streams. **Same pattern, different reason.** |

**Critical implication.** The seam (forward-symbolic / eval-materialize) maps directly: logits returned from `MlxAdapter::forward(input_ids, ...)` are symbolic until the sampler's `mx.eval(logits)` materializes them right before argmax/top-k. Our existing `ModelBackend::generate_step()` interface already has the right shape — Vulkan/CUDA backends return tokens after their own internal materialization; MLX backend defers materialization to the sampler. Each backend chooses where the boundary lives; the trait doesn't enforce.

For phase B of #897 the cleanest study target is `mlx-community`'s Qwen-family examples (text autoregressive, no VAE/sampler complexity) — `ml-explore/mlx-examples/llms/mlx_lm/` is the reference that maps most directly to what we'll implement.

## Implications for Continuum's MLX ModelBackend adapter

Concrete takeaways m5-test can act on in phase A (FFI + scaffold) of #897:

1. **Adapter is a thin shell.** `MlxAdapter: ModelBackend` holds an `mlx_rs::Module` reference and forwards `forward()` / `generate_step()` / `eval()` to the underlying crate. Most of the "work" is trait plumbing, not inference logic.

2. **Don't fight MLX's laziness.** Embrace the symbolic-ops pattern. Our `ModelBackend` trait already has a streaming-token interface (Vulkan/CUDA backends return tokens one-at-a-time); MLX slots in naturally if we treat each `generate_step()` call as "enqueue one timestep's worth of graph, then eval."

3. **Weight loading is a one-pass concern.** HF→MLX key remap table lives in our adapter (static data, reused across load sessions). Store the pre-quantized MLX weight file alongside our GGUF artifacts — same model_registry.json entry, different format field.

4. **No explicit device management in the adapter.** MLX's unified-memory behavior is automatic. The adapter's `new()` / `load()` don't accept a `Device` argument the way Candle's do.

5. **Quantization is baked into the artifact.** We ship `continuum-ai/qwen3.5-4b-omni-mlx-q4` (or similar name) per device tier. At load time the adapter picks the right one based on detected hardware (same device_target_ladder logic we use for GGUF selection).

   **Format note (m5-test 2026-04-16):** `mx.load` reads safetensors but MLX's native indexed-npy format is faster at load time. The forge pipeline (#894) should emit mlx-native alongside the GGUF tier per artifact, not just safetensors. Adapter loader prefers mlx-native when available, falls back to safetensors when not.

6. **Concrete starting point for reading.** The cleanest file-length pattern study is `ml-explore/mlx-examples/stable_diffusion/stable_diffusion/__init__.py` — 100ish lines, shows the full text-encoder → U-Net → VAE → scheduler flow with the MLX seam explicit. For the LLM-side analog, `ml-explore/mlx-examples/qwen/qwen.py` (or `mlx-community/qwen*` repos) would be the direct prior art.

## Open questions for m5-test to explore further

- **KV cache ownership — gates phase B (most critical).** Autoregressive text generation needs cache-per-sequence management. Candle threads the cache through `forward()` explicitly (caller-owned). MLX could either own it internally (simpler API, opaque to caller) OR expose it (more control for our continuous-batching scheduler that wants to manage cache pools across concurrent persona streams). The choice determines whether MLX adapter integrates trivially with the existing scheduler or needs a special-case path. Decide before phase B kicks off — the answer dictates the trait shape.
- **How does `mlx-rs` expose the lazy/eval boundary?** The crate's public API for deferred computation will shape how our `ModelBackend` methods are written. If it exposes `Array::eval()` naturally, we're set. If materialization is implicit in some ops, we need to understand the contract.
- **Audio head + vision head — phases C + D, reference is `mlx-community/Qwen3-Omni`.** Specifically need to understand where the audio encoder/decoder and vision encoder attach to the transformer spine in their MLX implementation. Determines how `ModelBackend` trait expands (default impls for backends that don't have these, real impls for MLX). m5-test (2026-04-16) flagged the trait shape stays additive with `unimplemented!()` defaults — vulkan/cuda/llama.cpp paths inherit the no-op, MLX overrides.
- **Weight format interop:** mlx-native indexed-npy is faster than safetensors. Forge pipeline emits both. Per-artifact one-time conversion is cheap; bake it into #894 forge stage.

## Not in scope of these reading notes

- Actual `mlx-rs` crate API surface — m5-test already evaluated oxideai v0.25.3 as viable; the bindings questions above live in phase A of #897, not in pattern-mapping.
- Specific Qwen3.5-Omni model architecture details (that's phases B/C/D of #897).
- Performance numbers for MLX vs Candle vs ggml-metal — measurement is a separate effort gated on a working adapter.
