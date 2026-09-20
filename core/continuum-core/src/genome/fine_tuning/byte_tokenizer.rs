//! [`ByteTokenizer`] — substrate-side deterministic byte-level
//! tokenizer. Default fallback for the [`super::Tokenizer`] trait
//! when no model-specific tokenizer is loaded.
//!
//! ## Why ship one
//!
//! The training pipeline (`training_loop.rs`) requires a [`Tokenizer`]
//! to turn text into id sequences. Production wiring (#233's follow-up
//! after real HF tokenizer support lands) replaces this with a
//! Qwen / Llama / Mistral tokenizer loaded from disk. Until then, the
//! substrate needs a *real* tokenizer (not a test-only one) so the
//! `LocalCandleFineTuner` can run end-to-end against arbitrary input
//! without any model-specific assets.
//!
//! Byte-level tokenization is a legitimate substrate primitive — some
//! research lines (ByT5, MambaByte) train directly on bytes; pad=0 is
//! the standard convention so byte b maps to id (b + 1), giving a
//! vocab of 257 (0 = pad, 1..256 = bytes 0..255).
//!
//! ## What it isn't
//!
//! - Not a *good* tokenizer in absolute terms — sequence length blows
//!   up by ~4x vs BPE. Use a model-specific tokenizer when one is
//!   wired.
//! - Not the FakeTokenizer used in `training_loop.rs` tests — that's
//!   deliberately weakened to vocab=4/32 so cross-entropy targets
//!   stay in range against tiny LoRA modules. ByteTokenizer is real.

use super::training_loop::{Tokenizer, TrainingError};

/// Vocabulary size for [`ByteTokenizer`] outputs: 1 pad token (id 0)
/// plus 256 byte values (ids 1..=256).
pub const BYTE_VOCAB: u32 = 257;

/// Pad token id used by [`ByteTokenizer`]. Held in a constant so
/// downstream code that masks against pad references the same value
/// the tokenizer produces — a future refactor changing the pad id
/// in one place but not the other would silently mis-mask training
/// loss.
pub const BYTE_PAD_ID: u32 = 0;

/// Deterministic byte-level tokenizer. Encodes each UTF-8 byte b as
/// id (b + 1), reserving 0 for the pad token.
#[derive(Debug, Default, Clone, Copy)]
pub struct ByteTokenizer;

impl ByteTokenizer {
    pub const fn new() -> Self {
        Self
    }
}

impl Tokenizer for ByteTokenizer {
    fn encode(&self, text: &str) -> Result<Vec<u32>, TrainingError> {
        Ok(text.bytes().map(|b| (b as u32) + 1).collect())
    }

    fn pad_token_id(&self) -> u32 {
        BYTE_PAD_ID
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: byte b maps to id (b + 1), pad id is 0.
    // A refactor that flipped the convention (b maps to id b, pad=256)
    // would silently invert which positions get masked from training
    // loss. This test pins the offset convention.
    #[test]
    fn encodes_byte_offset_by_one_pad_is_zero() {
        let tok = ByteTokenizer::new();
        assert_eq!(tok.pad_token_id(), 0);
        let ids = tok.encode("ab").unwrap();
        assert_eq!(ids, vec![b'a' as u32 + 1, b'b' as u32 + 1]);
        // No encoded byte ever collides with pad.
        for id in &ids {
            assert_ne!(*id, BYTE_PAD_ID);
        }
    }

    // what this catches: vocab constant matches actual range. A
    // future refactor that added a special token without bumping the
    // constant would silently produce out-of-range ids feeding into
    // cross-entropy (gather panic at runtime).
    #[test]
    fn vocab_constant_covers_all_emitted_ids() {
        let tok = ByteTokenizer::new();
        // Every possible byte value, encoded.
        let all_bytes: Vec<u8> = (0..=255).collect();
        let text = String::from_utf8_lossy(&all_bytes).into_owned();
        let ids = tok.encode(&text).unwrap();
        for id in ids {
            assert!(
                id < BYTE_VOCAB,
                "encoded id {id} must be < BYTE_VOCAB {BYTE_VOCAB}"
            );
        }
    }

    // what this catches: encoding is byte-deterministic — same input
    // always produces same output. Non-determinism here would break
    // training reproducibility (re-run with same dataset → different
    // gradients → different artifact hash).
    #[test]
    fn encoding_is_deterministic() {
        let tok = ByteTokenizer::new();
        let a = tok.encode("the quick brown fox").unwrap();
        let b = tok.encode("the quick brown fox").unwrap();
        assert_eq!(a, b);
    }
}
