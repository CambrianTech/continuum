//! The building blocks a layer is made of: norm, embedding, matmul (with LoRA merge), MLP and MoE.


use candle_core::quantized::QTensor;
use candle_core::quantized::gguf_file;
use candle_core::{DType, Device, Result, Tensor};
use candle_nn::Module;

/// RmsNorm wrapper — delegates to candle_nn::ops::rms_norm.
/// Stores dequantized weight and eps for use with quantized model loading.
#[derive(Debug, Clone)]
pub struct RmsNorm {
    pub(super) weight: Tensor,
    pub(super) eps: f64,
}

impl RmsNorm {
    pub fn from_qtensor(qtensor: QTensor, eps: f64) -> Result<Self> {
        let weight = qtensor.dequantize(&qtensor.device())?;
        Ok(Self { weight, eps })
    }
}

impl Module for RmsNorm {
    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        candle_nn::ops::rms_norm(x, &self.weight, self.eps as f32)
    }
}

/// Zero-overhead quantized embedding lookup via one-hot matmul.
///
/// Embedding: F16 table on the same device as the model (Metal).
/// 1.55 GB in Metal shared memory. Forward: Metal index_select → F32 cast.
/// Everything stays on GPU. No CPU↔Metal copies.
#[derive(Debug, Clone)]
pub(super) struct DeviceEmbedding {
    /// F16 embedding table: [vocab_size, hidden_size] on model device
    pub(super) table: Tensor,
    pub(super) hidden_size: usize,
}

impl DeviceEmbedding {
    pub(super) fn from_gguf<R: std::io::Seek + std::io::Read>(
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

    pub(super) fn forward(&self, token_ids: &Tensor) -> Result<Tensor> {
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
pub(super) struct QMatMul {
    pub(super) inner: candle_core::quantized::QMatMul,
    pub(super) span: tracing::Span,
}

impl QMatMul {
    pub(super) fn from_qtensor(qtensor: QTensor) -> Result<Self> {
        let inner = candle_core::quantized::QMatMul::from_qtensor(qtensor)?;
        let span = tracing::span!(tracing::Level::TRACE, "qmatmul");
        Ok(Self { inner, span })
    }

    pub(super) fn forward(&self, xs: &Tensor) -> Result<Tensor> {
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
    pub(super) fn merge_lora(
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
pub(super) struct Mlp {
    pub(super) feed_forward_w1: QMatMul,
    pub(super) feed_forward_w2: QMatMul,
    pub(super) feed_forward_w3: QMatMul,
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
pub(super) enum MlpOrMoe {
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
