//! Training loop primitives — data loader + AdamW-backed trainer.
//!
//! Sits on top of [`super::LoRAModule`] (math from #231). Provides
//! the substrate-side trainable pipeline that #233 wires into the
//! `LocalCandleFineTuner` lifecycle actor.
//!
//! ## What's here
//!
//! - **[`Tokenizer`] trait** — substrate-side abstraction over the
//!   tokenizer choice. The real impl (loading a base model's HF
//!   tokenizer) lands in #233 alongside the model-loading wiring;
//!   this module defines the trait so the data loader is
//!   testable in isolation against a fake.
//! - **[`TokenizedExample`] + [`TokenizedBatch`]** — typed
//!   tokenization output. Batches carry inputs + targets + an
//!   attention mask for padded positions.
//! - **[`DataLoader`]** — turns a `&[TrainingExample]` into batches
//!   of `[batch_size, sequence_length]` shape. Pads short
//!   sequences with the configured pad token; truncates long ones.
//! - **[`LoRATrainer`]** — wraps a `LoRAModule` + an AdamW
//!   optimizer, exposes `train_step` (single batch forward + loss
//!   + backward) and `train_epoch` (iterate a `DataLoader`'s
//!   batches, accumulate metrics).
//! - **[`TrainingMetrics`]** — per-step + per-epoch counters the
//!   lifecycle actor (#233) reads to populate the substrate's
//!   `TrainingStatus::Running { progress_pct, current_epoch }`.
//!
//! ## What's NOT here (deferred to #233)
//!
//! - **Job lifecycle actor** — the in-process `tokio::task` that
//!   owns one in-flight training run, exposes a `watch::Sender`
//!   for status snapshots, accepts cancel signals.
//! - **Checkpoint + safetensors output** — periodic checkpoints
//!   during training + the final safetensors write that produces
//!   the `TrainingArtifact`.
//! - **Real HF tokenizer wiring** — lives next to the model
//!   loader because they share the safetensors path.
//!
//! ## Doctrinal alignment
//!
//! - `[[no-fallbacks-ever]]`: every loader / trainer entry point
//!   returns a typed error. Empty dataset, zero batch size,
//!   sequence length mismatch — all surface as construction
//!   errors instead of opaque tensor failures at first forward.
//! - `[[matrix-dojo-layer-loading-as-substrate-primitive]]`: the
//!   trainer's output Vars stay in the same tensor layout as the
//!   inference-side `LoRAWeights`. After #233 wires safetensors
//!   output, layers loop back through `inference/lora.rs` with no
//!   glue.

use candle_core::{DType, Device, Tensor};
use candle_nn::loss::cross_entropy;
use candle_nn::{AdamW, Optimizer, ParamsAdamW};

use super::lora_module::LoRAModule;
use super::types::TrainingExample;

// ─── Tokenizer abstraction ──────────────────────────────────────────

/// Substrate-side tokenizer abstraction. The real impl (loading a
/// base model's HF tokenizer) lands in #233; this trait keeps the
/// data loader testable in isolation against a deterministic fake.
pub trait Tokenizer: Send + Sync {
    /// Encode a string into token ids. Used for both prompts and
    /// completions. Substrate-side tokenizers MUST be deterministic
    /// (same input → same output) — non-determinism here would
    /// produce different training data on retry, breaking
    /// reproducibility.
    fn encode(&self, text: &str) -> Result<Vec<u32>, TrainingError>;

    /// Token id used for padding short sequences. Loss masking
    /// against this id is the trainer's job, not the tokenizer's.
    fn pad_token_id(&self) -> u32;
}

// ─── Errors ─────────────────────────────────────────────────────────

/// Typed failure modes for the data loader + trainer. Mirror taxonomy
/// of `super::FineTuningError::LocalTrainerFailed` so the lifecycle
/// actor (#233) can propagate each variant with its specific
/// diagnostic intact.
#[derive(Debug, thiserror::Error)]
pub enum TrainingError {
    #[error("dataset is empty; need at least one example to train")]
    EmptyDataset,

    #[error("batch_size must be > 0, got {0}")]
    InvalidBatchSize(u32),

    #[error("sequence_length must be > 0, got {0}")]
    InvalidSequenceLength(u32),

    #[error("tokenizer encode failed: {0}")]
    TokenizerFailed(String),

    #[error("candle tensor op: {0}")]
    Candle(#[from] candle_core::Error),
}

// ─── Tokenized representations ──────────────────────────────────────

/// One example after tokenization. Used internally by the
/// `DataLoader` to build batches.
#[derive(Debug, Clone)]
pub struct TokenizedExample {
    /// Concatenated prompt + completion token ids.
    pub input_ids: Vec<u32>,
    /// Target ids for next-token prediction. Equal to `input_ids`
    /// shifted left by one (standard causal LM target shape).
    pub target_ids: Vec<u32>,
    /// 1 for prompt+completion tokens, 0 for padding. Loss masks on
    /// this so gradient flows only from real tokens.
    pub attention_mask: Vec<u8>,
}

/// One batch of tokenized examples, padded to the same length.
/// Tensors live on the configured [`Device`]. Used as the input to
/// [`LoRATrainer::train_step`].
#[derive(Debug, Clone)]
pub struct TokenizedBatch {
    /// `[batch_size, sequence_length]` token ids.
    pub input_ids: Tensor,
    /// `[batch_size, sequence_length]` target ids (input_ids shifted left).
    pub target_ids: Tensor,
    /// `[batch_size, sequence_length]` 1/0 mask; 1 for real, 0 for pad.
    pub attention_mask: Tensor,
}

// ─── Data loader ────────────────────────────────────────────────────

/// Turns a `&[TrainingExample]` into batches of typed
/// [`TokenizedBatch`]s. Padding / truncation / batch grouping
/// happen here so the trainer's `train_step` consumes uniform
/// `[batch_size, sequence_length]` tensors.
pub struct DataLoader {
    batches: Vec<TokenizedBatch>,
}

impl DataLoader {
    /// Pre-tokenize and batch the dataset. Substrate-side training
    /// preloads the full dataset into batches up front (training
    /// datasets are O(thousands of examples) — fits in memory; no
    /// reason to stream and pay tokenization cost per epoch).
    pub fn new(
        examples: &[TrainingExample],
        tokenizer: &dyn Tokenizer,
        batch_size: u32,
        sequence_length: u32,
        device: &Device,
    ) -> Result<Self, TrainingError> {
        if examples.is_empty() {
            return Err(TrainingError::EmptyDataset);
        }
        if batch_size == 0 {
            return Err(TrainingError::InvalidBatchSize(0));
        }
        if sequence_length == 0 {
            return Err(TrainingError::InvalidSequenceLength(0));
        }

        let seq_len = sequence_length as usize;
        let batch_size = batch_size as usize;
        let pad_id = tokenizer.pad_token_id();

        // 1. Tokenize every example. Each example's input_ids are
        //    `encode(prompt) ++ encode(completion)`; target_ids are
        //    shifted left by one. Truncate to `sequence_length`
        //    if over; pad with `pad_id` if under.
        let mut tokenized: Vec<TokenizedExample> = Vec::with_capacity(examples.len());
        for ex in examples {
            let prompt_ids = tokenizer.encode(&ex.prompt)?;
            let completion_ids = tokenizer.encode(&ex.completion)?;
            let mut input_ids: Vec<u32> = prompt_ids
                .into_iter()
                .chain(completion_ids.into_iter())
                .collect();

            // Truncate or pad to seq_len + 1 (we need an extra
            // token at the end so target_ids = input_ids[1..] is
            // exactly seq_len long).
            if input_ids.len() > seq_len + 1 {
                input_ids.truncate(seq_len + 1);
            } else {
                input_ids.resize(seq_len + 1, pad_id);
            }

            // target_ids = input_ids shifted left by 1.
            let target_ids: Vec<u32> = input_ids[1..].to_vec();
            let inputs: Vec<u32> = input_ids[..seq_len].to_vec();

            // attention_mask: 1 for real tokens, 0 for pad. A pad
            // run at the end of inputs marks pad positions; before
            // that there can be no pads (the encode results were
            // dense token streams).
            let attention_mask: Vec<u8> = inputs
                .iter()
                .map(|&id| if id == pad_id { 0 } else { 1 })
                .collect();

            tokenized.push(TokenizedExample {
                input_ids: inputs,
                target_ids,
                attention_mask,
            });
        }

        // 2. Group into batches. Drop the last partial batch
        //    (standard practice — keeps batch-size invariant for
        //    BatchNorm / similar, and the loss scale stays stable).
        let mut batches: Vec<TokenizedBatch> = Vec::new();
        for chunk in tokenized.chunks(batch_size) {
            if chunk.len() < batch_size {
                continue;
            }

            let mut all_inputs: Vec<u32> = Vec::with_capacity(batch_size * seq_len);
            let mut all_targets: Vec<u32> = Vec::with_capacity(batch_size * seq_len);
            let mut all_masks: Vec<u8> = Vec::with_capacity(batch_size * seq_len);
            for ex in chunk {
                all_inputs.extend_from_slice(&ex.input_ids);
                all_targets.extend_from_slice(&ex.target_ids);
                all_masks.extend_from_slice(&ex.attention_mask);
            }

            let input_ids =
                Tensor::from_vec(all_inputs, (batch_size, seq_len), device)?.to_dtype(DType::U32)?;
            let target_ids =
                Tensor::from_vec(all_targets, (batch_size, seq_len), device)?.to_dtype(DType::U32)?;
            // attention_mask as F32 (for multiplying loss; U8 would
            // need a cast anyway).
            let attention_mask = Tensor::from_vec(
                all_masks.iter().map(|&v| v as f32).collect::<Vec<_>>(),
                (batch_size, seq_len),
                device,
            )?;

            batches.push(TokenizedBatch {
                input_ids,
                target_ids,
                attention_mask,
            });
        }

        if batches.is_empty() {
            return Err(TrainingError::EmptyDataset);
        }

        Ok(Self { batches })
    }

    /// Number of batches this loader produces per epoch.
    pub fn len(&self) -> usize {
        self.batches.len()
    }

    pub fn is_empty(&self) -> bool {
        self.batches.is_empty()
    }

    /// Borrow the batches for one epoch. Substrate-side training
    /// passes through every batch each epoch; data shuffling (when
    /// added) randomizes the order on each call.
    pub fn batches(&self) -> impl Iterator<Item = &TokenizedBatch> {
        self.batches.iter()
    }
}

// ─── Trainer ────────────────────────────────────────────────────────

/// Per-step + per-epoch counters. Read by the lifecycle actor
/// (#233) to populate `TrainingStatus::Running { progress_pct,
/// current_epoch }` and `JobMetrics.final_loss`.
#[derive(Debug, Default, Clone)]
pub struct TrainingMetrics {
    pub steps_completed: u64,
    pub epochs_completed: u32,
    /// Loss on the most recent training step.
    pub last_train_loss: Option<f32>,
    /// Average loss across the most recent epoch.
    pub last_epoch_avg_loss: Option<f32>,
}

/// Wraps a [`LoRAModule`] + an AdamW optimizer + cross-entropy loss.
/// One `LoRATrainer` per training job (the lifecycle actor builds
/// one, runs N epochs through it, then drops it to free the optimizer
/// state).
///
/// ## Why mut self on train_step
///
/// The optimizer holds AdamW's first and second moment estimates,
/// which mutate every step. `train_step` takes `&mut self`. The
/// lifecycle actor (#233) owns the trainer inside its tokio task
/// + drops it cleanly when the job terminates.
pub struct LoRATrainer {
    module: LoRAModule,
    optimizer: AdamW,
    metrics: TrainingMetrics,
}

impl LoRATrainer {
    /// Construct with the module + AdamW hyperparams. The
    /// optimizer collects the LoRAModule's two `Var`s; base weight
    /// is frozen (NOT included) so its gradient is never computed
    /// — saves memory + compute.
    pub fn new(module: LoRAModule, learning_rate: f64) -> Result<Self, TrainingError> {
        let params = ParamsAdamW {
            lr: learning_rate,
            ..ParamsAdamW::default()
        };
        let vars = vec![module.lora_a().clone(), module.lora_b().clone()];
        let optimizer = AdamW::new(vars, params)?;
        Ok(Self {
            module,
            optimizer,
            metrics: TrainingMetrics::default(),
        })
    }

    /// One forward + cross-entropy + backward + optimizer step.
    /// Returns the loss value for this step (also stored in
    /// `metrics.last_train_loss`).
    ///
    /// ## Stand-in vs production
    ///
    /// This is the substrate-side stand-in path that exercises the
    /// gradient flow through `LoRAModule`'s A + B Vars end-to-end.
    /// Production wiring (#233) replaces:
    ///   - the U32→F32 cast with an embedding-table lookup
    ///   - the single-class-per-sample target with the full
    ///     [batch * seq] flattened token-prediction targets
    ///   - the standin's [batch, out_features] logits with a
    ///     transformer's [batch, seq, vocab] logits flattened to
    ///     [batch * seq, vocab]
    /// The optimizer + cross-entropy + backward shape stays
    /// identical across both — the swap is local to forward + target
    /// shaping.
    pub fn train_step(&mut self, batch: &TokenizedBatch) -> Result<f32, TrainingError> {
        // Stand-in forward: cast token ids to F32 and run through
        // the LoRA-wrapped linear. The actual transformer (#233)
        // embeds ids first; this stand-in exercises the gradient
        // path through A + B without needing the embedding table.
        let inputs_f32 = batch.input_ids.to_dtype(DType::F32)?;
        let logits = self.module.forward(&inputs_f32)?;

        // Stand-in target: first target id per sample (single-class
        // prediction). Real model wiring uses the full
        // sequence-aligned targets via flatten_all() and matches
        // logits flattened over batch * seq.
        let target_dims = batch.target_ids.dims();
        let targets_per_sample = if target_dims.len() >= 2 && target_dims[1] >= 1 {
            // `.contiguous()` is required after narrow → squeeze
            // because `narrow` produces a strided view; candle's
            // `cross_entropy` runs a `gather` underneath which only
            // accepts contiguous tensors. Without this, batch_size=1
            // passes (trivially contiguous) but batch_size>1 fails
            // at runtime — a class of bug that's easy to miss.
            batch.target_ids.narrow(1, 0, 1)?.squeeze(1)?.contiguous()?
        } else {
            batch.target_ids.clone()
        };
        let targets_u32 = targets_per_sample.to_dtype(DType::U32)?;

        let loss = cross_entropy(&logits, &targets_u32)?;
        self.optimizer.backward_step(&loss)?;

        let loss_scalar = loss.to_scalar::<f32>()?;
        self.metrics.steps_completed += 1;
        self.metrics.last_train_loss = Some(loss_scalar);
        Ok(loss_scalar)
    }

    /// One epoch — iterate every batch the loader produces, sum
    /// losses, update metrics.
    pub fn train_epoch(&mut self, loader: &DataLoader) -> Result<f32, TrainingError> {
        let mut sum_loss = 0.0_f32;
        let mut count = 0_u32;
        for batch in loader.batches() {
            let loss = self.train_step(batch)?;
            sum_loss += loss;
            count += 1;
        }
        let avg = if count > 0 {
            sum_loss / count as f32
        } else {
            0.0
        };
        self.metrics.epochs_completed += 1;
        self.metrics.last_epoch_avg_loss = Some(avg);
        Ok(avg)
    }

    /// Borrow the wrapped module — used after training to read
    /// the final A/B tensors for safetensors writeout (#233).
    pub fn module(&self) -> &LoRAModule {
        &self.module
    }

    /// Snapshot of metrics. Cheap to clone; the lifecycle actor
    /// (#233) reads this periodically to publish status updates
    /// via the watch channel.
    pub fn metrics(&self) -> TrainingMetrics {
        self.metrics.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic fake tokenizer: maps each char to its ASCII
    /// value (mod a small vocab). pad_token_id = 0. Easy to reason
    /// about + reproducible.
    struct FakeTokenizer {
        vocab: u32,
    }

    impl Tokenizer for FakeTokenizer {
        fn encode(&self, text: &str) -> Result<Vec<u32>, TrainingError> {
            Ok(text
                .bytes()
                .map(|b| (b as u32 % self.vocab).max(1)) // avoid colliding with pad=0
                .collect())
        }
        fn pad_token_id(&self) -> u32 {
            0
        }
    }

    fn example(prompt: &str, completion: &str) -> TrainingExample {
        TrainingExample {
            prompt: prompt.into(),
            completion: completion.into(),
            metadata: None,
        }
    }

    // what this catches: empty dataset → typed EmptyDataset, not a
    // mysterious tensor error later. The dead `channel.rs` trigger
    // we deleted in #1572 used to fire training on empty datasets;
    // this is the contract that catches "no examples sent" at the
    // boundary.
    #[test]
    fn empty_dataset_rejected() {
        let tok = FakeTokenizer { vocab: 32 };
        let err = DataLoader::new(&[], &tok, 1, 4, &Device::Cpu)
            .err()
            .expect("must reject");
        assert!(matches!(err, TrainingError::EmptyDataset));
    }

    // what this catches: 0 batch size → typed error. A future
    // refactor that silently sets batch_size=1 on zero input would
    // change training dynamics without the caller's knowledge.
    #[test]
    fn zero_batch_size_rejected() {
        let tok = FakeTokenizer { vocab: 32 };
        let err =
            DataLoader::new(&[example("hi", "ok")], &tok, 0, 4, &Device::Cpu)
                .err()
                .expect("must reject");
        assert!(matches!(err, TrainingError::InvalidBatchSize(0)));
    }

    // what this catches: 0 sequence length → typed error.
    #[test]
    fn zero_seq_length_rejected() {
        let tok = FakeTokenizer { vocab: 32 };
        let err =
            DataLoader::new(&[example("hi", "ok")], &tok, 1, 0, &Device::Cpu)
                .err()
                .expect("must reject");
        assert!(matches!(err, TrainingError::InvalidSequenceLength(0)));
    }

    // what this catches: batch tensor shape contract. Loader
    // produces `[batch_size, sequence_length]` shaped tensors for
    // input_ids, target_ids, attention_mask. A future refactor
    // that swaps axes (sequence-first) would break every
    // downstream consumer.
    #[test]
    fn batches_have_shape_batch_size_x_seq_len() {
        let tok = FakeTokenizer { vocab: 32 };
        let examples = vec![
            example("hello", "world"),
            example("foo", "bar"),
            example("ping", "pong"),
            example("aa", "bb"),
        ];
        let loader =
            DataLoader::new(&examples, &tok, 2, 6, &Device::Cpu).unwrap();
        // 4 examples / batch_size 2 = 2 batches.
        assert_eq!(loader.len(), 2);
        for batch in loader.batches() {
            assert_eq!(batch.input_ids.dims(), &[2, 6]);
            assert_eq!(batch.target_ids.dims(), &[2, 6]);
            assert_eq!(batch.attention_mask.dims(), &[2, 6]);
        }
    }

    // what this catches: attention_mask is 0 for pad positions, 1
    // for real tokens. A future refactor that flips polarity (1
    // for pad) would silently invert which positions contribute
    // to loss — training would treat pads as gradient signal.
    #[test]
    fn attention_mask_is_1_for_real_0_for_pad() {
        let tok = FakeTokenizer { vocab: 32 };
        let examples = vec![example("a", "b")]; // very short → padded
        let loader =
            DataLoader::new(&examples, &tok, 1, 8, &Device::Cpu).unwrap();
        let batch = loader.batches().next().unwrap();
        let mask: Vec<f32> = batch.attention_mask.flatten_all().unwrap().to_vec1().unwrap();
        // First 2 tokens are real (encoded "ab"), rest are pad.
        assert_eq!(mask[0], 1.0);
        assert_eq!(mask[1], 1.0);
        for &v in &mask[2..] {
            assert_eq!(v, 0.0, "pad positions must have mask=0");
        }
    }

    // what this catches: partial last batch is dropped. The trainer
    // relies on stable batch_size for loss aggregation; a partial
    // final batch would skew metrics. A future refactor that pads
    // the partial would silently inject pad gradient.
    #[test]
    fn partial_last_batch_is_dropped() {
        let tok = FakeTokenizer { vocab: 32 };
        let examples = vec![
            example("a", "b"),
            example("c", "d"),
            example("e", "f"),
        ];
        // 3 examples, batch_size=2 → 1 full batch + 1 partial,
        // partial is dropped.
        let loader =
            DataLoader::new(&examples, &tok, 2, 4, &Device::Cpu).unwrap();
        assert_eq!(loader.len(), 1);
    }

    // what this catches: AdamW step actually moves the parameters.
    // After one train_step, at least one of A or B must have
    // changed (gradient flowed, optimizer updated). A future
    // refactor that misconfigures the var list (e.g. passes base
    // weight to AdamW instead of A/B) would silently freeze the
    // LoRA — the test catches that immediately.
    #[test]
    fn adamw_step_moves_lora_parameters() {
        let device = Device::Cpu;
        let base = Tensor::full(0.1f32, (4, 4), &device).unwrap();
        let module = LoRAModule::new(base, 2, 4, DType::F32, &device).unwrap();

        // Capture A snapshot before training.
        let a_before: Vec<f32> = module
            .lora_a()
            .as_tensor()
            .flatten_all()
            .unwrap()
            .to_vec1()
            .unwrap();

        let mut trainer = LoRATrainer::new(module, 0.1).unwrap();

        // Build a fake batch with non-trivial inputs + targets so
        // the loss has gradient.
        let input_ids = Tensor::from_vec(
            vec![1u32, 2, 3, 0],
            (1, 4),
            &device,
        )
        .unwrap();
        let target_ids = Tensor::from_vec(vec![2u32, 3, 0, 0], (1, 4), &device).unwrap();
        let mask = Tensor::full(1.0f32, (1, 4), &device).unwrap();
        let batch = TokenizedBatch {
            input_ids,
            target_ids,
            attention_mask: mask,
        };

        trainer.train_step(&batch).unwrap();

        let a_after: Vec<f32> = trainer
            .module()
            .lora_a()
            .as_tensor()
            .flatten_all()
            .unwrap()
            .to_vec1()
            .unwrap();
        let max_delta = a_before
            .iter()
            .zip(a_after.iter())
            .map(|(b, a)| (a - b).abs())
            .fold(0.0_f32, f32::max);
        assert!(
            max_delta > 0.0,
            "AdamW step must move A (gradient signal flowed); max |Δ| = {max_delta}"
        );
    }

    // what this catches: metrics counters increment correctly.
    // step counter advances by 1 per train_step; epoch counter
    // advances by 1 per train_epoch. A future refactor that
    // miscounts would make the lifecycle actor's progress_pct
    // wrong, surfacing as stuck status in operator dashboards.
    #[test]
    fn metrics_count_steps_and_epochs() {
        let device = Device::Cpu;
        let base = Tensor::full(0.1f32, (4, 4), &device).unwrap();
        let module = LoRAModule::new(base, 2, 4, DType::F32, &device).unwrap();
        let mut trainer = LoRATrainer::new(module, 0.001).unwrap();

        // Vocab=4 lines up with the LoRAModule's out_features=4,
        // so the stand-in cross-entropy in train_step sees target
        // class indices that exist. A larger fake vocab would
        // produce out-of-range targets and surface as a gather
        // error inside cross_entropy — wrong path for THIS test
        // (this test exercises metric counters, not class-range
        // validation; that lives upstream in #233's real wiring).
        let tok = FakeTokenizer { vocab: 4 };
        let examples = vec![
            example("aa", "bb"),
            example("cc", "dd"),
            example("ee", "ff"),
            example("gg", "hh"),
        ];
        let loader = DataLoader::new(&examples, &tok, 2, 4, &device).unwrap();
        // 4 examples / batch=2 → 2 batches per epoch.
        assert_eq!(loader.len(), 2);

        assert_eq!(trainer.metrics().steps_completed, 0);
        assert_eq!(trainer.metrics().epochs_completed, 0);

        trainer.train_epoch(&loader).unwrap();
        assert_eq!(trainer.metrics().steps_completed, 2);
        assert_eq!(trainer.metrics().epochs_completed, 1);

        trainer.train_epoch(&loader).unwrap();
        assert_eq!(trainer.metrics().steps_completed, 4);
        assert_eq!(trainer.metrics().epochs_completed, 2);
    }
}
