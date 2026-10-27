//! Embedding + similarity — the relevance primitive for recall.
//!
//! Recall must surface the memory RELEVANT to the current burst, not merely the
//! most recent (PERSONA-BRAIN-ARCHITECTURE.md §5 / §2.9: "always remembering" =
//! similarity recall into the long-term store). That needs a vector embedding of
//! text + a similarity measure. This module owns both, behind a trait so the
//! BACKEND is swappable:
//!
//! - [`LexicalEmbedder`] — the bootstrap: a hashing-trick term-frequency vector.
//!   Real, deterministic, zero-dependency topical-overlap relevance that works on
//!   ANY machine with no model loaded ("solve for public users"). It is NOT a
//!   stub — lexical overlap is a genuine (if shallow) relevance signal.
//! - A neural embedder (llama.cpp `--embedding` mode, or a dedicated bge/nomic
//!   model) slots in behind the SAME trait for semantic relevance, when an
//!   embedding backend exists on the grid. The recall re-ranker (§7) does not
//!   change when it does — only the vectors get smarter.
//!
//! This is the project's adapter discipline: build the simplest outlier first,
//! prove the interface, let the strong backend slot in unchanged.

use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use dashmap::DashMap;

use crate::ai::adapter::AIProviderAdapter;
use crate::ai::types::{EmbeddingInput, EmbeddingRequest};

mod similarity;
pub mod vector_math;

pub use similarity::cosine_similarity;

mod embedding_cache_max;
pub use embedding_cache_max::*;
mod try_neural_embedder;
pub use try_neural_embedder::*;
#[cfg(any(test, feature = "test-fixtures"))]
mod counting_embedder;
#[cfg(any(test, feature = "test-fixtures"))]
pub use counting_embedder::CountingEmbedder;

#[cfg(test)]
mod tests;
