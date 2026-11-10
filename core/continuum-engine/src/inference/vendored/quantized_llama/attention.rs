//! One transformer layer: attention with RoPE dispatch and its KV cache, then the MLP.


use candle_core::{IndexOp, Result, Tensor};

use super::weights::{MlpOrMoe, QMatMul, RmsNorm};

#[derive(Debug, Clone)]
pub(super) struct LayerWeights {
    pub(super) attention_wq: QMatMul,
    pub(super) attention_wk: QMatMul,
    pub(super) attention_wv: QMatMul,
    pub(super) attention_wo: QMatMul,
    /// Optional Q/K/V biases (Qwen2 has bias on Q, K, V; Llama does not).
    pub(super) attention_bq: Option<Tensor>,
    pub(super) attention_bk: Option<Tensor>,
    pub(super) attention_bv: Option<Tensor>,
    pub(super) attention_norm: RmsNorm,
    pub(super) mlp_or_moe: MlpOrMoe,
    pub(super) ffn_norm: RmsNorm,
    pub(super) n_head: usize,
    pub(super) n_kv_head: usize,
    pub(super) head_dim: usize,
    /// RoPE type: true = NEOX (non-interleaved, pairs i with i+d/2),
    /// false = NORM (interleaved, pairs 2i with 2i+1).
    /// Qwen2 uses NEOX (type 2), standard Llama uses NORM (type 0).
    pub(super) rope_is_neox: bool,
    pub(super) cos: Tensor,
    pub(super) sin: Tensor,
    pub(super) neg_inf: Tensor,
    pub(super) kv_cache: Option<(Tensor, Tensor)>,
    pub(super) span_attn: tracing::Span,
    pub(super) span_rot: tracing::Span,
    pub(super) span_mlp: tracing::Span,
}

pub(super) fn masked_fill(on_false: &Tensor, mask: &Tensor, on_true: &Tensor) -> Result<Tensor> {
    let shape = mask.shape();
    let m = mask.where_cond(&on_true.broadcast_as(shape.dims())?, on_false)?;
    Ok(m)
}

impl LayerWeights {
    pub(super) fn apply_rotary_emb(&self, x: &Tensor, index_pos: usize) -> Result<Tensor> {
        let _enter = self.span_rot.enter();
        let (_b_sz, _n_head, seq_len, _n_embd) = x.dims4()?;
        let cos = self.cos.narrow(0, index_pos, seq_len)?;
        let sin = self.sin.narrow(0, index_pos, seq_len)?;
        if self.rope_is_neox {
            // GPT-NeoX style: pairs (i, i+d/2). Used by Qwen2, CodeLlama, etc.
            candle_nn::rotary_emb::rope(&x.contiguous()?, &cos, &sin)
        } else {
            // Interleaved style: pairs (2i, 2i+1). Used by standard Llama.
            candle_nn::rotary_emb::rope_i(&x.contiguous()?, &cos, &sin)
        }
    }

    pub(super) fn forward_attn(
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

        let dump_attn = std::env::var("CANDLE_DUMP_ATTN").is_ok() && index_pos == 0;

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

        if dump_attn {
            x.device().synchronize().ok();
            let last = seq_len - 1;
            // Q after bias (before reshape/RoPE) — matches llama.cpp "Qcur-0" first dump
            if let Ok(vals) = q.i((0, last, ..)).and_then(|t| t.to_vec1::<f32>()) {
                let first: Vec<String> = vals[..5.min(vals.len())]
                    .iter()
                    .map(|v| format!("{:.6}", v))
                    .collect();
                eprintln!(
                    "Q+bias (flat): {} dims, first5=[{}]",
                    vals.len(),
                    first.join(", ")
                );
            }
            if let Ok(vals) = k.i((0, last, ..)).and_then(|t| t.to_vec1::<f32>()) {
                let first: Vec<String> = vals[..5.min(vals.len())]
                    .iter()
                    .map(|v| format!("{:.6}", v))
                    .collect();
                eprintln!(
                    "K+bias (flat): {} dims, first5=[{}]",
                    vals.len(),
                    first.join(", ")
                );
            }
            if let Ok(vals) = v.i((0, last, ..)).and_then(|t| t.to_vec1::<f32>()) {
                let first: Vec<String> = vals[..5.min(vals.len())]
                    .iter()
                    .map(|v| format!("{:.6}", v))
                    .collect();
                eprintln!(
                    "V+bias (flat): {} dims, first5=[{}]",
                    vals.len(),
                    first.join(", ")
                );
            }
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

        if dump_attn {
            x.device().synchronize().ok();
            // Q after RoPE: shape [b, n_head, seq, head_dim]
            // llama.cpp "Qcur-0" after RoPE: shape [128, 25, 187, 1]
            // Compare last head's last position
            let last = seq_len - 1;
            let n_head = self.n_head;
            if let Ok(vals) = q
                .i((0, n_head - 1, last, ..))
                .and_then(|t| t.to_vec1::<f32>())
            {
                let first: Vec<String> = vals[..5.min(vals.len())]
                    .iter()
                    .map(|v| format!("{:.6}", v))
                    .collect();
                eprintln!(
                    "Q after RoPE (head {}, last tok): first5=[{}]",
                    n_head - 1,
                    first.join(", ")
                );
            }
            // Q head 0 last tok
            if let Ok(vals) = q.i((0, 0, last, ..)).and_then(|t| t.to_vec1::<f32>()) {
                let first: Vec<String> = vals[..5.min(vals.len())]
                    .iter()
                    .map(|v| format!("{:.6}", v))
                    .collect();
                eprintln!(
                    "Q after RoPE (head 0, last tok): first5=[{}]",
                    first.join(", ")
                );
            }
            if let Ok(vals) = k.i((0, 0, last, ..)).and_then(|t| t.to_vec1::<f32>()) {
                let first: Vec<String> = vals[..5.min(vals.len())]
                    .iter()
                    .map(|v| format!("{:.6}", v))
                    .collect();
                eprintln!(
                    "K after RoPE (head 0, last tok): first5=[{}]",
                    first.join(", ")
                );
            }
        }

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
        if dump_attn {
            x.device().synchronize().ok();
            let last = seq_len - 1;
            // Attention output before reshape (shape: [b, n_head, seq, head_dim])
            // llama.cpp "__fattn__-0" last head
            if let Ok(vals) = y.i((0, 0, last, ..)).and_then(|t| t.to_vec1::<f32>()) {
                let first: Vec<String> = vals[..5.min(vals.len())]
                    .iter()
                    .map(|v| format!("{:.6}", v))
                    .collect();
                eprintln!("attn_out (head 0, last tok): first5=[{}]", first.join(", "));
            }
        }

        let y = y
            .transpose(1, 2)?
            .reshape(&[b_sz, seq_len, self.n_head * self.head_dim])?;

        if dump_attn {
            x.device().synchronize().ok();
            let last = seq_len - 1;
            // kqv_out: reshaped attention output before Wo
            if let Ok(vals) = y.i((0, last, ..)).and_then(|t| t.to_vec1::<f32>()) {
                let first: Vec<String> = vals[..5.min(vals.len())]
                    .iter()
                    .map(|v| format!("{:.6}", v))
                    .collect();
                eprintln!(
                    "kqv_out (flat, last tok): {} dims, first5=[{}]",
                    vals.len(),
                    first.join(", ")
                );
                let data: Vec<u8> = vals.iter().flat_map(|v| v.to_le_bytes()).collect();
                std::fs::write("/tmp/candle_l0_kqv_out.bin", &data).ok();
            }
        }

        let y = self.attention_wo.forward(&y)?;

        if dump_attn {
            x.device().synchronize().ok();
            let last = seq_len - 1;
            if let Ok(vals) = y.i((0, last, ..)).and_then(|t| t.to_vec1::<f32>()) {
                let first: Vec<String> = vals[..5.min(vals.len())]
                    .iter()
                    .map(|v| format!("{:.6}", v))
                    .collect();
                eprintln!(
                    "attn_wo (Wo output, last tok): {} dims, first5=[{}]",
                    vals.len(),
                    first.join(", ")
                );
                let data: Vec<u8> = vals.iter().flat_map(|v| v.to_le_bytes()).collect();
                std::fs::write("/tmp/candle_l0_attn_wo.bin", &data).ok();
            }
        }

        Ok(y)
    }
}
