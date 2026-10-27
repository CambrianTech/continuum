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
///
/// ## Two distinct mask fields
///
/// The split between `attention_mask` and `target_mask` matters and
/// they are NOT interchangeable. Reviewer-caught semantic-drift bug
/// (PR #1581 round 1) shipped a metric that used `attention_mask`
/// when it should have used `target_mask` — because `target_ids` are
/// `input_ids` shifted left by 1, the two masks differ by one
/// position. A 1-byte ByteTokenizer example whose first INPUT is
/// non-pad but whose first TARGET IS pad was being counted as
/// gradient-bearing. The two fields enforce the distinction
/// structurally.
#[derive(Debug, Clone)]
pub struct TokenizedBatch {
    /// `[batch_size, sequence_length]` token ids fed into the
    /// (eventual) embedding lookup. Pad positions hold the
    /// tokenizer's pad id (typically 0).
    pub input_ids: Tensor,
    /// `[batch_size, sequence_length]` target ids (input_ids shifted
    /// left by 1 — standard causal LM target shape).
    pub target_ids: Tensor,
    /// `[batch_size, sequence_length]` 1/0 mask aligned to INPUT
    /// positions: 1 for real input, 0 for pad. Use this for
    /// transformer attention masking (when real wiring lands).
    pub attention_mask: Tensor,
    /// `[batch_size, sequence_length]` 1/0 mask aligned to TARGET
    /// positions: 1 for real target, 0 for pad target. Use this for
    /// loss/metric masking. Distinct from `attention_mask` because
    /// targets are input_ids shifted by 1: a sample whose last real
    /// input is at position k has its last real TARGET at position
    /// k-1. The training loop uses `target_mask.narrow(1, 0, 1)` to
    /// honestly count gradient-bearing first-target samples.
    pub target_mask: Tensor,
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
            let mut all_attn_masks: Vec<u8> = Vec::with_capacity(batch_size * seq_len);
            // Target mask is computed from target_ids vs pad_id —
            // structurally distinct from attention_mask (which
            // reflects INPUT pad status). Per reviewer-caught
            // semantic-drift: target[i] is input[i+1] (causal LM
            // shift), so a sample's last real INPUT can be at
            // position k while its last real TARGET is at k-1.
            let mut all_target_masks: Vec<u8> = Vec::with_capacity(batch_size * seq_len);
            for ex in chunk {
                all_inputs.extend_from_slice(&ex.input_ids);
                all_targets.extend_from_slice(&ex.target_ids);
                all_attn_masks.extend_from_slice(&ex.attention_mask);
                for &t in &ex.target_ids {
                    all_target_masks.push(if t == pad_id { 0 } else { 1 });
                }
            }

            let input_ids = Tensor::from_vec(all_inputs, (batch_size, seq_len), device)?
                .to_dtype(DType::U32)?;
            let target_ids = Tensor::from_vec(all_targets, (batch_size, seq_len), device)?
                .to_dtype(DType::U32)?;
            // attention_mask + target_mask both as F32 for
            // loss-scaling math (U8 would need a cast anyway).
            let attention_mask = Tensor::from_vec(
                all_attn_masks.iter().map(|&v| v as f32).collect::<Vec<_>>(),
                (batch_size, seq_len),
                device,
            )?;
            let target_mask = Tensor::from_vec(
                all_target_masks
                    .iter()
                    .map(|&v| v as f32)
                    .collect::<Vec<_>>(),
                (batch_size, seq_len),
                device,
            )?;

            batches.push(TokenizedBatch {
                input_ids,
                target_ids,
                attention_mask,
                target_mask,
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
    /// Honest count of gradient-bearing target positions consumed
    /// across all `train_step` calls so far. Replaces the inflated
    /// `steps × batch × seq_len` formula that the job actor's
    /// `trained_tokens` metric used previously (Reviewer 1's BLOCK
    /// M1). In the standin path this is `steps × batch × 1` (one
    /// target per sample); in the real-model-loading path it
    /// becomes `sum of attention_mask` across all batches. Either
    /// way the count reflects what actually flowed through the
    /// gradient, not a schedule-derived guess. The actor reads this
    /// straight into [`super::types::JobMetrics::trained_tokens`].
    pub gradient_tokens_consumed: u64,
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
    /// Returns `(loss, tokens_used)` — the loss value and the count
    /// of gradient-bearing target positions THIS STEP actually
    /// trained against. Per Reviewer 1's BLOCK M1: the previous
    /// version returned only loss, and the actor's `trained_tokens`
    /// metric was computed as `steps × batch × seq_len` after the
    /// fact — inflated by `seq_len`× in the stand-in path because
    /// the stand-in trains on ONE target per sample. The honest
    /// count flows out of THIS function and accumulates into
    /// [`TrainingMetrics::gradient_tokens_consumed`].
    ///
    /// ## Stand-in vs production
    ///
    /// This is the substrate-side stand-in path that exercises the
    /// gradient flow through `LoRAModule`'s A + B Vars end-to-end.
    /// Production wiring (replaces the synthetic base when real
    /// model loading lands):
    ///   - the U32→F32 cast becomes an embedding-table lookup
    ///   - the single-class-per-sample target becomes the full
    ///     [batch * seq] flattened token-prediction targets, masked
    ///     by `attention_mask`
    ///   - `tokens_used` becomes `attention_mask.sum()` — the count
    ///     of non-pad positions that received gradient signal
    /// The optimizer + cross-entropy + backward shape stays
    /// identical across both — the swap is local to forward + target
    /// shaping.
    ///
    /// ## Pad-as-target note (Reviewer 1's LGTM M3)
    ///
    /// The stand-in pulls `target_ids.narrow(1, 0, 1)` — the FIRST
    /// target id per sample. For samples whose first target IS the
    /// pad id (very short examples that fit entirely in the pad
    /// region of the seq dim), gradient flows for "predict pad."
    /// The honest mitigation in the stand-in: `tokens_used` is
    /// computed from `attention_mask.first_column.sum()` — the
    /// count of samples whose first target is NON-pad. The metric
    /// thus reports gradient-bearing tokens accurately; cross_entropy
    /// still computes loss over all samples (including pad targets)
    /// but the count surfaces the corruption to operators. The full
    /// fix — masking pad targets out of cross_entropy via per-sample
    /// loss scaling — lands in the real-model-loading slice
    /// alongside the embedding-table swap.
    pub fn train_step(&mut self, batch: &TokenizedBatch) -> Result<(f32, u64), TrainingError> {
        // Stand-in forward: cast token ids to F32 and run through
        // the LoRA-wrapped linear. The actual transformer embeds
        // ids first; this stand-in exercises the gradient path
        // through A + B without needing the embedding table.
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

        // Honest tokens_used: count of samples whose first TARGET
        // is non-pad. Use `target_mask`, NOT `attention_mask` —
        // they differ by one position because target_ids are
        // input_ids shifted left. This is the load-bearing fix for
        // the round-1 reviewer's BLOCK on M1 semantic drift: a
        // 1-byte ByteTokenizer example's first INPUT is non-pad
        // (mask_attn[0]=1) but its first TARGET IS pad
        // (mask_target[0]=0). Using attention_mask here re-inflates
        // the metric by counting pad-targeted samples as
        // gradient-bearing — the exact bug M1 was filed to kill.
        let tokens_used: u64 =
            if batch.target_mask.dims().len() >= 2 && batch.target_mask.dims()[1] >= 1 {
                let first_col = batch
                    .target_mask
                    .narrow(1, 0, 1)?
                    .squeeze(1)?
                    .contiguous()?;
                first_col.sum_all()?.to_scalar::<f32>()? as u64
            } else {
                batch.input_ids.dim(0)? as u64
            };

        // Per `[[no-fallbacks-ever]]`: if the whole batch is
        // pad-targeted (tokens_used == 0), skip the backward step
        // entirely. Letting AdamW step on a loss derived ENTIRELY
        // from pad-target predictions would corrupt the LoRA
        // parameters with garbage gradient signal. This is the
        // partial-pad mitigation flagged by reviewer round-1 LGTM
        // M3 — the full per-sample masking inside cross_entropy
        // lands with real-model loading, but the all-pad-batch case
        // we CAN catch now.
        if tokens_used == 0 {
            self.metrics.steps_completed += 1;
            // last_train_loss stays None (no real loss computed)
            return Ok((0.0, 0));
        }

        let loss = cross_entropy(&logits, &targets_u32)?;
        self.optimizer.backward_step(&loss)?;

        let loss_scalar = loss.to_scalar::<f32>()?;
        self.metrics.steps_completed += 1;
        self.metrics.gradient_tokens_consumed += tokens_used;
        self.metrics.last_train_loss = Some(loss_scalar);
        Ok((loss_scalar, tokens_used))
    }

    /// One epoch — iterate every batch the loader produces, sum
    /// losses, update metrics.
    pub fn train_epoch(&mut self, loader: &DataLoader) -> Result<f32, TrainingError> {
        // Per-batch train_step now returns (loss, tokens_used);
        // tokens_used accumulates inside train_step into
        // metrics.gradient_tokens_consumed, so we only need loss
        // here for the averaging.
        let mut sum_loss = 0.0_f32;
        let mut count = 0_u32;
        for batch in loader.batches() {
            let (loss, _tokens_used) = self.train_step(batch)?;
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
mod tests;
