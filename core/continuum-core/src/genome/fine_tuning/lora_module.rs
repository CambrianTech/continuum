//! [`LoRAModule`] — substrate-side LoRA layer for training.
//!
//! Sibling to `inference/lora.rs` (which handles the post-training
//! load + merge path). This module is the train-time half: a
//! trainable A/B pair wrapping a frozen base weight, with a forward
//! pass that participates in Candle's autograd so gradients flow
//! into A + B but NOT into the base weight.
//!
//! ## Math
//!
//! Standard LoRA, matching the paper (Hu et al. 2021) and the
//! existing inference-side merge math in `inference/lora.rs`:
//!
//! ```text
//! y = x · W^T + (alpha/rank) · (x · A^T) · B^T
//! ```
//!
//! Where:
//! - `W` is the frozen base weight, shape `[out_features, in_features]`
//! - `A` is the trainable down-projection, shape `[rank, in_features]`
//! - `B` is the trainable up-projection, shape `[out_features, rank]`
//! - `alpha/rank` is the LoRA scaling factor
//!
//! The shapes match what [`crate::inference::lora::merge_lora_weight`]
//! expects on the merge side. After training completes, the
//! `safetensors` writeout produces `<base>.lora_A.weight` +
//! `<base>.lora_B.weight` tensors that the inference path's
//! `load_lora_adapter` reads back unchanged.
//!
//! ## Init policy
//!
//! Per the LoRA paper section 4.1 + the Microsoft reference
//! implementation:
//! - `A` ~ Kaiming uniform with `a = √5` (matches PyTorch's
//!   `kaiming_uniform_` default)
//! - `B` = zeros
//!
//! This makes the initial delta `(B @ A) = 0`, so the model behaves
//! IDENTICALLY to the frozen base at step 0. Training perturbs A + B
//! away from this fixed point. Any other init policy (e.g. B random)
//! would cause the initial forward to differ from the base model —
//! the paper showed this hurts convergence.
//!
//! ## Why a struct + `forward`, not a `Module` trait impl
//!
//! `candle_nn::Module` requires a `&self` forward. LoRA training
//! REUSES the same module across batches with the SAME A/B
//! parameters (the parameters update OUT OF BAND via the optimizer
//! step on the `VarMap`). So `&self` is correct; we don't need to
//! re-construct the module per step.
//!
//! We do NOT implement `Module` for now because Module's signature
//! `forward(&self, &Tensor) -> Result<Tensor>` doesn't carry
//! dropout state (training vs eval). #232 adds the optimizer +
//! training loop and at that point a `forward_train` vs
//! `forward_eval` distinction is cleaner than a stateful Module.
//!
//! ## Doctrinal alignment
//!
//! - `[[matrix-dojo-layer-loading-as-substrate-primitive]]`: the
//!   output of training here IS the artifact that genome paging
//!   loads back in. Same tensor layout, same naming convention,
//!   no glue layer between train and inference.
//! - `[[no-fallbacks-ever]]`: every construction failure (bad
//!   rank, dimension mismatch) returns a typed `LoRAError`. No
//!   silent shape mismatches that surface as opaque tensor errors
//!   during the first forward pass.

use candle_core::{DType, Device, Tensor, Var};

/// Why a [`LoRAModule`] couldn't be constructed. Typed so the
/// trainer can branch on the specific failure shape rather than
/// inspecting Candle's generic tensor-shape errors.
#[derive(Debug, thiserror::Error)]
pub enum LoRAError {
    /// `rank == 0` makes the A/B matrices zero-sized, which means
    /// no capacity to learn anything. The LoRA paper recommends
    /// rank ∈ {4, 8, 16, 32, 64}; we don't constrain to those
    /// values, but `rank == 0` is rejected.
    #[error("LoRA rank must be > 0, got {0}")]
    InvalidRank(u32),

    /// `alpha == 0` makes the scale factor zero, which would zero
    /// out the LoRA delta entirely — the base model would behave
    /// as if no adapter were attached. The paper's heuristic is
    /// `alpha = rank * 2`; we don't enforce that but `alpha == 0`
    /// is rejected.
    #[error("LoRA alpha must be > 0, got {0}")]
    InvalidAlpha(u32),

    /// The frozen base weight's shape doesn't match the
    /// `[out_features, in_features]` contract.
    #[error("base_weight expected 2-D [out_features, in_features], got shape {actual:?}")]
    BaseWeightShape { actual: Vec<usize> },

    /// Tensor creation failed at the Candle level (device OOM,
    /// dtype mismatch). Wraps the underlying error.
    #[error("candle tensor op: {0}")]
    Candle(#[from] candle_core::Error),
}

/// One trainable LoRA layer wrapping a frozen base linear weight.
///
/// Holds the base weight as a plain [`Tensor`] (no autograd
/// participation — its gradient is never computed) and the
/// down-projection [`Var`] `A` + up-projection [`Var`] `B`.
/// During the forward pass, A and B's gradients are tracked by
/// Candle's autograd; the optimizer step (in #232) consults
/// these gradients and updates the underlying tensors.
pub struct LoRAModule {
    /// Frozen base weight. Shape `[out_features, in_features]`.
    base_weight: Tensor,
    /// Trainable down-projection. Shape `[rank, in_features]`.
    lora_a: Var,
    /// Trainable up-projection. Shape `[out_features, rank]`.
    lora_b: Var,
    /// Scaling factor `alpha / rank` precomputed at construction so
    /// the hot path of forward() doesn't recompute it.
    scale: f64,
    /// Echo for telemetry + safetensors metadata. Not used in math.
    rank: u32,
    /// Echo for telemetry + safetensors metadata. Not used in math.
    alpha: u32,
}

impl LoRAModule {
    /// Construct with paper-spec init: A ~ Kaiming uniform, B = 0.
    ///
    /// `base_weight` is moved in. The caller has loaded it from
    /// safetensors / from a quantized model and is handing
    /// ownership to this module; subsequent inference + LoRA-merge
    /// paths use the SAME tensor instance (it's frozen, sharing is
    /// safe).
    pub fn new(
        base_weight: Tensor,
        rank: u32,
        alpha: u32,
        dtype: DType,
        device: &Device,
    ) -> Result<Self, LoRAError> {
        if rank == 0 {
            return Err(LoRAError::InvalidRank(rank));
        }
        if alpha == 0 {
            return Err(LoRAError::InvalidAlpha(alpha));
        }

        let dims = base_weight.dims();
        if dims.len() != 2 {
            return Err(LoRAError::BaseWeightShape {
                actual: dims.to_vec(),
            });
        }
        let out_features = dims[0];
        let in_features = dims[1];

        // Kaiming uniform with a = √5 matches PyTorch's
        // `kaiming_uniform_` default, which is what the LoRA
        // reference implementation uses for A init.
        //
        // bound = √(6 / ((1 + 5) · fan_in)) = √(1 / fan_in)
        // where fan_in = in_features for A.
        let bound = (1.0 / in_features as f64).sqrt();
        let lora_a_init =
            Tensor::rand(-bound, bound, (rank as usize, in_features), device)?.to_dtype(dtype)?;
        let lora_a = Var::from_tensor(&lora_a_init)?;

        // B is initialized to zeros so the initial delta (B @ A) is
        // zero — the model behaves identically to the frozen base
        // at step 0. Training perturbs B + A away from this fixed
        // point.
        let lora_b_init = Tensor::zeros((out_features, rank as usize), dtype, device)?;
        let lora_b = Var::from_tensor(&lora_b_init)?;

        let scale = alpha as f64 / rank as f64;

        Ok(Self {
            base_weight,
            lora_a,
            lora_b,
            scale,
            rank,
            alpha,
        })
    }

    /// Forward pass: `y = x · W^T + scale · (x · A^T) · B^T`.
    ///
    /// Input `x` shape: `[..., in_features]` (any leading batch /
    /// sequence dims). Output shape: `[..., out_features]`.
    ///
    /// `x · W^T` is the standard linear forward — the base model
    /// term. `scale · (x · A^T) · B^T` is the LoRA delta — the only
    /// term gradients flow into during backward.
    pub fn forward(&self, x: &Tensor) -> Result<Tensor, candle_core::Error> {
        // Base linear: y_base = x · W^T.
        // base_weight is [out_features, in_features]; transpose is
        // [in_features, out_features]; matmul of [..., in_features]
        // with [in_features, out_features] gives [..., out_features].
        let base = x.broadcast_matmul(&self.base_weight.t()?)?;

        // LoRA delta path:
        //   step 1: x · A^T   shape [..., in_features] · [in_features, rank] -> [..., rank]
        //   step 2: result · B^T   shape [..., rank] · [rank, out_features] -> [..., out_features]
        // Use the `Var`'s `.as_tensor()` to participate in autograd.
        let lora_a_t = self.lora_a.as_tensor().t()?;
        let lora_b_t = self.lora_b.as_tensor().t()?;
        let down = x.broadcast_matmul(&lora_a_t)?;
        let up = down.broadcast_matmul(&lora_b_t)?;
        let scaled = (up * self.scale)?;

        base + scaled
    }

    /// Borrow the trainable A parameter. Used by the optimizer
    /// (#232) to collect Vars into a `VarMap` for gradient
    /// updates, and by the safetensors writer (#233) to read the
    /// final tensor for serialization.
    pub fn lora_a(&self) -> &Var {
        &self.lora_a
    }

    /// Borrow the trainable B parameter. Same usage as `lora_a`.
    pub fn lora_b(&self) -> &Var {
        &self.lora_b
    }

    /// Configured LoRA rank. Echo for telemetry.
    pub fn rank(&self) -> u32 {
        self.rank
    }

    /// Configured LoRA alpha. Echo for telemetry.
    pub fn alpha(&self) -> u32 {
        self.alpha
    }

    /// Configured scale factor (`alpha / rank`). Same value used in
    /// the merge math on the inference side.
    pub fn scale(&self) -> f64 {
        self.scale
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use candle_core::Device;

    fn cpu() -> Device {
        Device::Cpu
    }

    fn base_weight(out_features: usize, in_features: usize) -> Tensor {
        // Deterministic base: all 0.1s. Specific value doesn't
        // matter for the construction tests; what matters is the
        // shape contract.
        Tensor::full(0.1f32, (out_features, in_features), &cpu()).unwrap()
    }

    // what this catches: rank=0 must be rejected. The math would
    // produce zero-sized A/B matrices which Candle would accept,
    // but training would never learn anything. Better to fail
    // construction than silently produce a no-op module.
    #[test]
    fn rank_zero_rejected() {
        let w = base_weight(8, 4);
        let err = LoRAModule::new(w, 0, 8, DType::F32, &cpu())
            .err()
            .expect("must reject");
        assert!(matches!(err, LoRAError::InvalidRank(0)));
    }

    // what this catches: alpha=0 must be rejected. Scale would be
    // 0, so the LoRA delta would be zeroed in every forward — the
    // adapter would never affect output and training would never
    // converge. Fail loudly at construction.
    #[test]
    fn alpha_zero_rejected() {
        let w = base_weight(8, 4);
        let err = LoRAModule::new(w, 8, 0, DType::F32, &cpu())
            .err()
            .expect("must reject");
        assert!(matches!(err, LoRAError::InvalidAlpha(0)));
    }

    // what this catches: 1-D or 3-D base weight must be rejected
    // with a typed shape error. Without this, a caller passing a
    // bias vector (1-D) or a multi-head fused weight (3-D) would
    // get an opaque Candle matmul error from the first forward.
    #[test]
    fn non_2d_base_weight_rejected() {
        let one_d = Tensor::full(0.1f32, 16, &cpu()).unwrap();
        let err = LoRAModule::new(one_d, 4, 8, DType::F32, &cpu())
            .err()
            .expect("must reject");
        assert!(matches!(err, LoRAError::BaseWeightShape { .. }));
    }

    // what this catches: B is initialized to ALL zeros so the
    // initial LoRA delta is zero. The paper requires this — without
    // it, the model behaves differently from the frozen base at
    // step 0 and convergence suffers. A future "improvement" that
    // initializes B with noise would silently regress training.
    #[test]
    fn b_is_zero_initialized_so_initial_delta_is_zero() {
        let w = base_weight(8, 4);
        let module = LoRAModule::new(w.clone(), 4, 8, DType::F32, &cpu()).unwrap();
        let b = module.lora_b().as_tensor();
        // All B entries must be exactly zero at init.
        let max_abs = b
            .abs()
            .unwrap()
            .max(0)
            .unwrap()
            .max(0)
            .unwrap()
            .to_scalar::<f32>()
            .unwrap();
        assert_eq!(
            max_abs, 0.0,
            "B must be all zeros at init, got max |B| = {max_abs}"
        );

        // And as a consequence, the LoRA delta (B @ A) is exactly
        // zero — the initial forward equals the base forward.
        let x = Tensor::full(1.0f32, (2, 4), &cpu()).unwrap();
        let lora_out = module.forward(&x).unwrap();
        let base_out = x.broadcast_matmul(&w.t().unwrap()).unwrap();
        let diff = (lora_out - base_out)
            .unwrap()
            .abs()
            .unwrap()
            .max(0)
            .unwrap()
            .max(0)
            .unwrap()
            .to_scalar::<f32>()
            .unwrap();
        assert!(
            diff < 1e-6,
            "LoRA output at init must equal base output, got max diff {diff}"
        );
    }

    // what this catches: A is initialized with non-zero values
    // (Kaiming uniform). Without trainable signal in A, only B
    // would receive gradients during backward — and B's gradient
    // is `dL/dB = (dL/dy) · scale · (x · A^T)`. If A were zero, B's
    // gradient would also be zero, and training would be stuck.
    #[test]
    fn a_init_is_non_trivial() {
        let w = base_weight(8, 4);
        let module = LoRAModule::new(w, 16, 32, DType::F32, &cpu()).unwrap();
        let a = module.lora_a().as_tensor();
        let max_abs = a
            .abs()
            .unwrap()
            .max(0)
            .unwrap()
            .max(0)
            .unwrap()
            .to_scalar::<f32>()
            .unwrap();
        assert!(
            max_abs > 0.0,
            "A must have non-zero init for gradient signal to flow during training, got max |A| = 0"
        );
    }

    // what this catches: forward output shape matches the contract.
    // Input [batch, seq, in_features] -> output [batch, seq,
    // out_features]. A future refactor that swaps the in/out axes
    // would silently produce wrong-shaped activations that crash
    // downstream layers.
    #[test]
    fn forward_preserves_leading_dims_and_maps_in_to_out() {
        let in_features = 8;
        let out_features = 16;
        let w = base_weight(out_features, in_features);
        let module = LoRAModule::new(w, 4, 8, DType::F32, &cpu()).unwrap();
        let x = Tensor::full(0.5f32, (3, 5, in_features), &cpu()).unwrap();
        let y = module.forward(&x).unwrap();
        assert_eq!(y.dims(), &[3, 5, out_features]);
    }

    // what this catches: the scale factor matches the inference-
    // side merge math. `inference/lora.rs::merge_lora_weight`
    // multiplies by `lora.scale` which is `alpha / rank`. If the
    // train side used a different convention (e.g. alpha alone, or
    // a sqrt scaling), the layers produced here would behave
    // differently when loaded for inference — silent regression.
    #[test]
    fn scale_factor_matches_inference_side_convention() {
        let w = base_weight(8, 4);
        let module = LoRAModule::new(w, 8, 16, DType::F32, &cpu()).unwrap();
        assert_eq!(module.scale(), 16.0 / 8.0);

        let module = LoRAModule::new(base_weight(8, 4), 4, 8, DType::F32, &cpu()).unwrap();
        assert_eq!(module.scale(), 2.0);
    }

    // what this catches: when A and B are forced to non-zero values
    // post-construction (simulating after-training state), the LoRA
    // delta becomes non-zero and the forward output diverges from
    // base. This is the integration test for the math:
    //   y = base + scale · (x · A^T) · B^T
    #[test]
    fn forward_with_perturbed_ab_diverges_from_base_by_scaled_delta() {
        let in_features = 2;
        let out_features = 2;
        let rank = 1;
        let alpha = 2;
        // base_weight = identity-ish: [[1, 0], [0, 1]]
        let base = Tensor::from_slice(
            &[1.0f32, 0.0, 0.0, 1.0],
            (out_features, in_features),
            &cpu(),
        )
        .unwrap();
        let module = LoRAModule::new(base.clone(), rank, alpha, DType::F32, &cpu()).unwrap();

        // Force A and B to known values:
        // A = [[1, 0]]              shape [rank=1, in_features=2]
        // B = [[1], [0]]            shape [out_features=2, rank=1]
        // Expected delta = scale * (B @ A) = 2 * [[1, 0], [0, 0]]
        //                                  = [[2, 0], [0, 0]]
        let a = Tensor::from_slice(&[1.0f32, 0.0], (rank as usize, in_features), &cpu()).unwrap();
        let b = Tensor::from_slice(&[1.0f32, 0.0], (out_features, rank as usize), &cpu()).unwrap();
        module.lora_a().set(&a).unwrap();
        module.lora_b().set(&b).unwrap();

        // Input x = [[1, 1]]
        let x = Tensor::from_slice(&[1.0f32, 1.0], (1, in_features), &cpu()).unwrap();
        let y = module.forward(&x).unwrap();

        // Expected: y = x · W^T + scale · (x · A^T) · B^T
        //              = [[1, 1]]·[[1,0],[0,1]] + 2·[[1, 1]]·[[1],[0]]·[[1],[0]]^T
        //              = [[1, 1]]                  + 2·[1]·[[1, 0]]
        //              = [[1, 1]] + [[2, 0]]
        //              = [[3, 1]]
        let values: Vec<f32> = y.flatten_all().unwrap().to_vec1().unwrap();
        assert!(
            (values[0] - 3.0).abs() < 1e-5,
            "y[0,0] expected 3.0, got {}",
            values[0]
        );
        assert!(
            (values[1] - 1.0).abs() < 1e-5,
            "y[0,1] expected 1.0, got {}",
            values[1]
        );
    }

    /// VDD — validation-driven tests verifying numerical correctness
    /// against an independent closed-form computation.
    ///
    /// Difference vs the TDD tests above: TDD pins CONTRACTS
    /// (shape, error variants, init invariants). VDD pins NUMERICAL
    /// ACCURACY — the values produced match the LoRA paper's formula
    /// for arbitrary inputs, not just the worked-out 2x2 identity
    /// case. A refactor of the forward path that preserved the
    /// contract but flipped a transpose would pass TDD silently and
    /// fail here loudly.
    mod vdd {
        use super::*;

        /// Closed-form LoRA forward computed via hand-rolled nested
        /// loops over `Vec<f32>`. Independent from candle's matmul
        /// path. Output: `[batch * out_features]` row-major.
        ///
        /// `x`: `[batch * in_features]` row-major
        /// `w`: `[out_features * in_features]` row-major (base)
        /// `a`: `[rank * in_features]` row-major
        /// `b`: `[out_features * rank]` row-major
        fn closed_form_lora_forward(
            x: &[f32],
            batch: usize,
            in_features: usize,
            w: &[f32],
            out_features: usize,
            a: &[f32],
            rank: usize,
            b: &[f32],
            scale: f32,
        ) -> Vec<f32> {
            let mut y = vec![0f32; batch * out_features];

            // Base: y_base[b, o] = sum_i x[b, i] * w[o, i]
            for bi in 0..batch {
                for o in 0..out_features {
                    let mut s = 0f32;
                    for i in 0..in_features {
                        s += x[bi * in_features + i] * w[o * in_features + i];
                    }
                    y[bi * out_features + o] = s;
                }
            }

            // Delta path:
            //   d[b, r] = sum_i x[b, i] * a[r, i]
            //   z[b, o] = sum_r d[b, r] * b[o, r]
            //   y[b, o] += scale * z[b, o]
            let mut d = vec![0f32; batch * rank];
            for bi in 0..batch {
                for r in 0..rank {
                    let mut s = 0f32;
                    for i in 0..in_features {
                        s += x[bi * in_features + i] * a[r * in_features + i];
                    }
                    d[bi * rank + r] = s;
                }
            }
            for bi in 0..batch {
                for o in 0..out_features {
                    let mut s = 0f32;
                    for r in 0..rank {
                        s += d[bi * rank + r] * b[o * rank + r];
                    }
                    y[bi * out_features + o] += scale * s;
                }
            }
            y
        }

        // what this VDD catches: forward() output matches the
        // closed-form LoRA formula
        //   y = x @ W^T + (alpha/rank) * (x @ A^T) @ B^T
        // for arbitrary input + non-trivial weight values. A future
        // refactor that flipped a transpose (e.g. used `A` instead of
        // `A^T`) would still produce valid shapes — TDD would pass —
        // but every value would be wrong. This test catches that
        // class of regression by comparing against an independent
        // hand-rolled implementation, not against another candle
        // matmul.
        #[test]
        fn forward_matches_closed_form_for_arbitrary_inputs() {
            let device = Device::Cpu;
            let in_features = 5;
            let out_features = 3;
            let rank = 2;
            let alpha = 4;

            // Non-trivial base, A, B — pattern values so a transpose
            // bug shows up as a definite mismatch.
            let w_vec: Vec<f32> = (0..(out_features * in_features))
                .map(|i| (i as f32) * 0.1 - 0.7)
                .collect();
            let base = Tensor::from_slice(&w_vec, (out_features, in_features), &device).unwrap();

            let module = LoRAModule::new(base, rank, alpha, DType::F32, &device).unwrap();

            // Force A and B to known non-trivial patterns. B at init
            // is zeros; overriding makes the delta path contribute.
            let a_vec: Vec<f32> = (0..(rank as usize * in_features))
                .map(|i| ((i as f32 * 0.13) - 0.4).sin())
                .collect();
            let b_vec: Vec<f32> = (0..(out_features * rank as usize))
                .map(|i| ((i as f32 * 0.27) + 0.1).cos() * 0.5)
                .collect();
            let a_t = Tensor::from_slice(&a_vec, (rank as usize, in_features), &device).unwrap();
            let b_t = Tensor::from_slice(&b_vec, (out_features, rank as usize), &device).unwrap();
            module.lora_a().set(&a_t).unwrap();
            module.lora_b().set(&b_t).unwrap();

            // Batch of 3 distinct inputs.
            let batch = 3;
            let x_vec: Vec<f32> = (0..(batch * in_features))
                .map(|i| ((i as f32 * 0.31) - 1.0).cos())
                .collect();
            let x = Tensor::from_slice(&x_vec, (batch, in_features), &device).unwrap();

            // Reference: closed-form computation.
            let scale = alpha as f32 / rank as f32;
            let expected = closed_form_lora_forward(
                &x_vec,
                batch,
                in_features,
                &w_vec,
                out_features,
                &a_vec,
                rank as usize,
                &b_vec,
                scale,
            );

            // Actual: candle forward.
            let y = module.forward(&x).unwrap();
            let actual: Vec<f32> = y.flatten_all().unwrap().to_vec1().unwrap();

            assert_eq!(actual.len(), expected.len(), "shape contract");
            for (idx, (a_val, e_val)) in actual.iter().zip(expected.iter()).enumerate() {
                let diff = (a_val - e_val).abs();
                let bi = idx / out_features;
                let oi = idx % out_features;
                assert!(
                    diff < 5e-5,
                    "VDD mismatch at [batch={bi}, out={oi}]: \
                     actual={a_val:.6}, expected={e_val:.6}, diff={diff:.2e}"
                );
            }
        }

        // what this VDD catches: scale = alpha / rank is applied
        // exactly ONCE in the delta path, not zero times (LoRA
        // wouldn't affect output) and not twice (delta would be
        // scale²-amplified, breaking convergence). The test sets A
        // and B to specific values where the delta term, without
        // scale, would equal exactly [1, 1] for every row — then
        // verifies the candle-computed output is base + scale*[1, 1].
        // A refactor that double-applied scale would here produce
        // base + scale² * [1, 1] and the test catches it.
        #[test]
        fn scale_is_applied_exactly_once_in_delta() {
            let device = Device::Cpu;
            let in_features = 2;
            let out_features = 2;
            let rank = 1;
            let alpha = 8;
            let scale = alpha as f32 / rank as f32;

            // Zero base — so output equals exactly the scaled delta.
            let base = Tensor::zeros((out_features, in_features), DType::F32, &device).unwrap();
            let module = LoRAModule::new(base, rank, alpha, DType::F32, &device).unwrap();

            // A = [[1, 1]] (rank=1, in=2)
            // B = [[1], [1]] (out=2, rank=1)
            // For x = [[1, 0]]:
            //   x · A^T = [1*1 + 0*1] = [1] (batch=1, rank=1)
            //   (x · A^T) · B^T = [1] · [1, 1] = [1, 1]
            //   delta = scale * [1, 1] = [scale, scale]
            let a =
                Tensor::from_slice(&[1.0f32, 1.0], (rank as usize, in_features), &device).unwrap();
            let b =
                Tensor::from_slice(&[1.0f32, 1.0], (out_features, rank as usize), &device).unwrap();
            module.lora_a().set(&a).unwrap();
            module.lora_b().set(&b).unwrap();

            let x = Tensor::from_slice(&[1.0f32, 0.0], (1, in_features), &device).unwrap();
            let y = module.forward(&x).unwrap();
            let values: Vec<f32> = y.flatten_all().unwrap().to_vec1().unwrap();

            for (idx, v) in values.iter().enumerate() {
                let diff = (v - scale).abs();
                assert!(
                    diff < 1e-5,
                    "VDD: output [{idx}] = {v}, expected scale = {scale} (alpha={alpha}/rank={rank})"
                );
            }
        }
    }
}
