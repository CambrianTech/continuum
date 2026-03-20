//! Vendored from candle-transformers 0.8.4 `models/quantized_llama.rs`.
//!
//! Changes from upstream:
//!   1. Replaced hardcoded `MAX_SEQ_LEN = 4096` with `context_length` read from
//!      GGUF metadata (`llama.context_length`). The upstream Llama implementation
//!      forgot to read this, while Qwen2 and Phi3 in the same library do.
//!   2. Exposed `context_length` as a public field on `ModelWeights` so callers
//!      can query the model's true context limit.
//!   3. Changed crate-internal imports (`candle::`, `crate::`) to external crate
//!      imports (`candle_core::`, `candle_transformers::`) since this is vendored
//!      outside of candle-transformers.
//!
//! When candle-transformers publishes a release that reads context_length from
//! GGUF metadata for Llama, this vendored copy can be removed.

use std::collections::HashMap;

use candle_core::quantized::QTensor;
use candle_core::quantized::{ggml_file, gguf_file};
use candle_core::{DType, Device, IndexOp, Result, Tensor};
use candle_nn::Module;

// Toggle: use original Candle RmsNorm for A/B testing
const USE_ORIGINAL_RMSNORM: bool = true;

/// Softmax with f64 sum accumulation (matching llama.cpp's ggml_float).
///
/// Candle's softmax_last_dim accumulates exp() sum in f32. llama.cpp uses f64.
/// With 187 tokens × 48 layers × 25 heads = 1200 softmax calls during prefill,
/// the f32 error compounds and shifts attention patterns enough to cause divergence.
fn softmax_f64(xs: &Tensor) -> Result<Tensor> {
    let device = xs.device();
    let dims = xs.dims();
    let last_dim = *dims.last().unwrap();

    let xs_f32 = xs.to_dtype(DType::F32)?;
    let flat = xs_f32.flatten_all()?;
    let data: Vec<f32> = flat.to_vec1()?;

    let mut result = vec![0f32; data.len()];
    for chunk in 0..(data.len() / last_dim) {
        let start = chunk * last_dim;
        let end = start + last_dim;
        let row = &data[start..end];

        // Max for numerical stability
        let max = row.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

        // Accumulate exp() sum in f64 (matching llama.cpp)
        let mut sum: f64 = 0.0;
        for i in 0..last_dim {
            let e = ((row[i] - max) as f64).exp();
            result[start + i] = e as f32;
            sum += e;
        }

        // Normalize
        let inv_sum = (1.0 / sum) as f32;
        for i in 0..last_dim {
            result[start + i] *= inv_sum;
        }
    }

    Tensor::from_vec(result, dims, device)
}

/// RmsNorm with f64 sum-of-squares accumulation (matching llama.cpp).
///
/// Candle's built-in RmsNorm uses f32 accumulation, which loses precision
/// for hidden_size=5120 and compounds across 48 layers, causing token-level
/// divergence from llama.cpp's output. See: https://github.com/huggingface/candle/issues/3410
#[derive(Debug, Clone)]
pub struct RmsNorm {
    weight: Tensor,
    eps: f64,
}

impl RmsNorm {
    pub fn from_qtensor(qtensor: QTensor, eps: f64) -> Result<Self> {
        let weight = qtensor.dequantize(&qtensor.device())?;
        Ok(Self { weight, eps })
    }
}

impl Module for RmsNorm {
    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        if USE_ORIGINAL_RMSNORM {
            // Use original Candle RmsNorm for A/B testing
            candle_nn::ops::rms_norm(x, &self.weight, self.eps as f32)
        } else {
            // f64 accumulation matching llama.cpp
            let device = x.device();
            let x_f32 = x.to_dtype(DType::F32)?;
            let dim = *x_f32.dims().last().unwrap();
            let x_sq = (&x_f32 * &x_f32)?;
            let sum_sq = x_sq.sum_keepdim(candle_core::D::Minus1)?
                .to_device(&Device::Cpu)?;
            let sum_vec: Vec<f32> = sum_sq.flatten_all()?.to_vec1()?;
            let rms_vec: Vec<f32> = sum_vec.iter()
                .map(|&s| ((s as f64 / dim as f64 + self.eps).sqrt()) as f32)
                .collect();
            let rms = Tensor::from_vec(rms_vec, sum_sq.dims().to_vec(), device)?;
            x_f32.broadcast_div(&rms)?.broadcast_mul(&self.weight)
        }
    }
}

/// Default fallback if GGUF metadata doesn't contain context_length.
const DEFAULT_CONTEXT_LENGTH: usize = 4096;

/// Zero-overhead quantized embedding lookup via one-hot matmul.
///
/// Embedding: F16 table on the same device as the model (Metal).
/// 1.55 GB in Metal shared memory. Forward: Metal index_select → F32 cast.
/// Everything stays on GPU. No CPU↔Metal copies.
#[derive(Debug, Clone)]
struct DeviceEmbedding {
    /// F16 embedding table: [vocab_size, hidden_size] on model device
    table: Tensor,
    hidden_size: usize,
}

impl DeviceEmbedding {
    fn from_gguf<R: std::io::Seek + std::io::Read>(
        ct: &gguf_file::Content,
        reader: &mut R,
        tensor_name: &str,
        hidden_size: usize,
        device: &Device,
    ) -> Result<Self> {
        // Dequantize to F32 on CPU, then move to device.
        // Must be F32 (not F16) — F16 loses precision on embedding values,
        // causing wrong logit distributions for special tokens.
        let qt_cpu = ct.tensor(reader, tensor_name, &Device::Cpu)?;
        let table = qt_cpu.dequantize(&Device::Cpu)?.to_device(device)?;
        Ok(Self { table, hidden_size })
    }

    fn from_qtensor(qtensor: QTensor, hidden_size: usize, device: &Device) -> Result<Self> {
        let table = qtensor.dequantize(&Device::Cpu)?.to_device(device)?;
        Ok(Self { table, hidden_size })
    }

    fn forward(&self, token_ids: &Tensor) -> Result<Tensor> {
        // Table is already F32 — just index_select
        let embeddings = self.table.index_select(&token_ids.flatten_all()?, 0)?;
        let orig_dims = token_ids.dims();
        if orig_dims.len() == 2 {
            embeddings.reshape((orig_dims[0], orig_dims[1], self.hidden_size))
        } else {
            Ok(embeddings)
        }
    }
}

// QMatMul wrapper adding some tracing.
#[derive(Debug, Clone)]
struct QMatMul {
    inner: candle_core::quantized::QMatMul,
    span: tracing::Span,
}

impl QMatMul {
    fn from_qtensor(qtensor: QTensor) -> Result<Self> {
        let inner = candle_core::quantized::QMatMul::from_qtensor(qtensor)?;
        let span = tracing::span!(tracing::Level::TRACE, "qmatmul");
        Ok(Self { inner, span })
    }

    fn forward(&self, xs: &Tensor) -> Result<Tensor> {
        let _enter = self.span.enter();
        self.inner.forward(xs)
    }

    /// Dequantize the inner weight to a full-precision Tensor, apply LoRA merge,
    /// and store back as a non-quantized QMatMul.
    ///
    /// LoRA formula: W' = W + scale * (B @ A)
    /// where A is [rank, in_features] and B is [out_features, rank]
    ///
    /// QMatMul stores weights transposed: [in_features, out_features] for the
    /// QTensor variant, but dequantize() returns it in the same layout.
    /// After merging, we wrap in QMatMul::Tensor which uses regular matmul.
    fn merge_lora(
        &mut self,
        lora_a: &Tensor,
        lora_b: &Tensor,
        scale: f64,
        device: &Device,
    ) -> Result<()> {
        // Dequantize the current weight to F32
        let base_weight = match &self.inner {
            candle_core::quantized::QMatMul::QTensor(qt) => qt.dequantize(device)?,
            candle_core::quantized::QMatMul::Tensor(t) => t.clone(),
            candle_core::quantized::QMatMul::TensorF16(t) => t.to_dtype(DType::F32)?,
        };

        // LoRA merge: W' = W + scale * (B @ A)
        // B is [out_features, rank], A is [rank, in_features]
        // B @ A gives [out_features, in_features]
        let delta = lora_b.matmul(lora_a)?;
        let scaled_delta = (delta * scale)?;

        // QMatMul stores weights as [out, in] (same as the LoRA delta).
        // But dequantize may return [in, out] depending on format.
        // If shapes don't match, try transposing the delta.
        let merged = if base_weight.dims() == scaled_delta.dims() {
            (&base_weight + &scaled_delta)?
        } else {
            let delta_t = scaled_delta.t()?;
            if base_weight.dims() == delta_t.dims() {
                (&base_weight + &delta_t)?
            } else {
                return Err(candle_core::Error::Msg(format!(
                    "Shape mismatch: base={:?}, delta={:?}, delta_t={:?}",
                    base_weight.dims(),
                    scaled_delta.dims(),
                    delta_t.dims()
                )));
            }
        };

        self.inner = candle_core::quantized::QMatMul::Tensor(merged);
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct Mlp {
    feed_forward_w1: QMatMul,
    feed_forward_w2: QMatMul,
    feed_forward_w3: QMatMul,
}

impl Module for Mlp {
    fn forward(&self, xs: &Tensor) -> Result<Tensor> {
        let w1 = self.feed_forward_w1.forward(xs)?;
        let w3 = self.feed_forward_w3.forward(xs)?;
        self.feed_forward_w2
            .forward(&(candle_nn::ops::silu(&w1)? * w3)?)
    }
}

#[derive(Debug, Clone)]
enum MlpOrMoe {
    Mlp(Mlp),
    MoE {
        n_expert_used: usize,
        feed_forward_gate_inp: QMatMul,
        experts: Vec<Mlp>,
    },
}

impl Module for MlpOrMoe {
    fn forward(&self, xs: &Tensor) -> Result<Tensor> {
        match self {
            Self::MoE {
                feed_forward_gate_inp,
                experts,
                n_expert_used,
            } => {
                let (b_size, seq_len, hidden_dim) = xs.dims3()?;
                let xs = xs.reshape(((), hidden_dim))?;
                let router_logits = feed_forward_gate_inp.forward(&xs)?;
                let routing_weights = candle_nn::ops::softmax_last_dim(&router_logits)?;

                let routing_weights = routing_weights.to_dtype(DType::F32)?.to_vec2::<f32>()?;

                let mut top_x = vec![vec![]; experts.len()];
                let mut selected_rws = vec![vec![]; experts.len()];
                for (row_idx, rw) in routing_weights.iter().enumerate() {
                    let mut dst = (0..rw.len() as u32).collect::<Vec<u32>>();
                    dst.sort_by(|&i, &j| rw[j as usize].total_cmp(&rw[i as usize]));
                    let mut sum_routing_weights = 0f32;
                    for &expert_idx in dst.iter().take(*n_expert_used) {
                        let expert_idx = expert_idx as usize;
                        let routing_weight = rw[expert_idx];
                        sum_routing_weights += routing_weight;
                        top_x[expert_idx].push(row_idx as u32);
                    }
                    for &expert_idx in dst.iter().take(*n_expert_used) {
                        let expert_idx = expert_idx as usize;
                        let routing_weight = rw[expert_idx];
                        selected_rws[expert_idx].push(routing_weight / sum_routing_weights)
                    }
                }

                let mut ys = xs.zeros_like()?;
                for (expert_idx, expert_layer) in experts.iter().enumerate() {
                    let top_x = &top_x[expert_idx];
                    if top_x.is_empty() {
                        continue;
                    }
                    let top_x = Tensor::new(top_x.as_slice(), xs.device())?;
                    let selected_rws =
                        Tensor::new(selected_rws[expert_idx].as_slice(), xs.device())?
                            .reshape(((), 1))?;
                    let current_state = xs.index_select(&top_x, 0)?.reshape(((), hidden_dim))?;
                    let current_hidden_states = expert_layer.forward(&current_state)?;
                    let current_hidden_states =
                        current_hidden_states.broadcast_mul(&selected_rws)?;
                    ys = ys.index_add(&top_x, &current_hidden_states, 0)?;
                }

                let ys = ys.reshape((b_size, seq_len, hidden_dim))?;
                Ok(ys)
            }
            Self::Mlp(mlp) => mlp.forward(xs),
        }
    }
}

#[derive(Debug, Clone)]
struct LayerWeights {
    attention_wq: QMatMul,
    attention_wk: QMatMul,
    attention_wv: QMatMul,
    attention_wo: QMatMul,
    /// Optional Q/K/V biases (Qwen2 has bias on Q, K, V; Llama does not).
    attention_bq: Option<Tensor>,
    attention_bk: Option<Tensor>,
    attention_bv: Option<Tensor>,
    attention_norm: RmsNorm,
    mlp_or_moe: MlpOrMoe,
    ffn_norm: RmsNorm,
    n_head: usize,
    n_kv_head: usize,
    head_dim: usize,
    cos: Tensor,
    sin: Tensor,
    neg_inf: Tensor,
    kv_cache: Option<(Tensor, Tensor)>,
    span_attn: tracing::Span,
    span_rot: tracing::Span,
    span_mlp: tracing::Span,
}

fn masked_fill(on_false: &Tensor, mask: &Tensor, on_true: &Tensor) -> Result<Tensor> {
    let shape = mask.shape();
    let m = mask.where_cond(&on_true.broadcast_as(shape.dims())?, on_false)?;
    Ok(m)
}

impl LayerWeights {
    fn apply_rotary_emb(&self, x: &Tensor, index_pos: usize) -> Result<Tensor> {
        let _enter = self.span_rot.enter();
        let (_b_sz, _n_head, seq_len, _n_embd) = x.dims4()?;
        let cos = self.cos.narrow(0, index_pos, seq_len)?;
        let sin = self.sin.narrow(0, index_pos, seq_len)?;
        candle_nn::rotary_emb::rope_i(&x.contiguous()?, &cos, &sin)
    }

    fn forward_attn(
        &mut self,
        x: &Tensor,
        mask: Option<&Tensor>,
        index_pos: usize,
    ) -> Result<Tensor> {
        let _enter = self.span_attn.enter();
        let (b_sz, seq_len, _n_embd) = x.dims3()?;
        let mut q = self.attention_wq.forward(x)?;
        let mut k = self.attention_wk.forward(x)?;
        let mut v = self.attention_wv.forward(x)?;

        // Apply attention biases (Qwen2 has bias on Q, K, V; Llama skips this)
        if let Some(bq) = &self.attention_bq {
            q = q.broadcast_add(bq)?;
        }
        if let Some(bk) = &self.attention_bk {
            k = k.broadcast_add(bk)?;
        }
        if let Some(bv) = &self.attention_bv {
            v = v.broadcast_add(bv)?;
        }

        let q = q
            .reshape((b_sz, seq_len, self.n_head, self.head_dim))?
            .transpose(1, 2)?;
        let k = k
            .reshape((b_sz, seq_len, self.n_kv_head, self.head_dim))?
            .transpose(1, 2)?;
        let v = v
            .reshape((b_sz, seq_len, self.n_kv_head, self.head_dim))?
            .transpose(1, 2)?
            .contiguous()?;

        let q = self.apply_rotary_emb(&q, index_pos)?;
        let k = self.apply_rotary_emb(&k, index_pos)?;

        let (k, v) = match &self.kv_cache {
            None => (k, v),
            Some((k_cache, v_cache)) => {
                if index_pos == 0 {
                    (k, v)
                } else {
                    let k = Tensor::cat(&[k_cache, &k], 2)?;
                    let v = Tensor::cat(&[v_cache, &v], 2)?;
                    (k, v)
                }
            }
        };
        self.kv_cache = Some((k.clone(), v.clone()));

        let y = if q.device().is_metal() && seq_len == 1 {
            // Metal SDPA vectorized kernel — fast path for single-token generation.
            // is_causal=true with real quantized weights produces corrupted KV cache
            // (verified: synthetic tensors pass but real 14B weights produce garbage).
            candle_nn::ops::sdpa(
                &q,
                &k,
                &v,
                None,
                false,
                1. / (self.head_dim as f32).sqrt(),
                1.,
            )?
        } else {
            // Manual attention for batch prefill and CPU/CUDA generation.
            let k = candle_transformers::utils::repeat_kv(k, self.n_head / self.n_kv_head)?;
            let v = candle_transformers::utils::repeat_kv(v, self.n_head / self.n_kv_head)?;

            let att = (q.matmul(&k.t()?)? / (self.head_dim as f64).sqrt())?;
            let att = match mask {
                None => att,
                Some(mask) => {
                    let mask = mask.broadcast_as(att.shape())?;
                    masked_fill(&att, &mask, &self.neg_inf)?
                }
            };
            let att = candle_nn::ops::softmax_last_dim(&att)?;
            att.matmul(&v.contiguous()?)?
        };

        // Reshape from [batch, n_head, seq, head_dim] to [batch, seq, n_head * head_dim].
        // For compacted models n_head * head_dim != n_embd (e.g. 25*128=3200 vs 5120),
        // so we must NOT use n_embd here. W_o then projects back to hidden_size.
        let y = y
            .transpose(1, 2)?
            .reshape(&[b_sz, seq_len, self.n_head * self.head_dim])?;
        let y = self.attention_wo.forward(&y)?;
        Ok(y)
    }
}

/// Projection types for LoRA weight mapping.
#[derive(Debug, Clone, Copy)]
enum Projection {
    Q,
    K,
    V,
    O,
    Gate,
    Up,
    Down,
}

/// Parse a LoRA weight name to extract layer index and projection type.
///
/// Handles PEFT naming conventions:
///   "base_model.model.model.layers.{N}.self_attn.{q|k|v|o}_proj"
///   "base_model.model.model.layers.{N}.mlp.{gate|up|down}_proj"
///   "model.layers.{N}.self_attn.{q|k|v|o}_proj"
fn parse_lora_layer_name(name: &str) -> Option<(usize, Projection)> {
    // Strip common prefixes
    let name = name.strip_prefix("base_model.model.").unwrap_or(name);
    let name = name.strip_prefix("model.").unwrap_or(name);

    // Now expect: "layers.{N}.self_attn.{X}_proj" or "layers.{N}.mlp.{X}_proj"
    let parts: Vec<&str> = name.split('.').collect();

    // Minimum: ["layers", N, "self_attn"|"mlp", "X_proj"]
    if parts.len() < 4 || parts[0] != "layers" {
        return None;
    }

    let layer_idx: usize = parts[1].parse().ok()?;
    let module = parts[2];
    let proj_name = parts[3];

    let projection = match (module, proj_name) {
        ("self_attn", "q_proj") => Projection::Q,
        ("self_attn", "k_proj") => Projection::K,
        ("self_attn", "v_proj") => Projection::V,
        ("self_attn", "o_proj") => Projection::O,
        ("mlp", "gate_proj") => Projection::Gate,
        ("mlp", "up_proj") => Projection::Up,
        ("mlp", "down_proj") => Projection::Down,
        _ => return None,
    };

    Some((layer_idx, projection))
}

#[derive(Debug, Clone)]
pub struct ModelWeights {
    tok_embeddings: DeviceEmbedding,
    layers: Vec<LayerWeights>,
    norm: RmsNorm,
    output: QMatMul,
    masks: HashMap<usize, Tensor>,
    span: tracing::Span,
    span_output: tracing::Span,
    /// Context length read from GGUF metadata. This is the model's true limit.
    pub context_length: usize,
}

fn precomput_freqs_cis(
    head_dim: usize,
    freq_base: f32,
    context_length: usize,
    device: &Device,
) -> Result<(Tensor, Tensor)> {
    let theta: Vec<_> = (0..head_dim)
        .step_by(2)
        .map(|i| 1f32 / freq_base.powf(i as f32 / head_dim as f32))
        .collect();
    let theta = Tensor::new(theta.as_slice(), device)?;
    let idx_theta = Tensor::arange(0, context_length as u32, device)?
        .to_dtype(DType::F32)?
        .reshape((context_length, 1))?
        .matmul(&theta.reshape((1, theta.elem_count()))?)?;
    let cos = idx_theta.cos()?;
    let sin = idx_theta.sin()?;
    Ok((cos, sin))
}

impl ModelWeights {
    pub fn from_ggml(mut ct: ggml_file::Content, gqa: usize) -> Result<Self> {
        let head_dim = (ct.hparams.n_embd / ct.hparams.n_head) as usize;
        let context_length = DEFAULT_CONTEXT_LENGTH; // GGML doesn't store context_length
        let (cos, sin) = precomput_freqs_cis(head_dim, 10000., context_length, &ct.device)?;
        let neg_inf = Tensor::new(f32::NEG_INFINITY, &ct.device)?;
        let embedding_length = ct.hparams.n_embd as usize;
        let tok_embeddings_q = ct.remove("tok_embeddings.weight")?;
        let tok_embeddings = DeviceEmbedding::from_qtensor(tok_embeddings_q, embedding_length, &ct.device)?;
        let norm = RmsNorm::from_qtensor(ct.remove("norm.weight")?, 1e-5)?;
        let output = ct.remove("output.weight")?;
        let mut layers = Vec::with_capacity(ct.hparams.n_layer as usize);
        for layer_idx in 0..ct.hparams.n_layer {
            let prefix = format!("layers.{layer_idx}");
            let attention_wq = ct.remove(&format!("{prefix}.attention.wq.weight"))?;
            let attention_wk = ct.remove(&format!("{prefix}.attention.wk.weight"))?;
            let attention_wv = ct.remove(&format!("{prefix}.attention.wv.weight"))?;
            let attention_wo = ct.remove(&format!("{prefix}.attention.wo.weight"))?;
            let mlp_or_moe = {
                let feed_forward_w1 = ct.remove(&format!("{prefix}.feed_forward.w1.weight"))?;
                let feed_forward_w2 = ct.remove(&format!("{prefix}.feed_forward.w2.weight"))?;
                let feed_forward_w3 = ct.remove(&format!("{prefix}.feed_forward.w3.weight"))?;
                MlpOrMoe::Mlp(Mlp {
                    feed_forward_w1: QMatMul::from_qtensor(feed_forward_w1)?,
                    feed_forward_w2: QMatMul::from_qtensor(feed_forward_w2)?,
                    feed_forward_w3: QMatMul::from_qtensor(feed_forward_w3)?,
                })
            };
            let attention_norm = ct.remove(&format!("{prefix}.attention_norm.weight"))?;
            let ffn_norm = ct.remove(&format!("{prefix}.ffn_norm.weight"))?;
            let span_attn = tracing::span!(tracing::Level::TRACE, "attn");
            let span_rot = tracing::span!(tracing::Level::TRACE, "attn-rot");
            let span_mlp = tracing::span!(tracing::Level::TRACE, "attn-mlp");
            layers.push(LayerWeights {
                attention_wq: QMatMul::from_qtensor(attention_wq)?,
                attention_wk: QMatMul::from_qtensor(attention_wk)?,
                attention_wv: QMatMul::from_qtensor(attention_wv)?,
                attention_wo: QMatMul::from_qtensor(attention_wo)?,
                attention_bq: None, // GGML format doesn't have bias
                attention_bk: None,
                attention_bv: None,
                attention_norm: RmsNorm::from_qtensor(attention_norm, 1e-5)?,
                mlp_or_moe,
                ffn_norm: RmsNorm::from_qtensor(ffn_norm, 1e-5)?,
                n_head: ct.hparams.n_head as usize,
                n_kv_head: ct.hparams.n_head as usize / gqa,
                head_dim: (ct.hparams.n_embd / ct.hparams.n_head) as usize,
                cos: cos.clone(),
                sin: sin.clone(),
                neg_inf: neg_inf.clone(),
                kv_cache: None,
                span_attn,
                span_rot,
                span_mlp,
            })
        }
        let span = tracing::span!(tracing::Level::TRACE, "model");
        let span_output = tracing::span!(tracing::Level::TRACE, "output");
        Ok(Self {
            tok_embeddings,
            layers,
            norm,
            output: QMatMul::from_qtensor(output)?,
            masks: HashMap::new(),
            span,
            span_output,
            context_length,
        })
    }

    pub fn from_gguf<R: std::io::Seek + std::io::Read>(
        ct: gguf_file::Content,
        reader: &mut R,
        device: &Device,
    ) -> Result<Self> {
        // Determine architecture prefix (llama, qwen2, etc.) from metadata.
        // GGUF stores metadata keys as "{arch}.{param}", e.g. "qwen2.block_count".
        let arch = ct
            .metadata
            .get("general.architecture")
            .and_then(|v| v.to_string().ok())
            .cloned()
            .unwrap_or_else(|| "llama".to_string());

        let md_get = |s: &str| match ct.metadata.get(s) {
            None => candle_core::bail!("cannot find {s} in metadata"),
            Some(v) => Ok(v),
        };

        // Architecture-aware metadata key lookup
        let arch_key = |param: &str| format!("{arch}.{param}");

        let context_length = md_get(&arch_key("context_length"))
            .and_then(|v| v.to_u32())
            .map(|v| v as usize)
            .unwrap_or(DEFAULT_CONTEXT_LENGTH);

        let n_expert = md_get(&arch_key("expert_count"))
            .and_then(|v| v.to_u32())
            .unwrap_or(0) as usize;
        let n_expert_used = md_get(&arch_key("expert_used_count"))
            .and_then(|v| v.to_u32())
            .unwrap_or(0) as usize;
        let head_count = md_get(&arch_key("attention.head_count"))?.to_u32()? as usize;
        let head_count_kv = md_get(&arch_key("attention.head_count_kv"))?.to_u32()? as usize;
        let block_count = md_get(&arch_key("block_count"))?.to_u32()? as usize;
        let embedding_length = md_get(&arch_key("embedding_length"))?.to_u32()? as usize;
        // Head dimension: try explicit metadata first, then derive from K weight shape.
        // NEVER use embedding_length / head_count — it gives wrong answers for compacted
        // models where head_count was reduced (e.g. 5120/25=204 instead of 128).
        let head_dim = md_get(&arch_key("attention.key_length"))
            .and_then(|v| v.to_u32())
            .or_else(|_| md_get(&arch_key("attention.value_length")).and_then(|v| v.to_u32()))
            .or_else(|_| md_get(&arch_key("rope.dimension_count")).and_then(|v| v.to_u32()))
            .map(|v| v as usize)
            .unwrap_or_else(|_| {
                // Last resort: derive from K projection weight shape.
                // K weight is [embedding_length, kv_heads * head_dim], so:
                // head_dim = K_weight_dim1 / kv_heads
                // But we don't have the tensor yet. Use embedding_length / ORIGINAL head_count
                // approximation: for standard models this is correct, for compacted we need metadata.
                // Qwen2 always uses 128.
                if arch == "qwen2" { 128 } else { embedding_length / head_count }
            });
        let rope_dim = head_dim;
        let rms_norm_eps =
            md_get(&arch_key("attention.layer_norm_rms_epsilon"))?.to_f32()? as f64;

        let rope_freq_base = md_get(&arch_key("rope.freq_base"))
            .and_then(|m| m.to_f32())
            .unwrap_or(10000f32);
        let (cos, sin) = precomput_freqs_cis(rope_dim, rope_freq_base, context_length, device)?;
        let neg_inf = Tensor::new(f32::NEG_INFINITY, device)?;

        // Load embedding directly to CPU — bypasses Metal buffer pool entirely.
        let tok_embeddings = DeviceEmbedding::from_gguf(
            &ct, reader, "token_embd.weight", embedding_length, device,
        )?;
        let norm = RmsNorm::from_qtensor(
            ct.tensor(reader, "output_norm.weight", device)?,
            rms_norm_eps,
        )?;
        // Output head: use dedicated output.weight if present, otherwise re-read
        // token_embd.weight (tied embeddings).
        let output = match ct.tensor(reader, "output.weight", device) {
            Ok(tensor) => tensor,
            Err(_) => ct.tensor(reader, "token_embd.weight", device)?,
        };
        let mut layers = Vec::with_capacity(block_count);
        for layer_idx in 0..block_count {
            let prefix = format!("blk.{layer_idx}");
            let attention_wq = ct.tensor(reader, &format!("{prefix}.attn_q.weight"), device)?;
            let attention_wk = ct.tensor(reader, &format!("{prefix}.attn_k.weight"), device)?;
            let attention_wv = ct.tensor(reader, &format!("{prefix}.attn_v.weight"), device)?;
            let attention_wo =
                ct.tensor(reader, &format!("{prefix}.attn_output.weight"), device)?;

            // Optional attention biases (Qwen2 has Q/K/V bias; Llama does not)
            let attention_bq = ct
                .tensor(reader, &format!("{prefix}.attn_q.bias"), device)
                .ok()
                .map(|qt| qt.dequantize(device))
                .transpose()?;
            let attention_bk = ct
                .tensor(reader, &format!("{prefix}.attn_k.bias"), device)
                .ok()
                .map(|qt| qt.dequantize(device))
                .transpose()?;
            let attention_bv = ct
                .tensor(reader, &format!("{prefix}.attn_v.bias"), device)
                .ok()
                .map(|qt| qt.dequantize(device))
                .transpose()?;

            // Log shapes for first layer to verify compacted model dimensions
            if layer_idx == 0 {
                let log = crate::runtime::logger("candle");
                log.info(&format!("Layer 0 weight shapes: Q={:?} K={:?} V={:?} O={:?}",
                    attention_wq.shape(), attention_wk.shape(), attention_wv.shape(), attention_wo.shape()));
                if let Some(ref bq) = attention_bq {
                    log.info(&format!("Layer 0 bias shapes: Q={:?} K={:?} V={:?}",
                        bq.dims(), attention_bk.as_ref().map(|t| t.dims()), attention_bv.as_ref().map(|t| t.dims())));
                }
                log.info(&format!("Layer 0 config: n_head={}, n_kv_head={}, head_dim={}, rope_dim={}",
                    head_count, head_count_kv, head_dim, rope_dim));
            }

            let mlp_or_moe = if n_expert <= 1 {
                let feed_forward_w1 =
                    ct.tensor(reader, &format!("{prefix}.ffn_gate.weight"), device)?;
                let feed_forward_w2 =
                    ct.tensor(reader, &format!("{prefix}.ffn_down.weight"), device)?;
                let feed_forward_w3 =
                    ct.tensor(reader, &format!("{prefix}.ffn_up.weight"), device)?;
                MlpOrMoe::Mlp(Mlp {
                    feed_forward_w1: QMatMul::from_qtensor(feed_forward_w1)?,
                    feed_forward_w2: QMatMul::from_qtensor(feed_forward_w2)?,
                    feed_forward_w3: QMatMul::from_qtensor(feed_forward_w3)?,
                })
            } else {
                let feed_forward_gate_inp =
                    ct.tensor(reader, &format!("{prefix}.ffn_gate_inp.weight"), device)?;
                let mut experts = Vec::with_capacity(n_expert);
                for i in 0..n_expert {
                    let feed_forward_w1 =
                        ct.tensor(reader, &format!("{prefix}.ffn_gate.{i}.weight"), device)?;
                    let feed_forward_w2 =
                        ct.tensor(reader, &format!("{prefix}.ffn_down.{i}.weight"), device)?;
                    let feed_forward_w3 =
                        ct.tensor(reader, &format!("{prefix}.ffn_up.{i}.weight"), device)?;
                    experts.push(Mlp {
                        feed_forward_w1: QMatMul::from_qtensor(feed_forward_w1)?,
                        feed_forward_w2: QMatMul::from_qtensor(feed_forward_w2)?,
                        feed_forward_w3: QMatMul::from_qtensor(feed_forward_w3)?,
                    })
                }
                MlpOrMoe::MoE {
                    n_expert_used,
                    feed_forward_gate_inp: QMatMul::from_qtensor(feed_forward_gate_inp)?,
                    experts,
                }
            };
            let attention_norm =
                ct.tensor(reader, &format!("{prefix}.attn_norm.weight"), device)?;
            let ffn_norm = ct.tensor(reader, &format!("{prefix}.ffn_norm.weight"), device)?;
            let span_attn = tracing::span!(tracing::Level::TRACE, "attn");
            let span_rot = tracing::span!(tracing::Level::TRACE, "attn-rot");
            let span_mlp = tracing::span!(tracing::Level::TRACE, "attn-mlp");
            layers.push(LayerWeights {
                attention_wq: QMatMul::from_qtensor(attention_wq)?,
                attention_wk: QMatMul::from_qtensor(attention_wk)?,
                attention_wv: QMatMul::from_qtensor(attention_wv)?,
                attention_wo: QMatMul::from_qtensor(attention_wo)?,
                attention_bq,
                attention_bk,
                attention_bv,
                attention_norm: RmsNorm::from_qtensor(attention_norm, rms_norm_eps)?,
                mlp_or_moe,
                ffn_norm: RmsNorm::from_qtensor(ffn_norm, rms_norm_eps)?,
                n_head: head_count,
                n_kv_head: head_count_kv,
                // Use rope_dim as head_dim: for standard models rope_dim == head_dim,
                // and for compacted models where embedding_length / head_count is non-integer,
                // rope_dim (from metadata or defaulting to embedding_length/head_count) is correct.
                head_dim: rope_dim,
                cos: cos.clone(),
                sin: sin.clone(),
                neg_inf: neg_inf.clone(),
                kv_cache: None,
                span_attn,
                span_rot,
                span_mlp,
            })
        }
        let span = tracing::span!(tracing::Level::TRACE, "model");
        let span_output = tracing::span!(tracing::Level::TRACE, "output");
        Ok(Self {
            tok_embeddings,
            layers,
            norm,
            output: QMatMul::from_qtensor(output)?,
            masks: HashMap::new(),
            span,
            span_output,
            context_length,
        })
    }

    fn mask(&mut self, t: usize, device: &Device) -> Result<Tensor> {
        if let Some(mask) = self.masks.get(&t) {
            Ok(mask.clone())
        } else {
            let mask: Vec<_> = (0..t)
                .flat_map(|i| (0..t).map(move |j| u8::from(j > i)))
                .collect();
            let mask = Tensor::from_slice(&mask, (t, t), device)?;
            self.masks.insert(t, mask.clone());
            Ok(mask)
        }
    }

    /// Apply LoRA adapters to quantized weights via mixed-precision merge.
    ///
    /// For each LoRA weight pair (A, B), finds the matching GGUF layer weight,
    /// dequantizes it to FP32, applies W' = W + scale * (B @ A), and stores
    /// the merged weight as a non-quantized QMatMul::Tensor.
    ///
    /// This is the GGUF equivalent of `rebuild_with_stacked_lora` for safetensors.
    /// The key difference: only layers with LoRA adapters are dequantized.
    /// Layers without adapters stay quantized, preserving memory efficiency.
    ///
    /// Returns (merged_count, failed_count).
    pub fn apply_lora_adapters(
        &mut self,
        lora_weights: &HashMap<String, (Tensor, Tensor, f64)>, // layer_name → (lora_a, lora_b, scale)
    ) -> Result<(usize, usize)> {
        let mut merged = 0usize;
        let mut failed = 0usize;

        // Extract device upfront to avoid borrow conflict with self.layers
        let device = self.layers[0].cos.device().clone();

        for (lora_name, (lora_a, lora_b, scale)) in lora_weights {
            // Parse layer index and projection from LoRA name
            // LoRA names: "base_model.model.model.layers.{N}.self_attn.{q|k|v|o}_proj"
            //          or "base_model.model.model.layers.{N}.mlp.{gate|up|down}_proj"
            if let Some((layer_idx, proj)) = parse_lora_layer_name(lora_name) {
                if layer_idx >= self.layers.len() {
                    failed += 1;
                    continue;
                }
                let layer = &mut self.layers[layer_idx];
                let qmatmul = match proj {
                    Projection::Q => &mut layer.attention_wq,
                    Projection::K => &mut layer.attention_wk,
                    Projection::V => &mut layer.attention_wv,
                    Projection::O => &mut layer.attention_wo,
                    Projection::Gate => match &mut layer.mlp_or_moe {
                        MlpOrMoe::Mlp(mlp) => &mut mlp.feed_forward_w1,
                        MlpOrMoe::MoE { .. } => {
                            failed += 1;
                            continue;
                        }
                    },
                    Projection::Up => match &mut layer.mlp_or_moe {
                        MlpOrMoe::Mlp(mlp) => &mut mlp.feed_forward_w3,
                        MlpOrMoe::MoE { .. } => {
                            failed += 1;
                            continue;
                        }
                    },
                    Projection::Down => match &mut layer.mlp_or_moe {
                        MlpOrMoe::Mlp(mlp) => &mut mlp.feed_forward_w2,
                        MlpOrMoe::MoE { .. } => {
                            failed += 1;
                            continue;
                        }
                    },
                };

                match qmatmul.merge_lora(lora_a, lora_b, *scale, &device) {
                    Ok(()) => merged += 1,
                    Err(_e) => failed += 1,
                }
            } else {
                failed += 1;
            }
        }

        Ok((merged, failed))
    }

    /// Reset KV cache for all layers without reloading weights.
    pub fn clear_kv_cache(&mut self) {
        for layer in &mut self.layers {
            layer.kv_cache = None;
        }
    }

    /// Forward with optional layer limit for memory debugging.
    /// Pass max_layers=0 for full forward.
    pub fn forward_limited(
        &mut self,
        x: &Tensor,
        index_pos: usize,
        max_layers: usize,
    ) -> Result<Tensor> {
        self.forward_inner(x, index_pos, if max_layers == 0 { self.layers.len() } else { max_layers })
    }

    pub fn forward(&mut self, x: &Tensor, index_pos: usize) -> Result<Tensor> {
        self.forward_inner(x, index_pos, self.layers.len())
    }

    fn forward_inner(&mut self, x: &Tensor, index_pos: usize, max_layers: usize) -> Result<Tensor> {
        // Debug: CANDLE_MAX_LAYERS env var limits layer count for memory profiling
        let effective_max = std::env::var("CANDLE_MAX_LAYERS")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(max_layers)
            .min(max_layers);

        let (_b_sz, seq_len) = x.dims2()?;
        let mask = if seq_len == 1 {
            None
        } else {
            Some(self.mask(seq_len, x.device())?)
        };
        let _enter = self.span.enter();
        let device = x.device().clone();
        let mut layer_in = self.tok_embeddings.forward(x)?;

        // Dump embedding output for divergence debugging
        if std::env::var("CANDLE_DUMP_LAYERS").is_ok() {
            device.synchronize()?;
            if let Ok(flat) = layer_in.flatten_all().and_then(|t| t.to_vec1::<f32>()) {
                let n = flat.len().min(10);
                let first10: Vec<String> = flat[..n].iter().map(|v| format!("{:.6}", v)).collect();
                eprintln!("EMBED    shape={:?} first10=[{}]", layer_in.dims(), first10.join(", "));
            }
        }

        // Debug: if CANDLE_MAX_LAYERS=0, return embedding directly (skip all layers)
        if effective_max == 0 {
            return self.output.forward(&self.norm.forward(&layer_in)?.i((.., seq_len - 1, ..))?);
        }
        for (layer_idx, layer) in self.layers.iter_mut().enumerate() {
            if layer_idx >= effective_max {
                break;
            }
            let x = layer_in;
            let residual = &x;
            let x = layer.attention_norm.forward(&x)?;
            let attn = layer.forward_attn(&x, mask.as_ref(), index_pos)?;
            let x = (attn + residual)?;

            // MLP
            let _enter = layer.span_mlp.enter();
            let residual = &x;
            let x = layer.ffn_norm.forward(&x)?;
            let x = layer.mlp_or_moe.forward(&x)?;
            let x = (x + residual)?;
            layer_in = x;

            // Log hidden state stats per layer (CANDLE_DEBUG_LAYERS=1)
            if layer_idx % 16 == 0 || layer_idx == effective_max - 1 {
                if std::env::var("CANDLE_DEBUG_LAYERS").is_ok() {
                    device.synchronize()?;
                    if let Ok(flat) = layer_in.flatten_all().and_then(|t| t.to_vec1::<f32>()) {
                        let mean = flat.iter().sum::<f32>() / flat.len() as f32;
                        let max = flat.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                        let min = flat.iter().cloned().fold(f32::INFINITY, f32::min);
                        let has_nan = flat.iter().any(|v| v.is_nan());
                        let has_inf = flat.iter().any(|v| v.is_infinite());
                        eprintln!(
                            "  [layer {:>2}] mean={:>8.3} min={:>8.1} max={:>8.1} nan={} inf={}",
                            layer_idx, mean, min, max, has_nan, has_inf
                        );
                    }
                }
            }

            // Commit Metal command buffer every 24 layers so the GPU can start
            // executing early layers while we encode later ones.
            // Too frequent (8) = CPU waits too much. Never = giant stall at end.
            if (layer_idx + 1) % 24 == 0 {
                device.synchronize()?;
            }

            // Dump hidden state for divergence debugging
            if std::env::var("CANDLE_DUMP_LAYERS").is_ok() && (layer_idx < 3 || layer_idx == effective_max - 1) {
                device.synchronize()?;
                if let Ok(flat) = layer_in.flatten_all().and_then(|t| t.to_vec1::<f32>()) {
                    let n = flat.len().min(10);
                    let first10: Vec<String> = flat[..n].iter().map(|v| format!("{:.6}", v)).collect();
                    let mean: f64 = flat.iter().map(|&v| v as f64).sum::<f64>() / flat.len() as f64;
                    let absmax = flat.iter().cloned().fold(0f32, |a, b| a.max(b.abs()));
                    eprintln!("LAYER[{:>2}] mean={:.6} absmax={:.3} first10=[{}]",
                        layer_idx, mean, absmax, first10.join(", "));
                }
            }
        }
        let x = self.norm.forward(&layer_in)?;
        let x = x.i((.., seq_len - 1, ..))?;
        let _enter = self.span_output.enter();
        self.output.forward(&x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_lora_layer_name_peft_format() {
        // Full PEFT format: base_model.model.model.layers.N.module.proj
        let (idx, proj) =
            parse_lora_layer_name("base_model.model.model.layers.5.self_attn.q_proj").unwrap();
        assert_eq!(idx, 5);
        assert!(matches!(proj, Projection::Q));

        let (idx, proj) =
            parse_lora_layer_name("base_model.model.model.layers.0.self_attn.v_proj").unwrap();
        assert_eq!(idx, 0);
        assert!(matches!(proj, Projection::V));

        let (idx, proj) =
            parse_lora_layer_name("base_model.model.model.layers.27.mlp.gate_proj").unwrap();
        assert_eq!(idx, 27);
        assert!(matches!(proj, Projection::Gate));
    }

    #[test]
    fn test_parse_lora_layer_name_short_format() {
        // Short format: model.layers.N.module.proj
        let (idx, proj) = parse_lora_layer_name("model.layers.3.self_attn.o_proj").unwrap();
        assert_eq!(idx, 3);
        assert!(matches!(proj, Projection::O));

        let (idx, proj) = parse_lora_layer_name("model.layers.10.mlp.up_proj").unwrap();
        assert_eq!(idx, 10);
        assert!(matches!(proj, Projection::Up));

        let (idx, proj) = parse_lora_layer_name("model.layers.0.mlp.down_proj").unwrap();
        assert_eq!(idx, 0);
        assert!(matches!(proj, Projection::Down));
    }

    #[test]
    fn test_parse_lora_layer_name_bare_format() {
        // Bare format: layers.N.module.proj (no model. prefix)
        let (idx, proj) = parse_lora_layer_name("layers.7.self_attn.k_proj").unwrap();
        assert_eq!(idx, 7);
        assert!(matches!(proj, Projection::K));
    }

    #[test]
    fn test_parse_lora_layer_name_invalid() {
        assert!(parse_lora_layer_name("some.random.name").is_none());
        assert!(parse_lora_layer_name("").is_none());
        assert!(parse_lora_layer_name("layers.not_a_number.self_attn.q_proj").is_none());
        assert!(parse_lora_layer_name("layers.0.self_attn.unknown_proj").is_none());
    }

    #[test]
    fn test_parse_all_seven_projections() {
        let projections = [
            ("self_attn.q_proj", "Q"),
            ("self_attn.k_proj", "K"),
            ("self_attn.v_proj", "V"),
            ("self_attn.o_proj", "O"),
            ("mlp.gate_proj", "Gate"),
            ("mlp.up_proj", "Up"),
            ("mlp.down_proj", "Down"),
        ];

        for (proj_str, expected) in projections {
            let name = format!("layers.0.{}", proj_str);
            let result = parse_lora_layer_name(&name);
            assert!(result.is_some(), "Failed to parse: {}", name);
            let (idx, proj) = result.unwrap();
            assert_eq!(idx, 0);
            let proj_name = format!("{:?}", proj);
            assert_eq!(proj_name, expected, "Wrong projection for {}", name);
        }
    }
}
