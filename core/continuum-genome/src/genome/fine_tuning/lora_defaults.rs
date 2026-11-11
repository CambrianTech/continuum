//! The LoRA shape and schedule a learner takes when its request names none: one owner for every
//! backend that learns (the in-place learner's `learn_policy`, the forge's MLX run).

use super::{LoRAHyperparams, ScheduleParams};

/// The learning rate a LoRA run takes when its request names none, for EVERY backend (the
/// engine, CUDA, MLX and the stand-in actor). It was two numbers that disagreed: 1e-5 here
/// (from #4322, never measured against working gradients; every gene before 2026-10-07
/// trained on broken ones) and 1e-4 in the actor. Measured on the 5090 2026-10-10 (job
/// ff186697, Kimi's 18 'code' examples, the exact walk): at 1e-5 train_loss went 2.7796 ->
/// 2.7733 over an epoch, -0.2%. The exact walk takes ONE AdamW step per window, so 18
/// examples x 3 epochs is 54 steps, and at 1e-5 the adapter barely moves. 1e-4 is the
/// conventional floor of the LoRA range at rank 8.
pub(crate) const DEFAULT_LEARNING_RATE: f64 = 1e-4;

pub(crate) fn default_schedule() -> ScheduleParams {
    ScheduleParams {
        epochs: 3,
        batch_size: 4,
        sequence_length: 2048,
        learning_rate: DEFAULT_LEARNING_RATE,
    }
}

pub(crate) fn default_lora() -> LoRAHyperparams {
    LoRAHyperparams {
        rank: 8,
        alpha: 16,
        dropout: 0.0,
        target_modules: vec!["q_proj".into(), "v_proj".into()],
        top_layers: None,
    }
}

