//! Text Analysis Module
//!
//! Pure-compute text analysis functions moved from TypeScript god classes.
//! Each sub-module is independently callable and composable.
//!
//! Phase 1: Unified Jaccard similarity (kills 3 TS duplicates)
//! Phase 2: Validation gates (garbage detection, loop detection)
//! Phase 3: Mention detection, response cleaning
//! Phase 4: Conversation heuristics

pub mod similarity;
pub mod types;

pub use similarity::{jaccard_char_bigram_similarity, jaccard_ngram_similarity, check_semantic_loop};
pub use types::*;
