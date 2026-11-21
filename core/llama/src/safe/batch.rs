//! A batch of tokens for `Context::decode`: positions, sequences, which rows return logits, and
//! which rows are training rows (labels).

use super::*;

/// A batch of tokens to feed into `Context::decode`.
///
/// Two construction modes:
///   * `for_tokens(&[...])` — single sequence, positions auto-assigned.
///     Cheapest path for generation. Uses `llama_batch_get_one`.
///   * `allocated(n_tokens, n_seq_max)` — preallocated for manual push,
///     supports multi-sequence. Uses `llama_batch_init` / `_free`.
pub struct Batch {
    pub(super) inner: sys::llama_batch,
    pub(super) storage: BatchStorage,
    /// Present only on a [`Batch::labelled`] batch: the arrays `inner.label` and
    /// `inner.label_weight` point into, sized once to the batch's capacity so they never move.
    labels: Option<Labels>,
}

/// One row of a batch: a token at a position of a sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub token: i32,
    pub pos: i32,
    pub seq: i32,
}

/// The target after one row's token. A labelled row is a training row: `llama_decode` also runs the
/// loss, the backward and the optimizer write of the context's trainable adapter in the same graph.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Label {
    /// The token after this row, and how much it counts in the loss (finite, >= 0).
    Target { token: i32, weight: f32 },
    /// No target after this token: the trainable adapter does not apply to its row.
    Unlabelled,
}

impl Label {
    /// The native label and weight: `-1` and no weight for a row with no target.
    pub(super) fn native(self) -> (i32, f32) {
        match self {
            Self::Target { token, weight } => (token, weight),
            Self::Unlabelled => (-1, 0.0),
        }
    }
}

/// The label and weight per row of a labelled batch.
struct Labels {
    label: Vec<i32>,
    weight: Vec<f32>,
}

/// Why a labelled row was not added.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum LabelRefusal {
    #[error("this batch was not built with labels (Batch::labelled)")]
    NotLabelled,
    #[error("the batch is full at {capacity} rows")]
    Full { capacity: i32 },
    #[error("a label weight is finite and >= 0, not {0}")]
    Weight(f32),
}

pub(super) enum BatchStorage {
    /// Owns the token Vec that `inner.token` points into. The Vec is never
    /// read directly from Rust — llama.cpp dereferences the raw pointer
    /// during decode — but it MUST outlive the batch, so we pin it here.
    OneSequence(#[allow(dead_code)] Vec<i32>),
    /// C-allocated via `llama_batch_init`; must `llama_batch_free` on drop.
    /// The seq_id[i] slots are llama-owned (pre-allocated with n_seq_max
    /// slots each); we write INTO them on push, never replace the pointers.
    /// `capacity` is the llama-allocated array length — push must fail
    /// (not corrupt memory) if callers try to exceed it.
    Allocated { n_seq_max: i32, capacity: i32 },
}

unsafe impl Send for Batch {}

impl Batch {
    /// Single-sequence batch of the given tokens. Positions auto-assigned by
    /// `llama_decode` based on KV state. Only the last token gets logits.
    pub fn for_tokens(mut tokens: Vec<i32>) -> Self {
        backend_init();
        // SAFETY: tokens' backing storage is kept alive via storage field;
        // llama_batch_get_one points at the slice, does not take ownership.
        let inner = unsafe { sys::llama_batch_get_one(tokens.as_mut_ptr(), tokens.len() as i32) };
        Self {
            inner,
            storage: BatchStorage::OneSequence(tokens),
            labels: None,
        }
    }

    /// Preallocated batch capable of holding up to `n_tokens` with up to
    /// `n_seq_max` sequences per token. Populate with `push`.
    pub fn allocated(n_tokens: i32, n_seq_max: i32) -> Self {
        backend_init();
        let inner = unsafe { sys::llama_batch_init(n_tokens, 0, n_seq_max) };
        let mut b = Self {
            inner,
            storage: BatchStorage::Allocated {
                n_seq_max,
                capacity: n_tokens,
            },
            labels: None,
        };
        // init leaves n_tokens uninitialized; clear forces it to 0.
        b.clear();
        b
    }

    /// A one-sequence batch of up to `n_tokens` rows that may carry labels: her ordinary decode, with
    /// the training rows of a lesson switched on ([`Batch::push_labelled`]). Rows added with
    /// [`Batch::push`] are unlabelled.
    pub fn labelled(n_tokens: i32) -> Self {
        let mut batch = Self::allocated(n_tokens, 1);
        let rows = usize::try_from(n_tokens).unwrap_or(0); // unwrap_or: a negative capacity holds no rows, and push refuses every one
        let mut labels = Labels { label: Vec::with_capacity(rows), weight: Vec::with_capacity(rows) };
        // The vectors never grow past their capacity (push refuses a full batch first), so these
        // pointers stay valid for the batch's life.
        batch.inner.label = labels.label.as_mut_ptr();
        batch.inner.label_weight = labels.weight.as_mut_ptr();
        batch.labels = Some(labels);
        batch
    }

    /// Append a training row with the target after it. A labelled row returns its logits (the loss
    /// reads them). The fork refuses, at decode, a batch whose labelled run is followed by more rows
    /// of the same sequence.
    pub fn push_labelled(&mut self, row: Row, label: Label) -> Result<(), LabelRefusal> {
        if self.labels.is_none() {
            return Err(LabelRefusal::NotLabelled);
        }
        let (target, weight) = label.native();
        if !(weight.is_finite() && weight >= 0.0) {
            return Err(LabelRefusal::Weight(weight));
        }
        let capacity = match self.storage {
            BatchStorage::Allocated { capacity, .. } => capacity,
            BatchStorage::OneSequence(_) => return Err(LabelRefusal::NotLabelled),
        };
        if self.inner.n_tokens >= capacity {
            return Err(LabelRefusal::Full { capacity });
        }
        self.push(row.token, row.pos, &[row.seq], matches!(label, Label::Target { .. }));
        if let Some(labels) = self.labels.as_mut() {
            // `push` added an unlabelled row for this index; this row is the labelled one.
            if let (Some(row_label), Some(row_weight)) = (labels.label.last_mut(), labels.weight.last_mut()) {
                *row_label = target;
                *row_weight = weight;
            }
        }
        Ok(())
    }

    /// Append a token to an `allocated` batch. Panics if called on a
    /// `for_tokens` batch, if the batch is already at capacity, or if
    /// `seq_ids.len() > n_seq_max`.
    pub fn push(&mut self, token: i32, pos: i32, seq_ids: &[i32], want_logits: bool) {
        let (n_seq_max, capacity) = match self.storage {
            BatchStorage::Allocated {
                n_seq_max,
                capacity,
            } => (n_seq_max, capacity),
            BatchStorage::OneSequence(_) => panic!("push() on single-sequence batch"),
        };
        assert!(
            self.inner.n_tokens < capacity,
            "Batch::push overflow: n_tokens={} already at capacity={}. \
             Chunk your prefill into capacity-sized decode calls \
             (prompts longer than the batch size must be decoded in pieces).",
            self.inner.n_tokens,
            capacity
        );
        assert!(
            seq_ids.len() as i32 <= n_seq_max,
            "seq_ids.len()={} exceeds n_seq_max={}",
            seq_ids.len(),
            n_seq_max
        );
        let idx = self.inner.n_tokens as usize;
        // SAFETY: we write INTO llama-allocated arrays (token/pos/n_seq_id/
        // logits each sized n_tokens; seq_id[idx] sized n_seq_max). We do not
        // replace any pointer — `llama_batch_free` walks seq_id[] and frees
        // each slot, so keeping llama's allocation intact is required.
        unsafe {
            *self.inner.token.add(idx) = token;
            *self.inner.pos.add(idx) = pos;
            *self.inner.n_seq_id.add(idx) = seq_ids.len() as i32;
            let seq_slot = *self.inner.seq_id.add(idx);
            for (i, &sid) in seq_ids.iter().enumerate() {
                *seq_slot.add(i) = sid;
            }
            *self.inner.logits.add(idx) = i8::from(want_logits);
        }
        if let Some(labels) = self.labels.as_mut() {
            let (label, weight) = Label::Unlabelled.native();
            labels.label.push(label);
            labels.weight.push(weight);
        }
        self.inner.n_tokens += 1;
    }

    /// Reset an `allocated` batch to empty.
    pub fn clear(&mut self) {
        self.inner.n_tokens = 0;
        if let Some(labels) = self.labels.as_mut() {
            labels.label.clear();
            labels.weight.clear();
        }
    }

    pub fn n_tokens(&self) -> i32 {
        self.inner.n_tokens
    }
}

impl Drop for Batch {
    fn drop(&mut self) {
        if matches!(self.storage, BatchStorage::Allocated { .. }) {
            // `llama_batch_free` frees `label` and `label_weight` when set; a labelled batch's arrays
            // are Rust's (`Labels`), freed when it drops, so native code must not see them here.
            self.inner.label = std::ptr::null_mut();
            self.inner.label_weight = std::ptr::null_mut();
            unsafe {
                sys::llama_batch_free(self.inner);
            }
        }
        // OneSequence: Vec drop handles token memory; batch struct itself is
        // stack-allocated, no free needed.
    }
}
