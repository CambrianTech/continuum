//! continuum-genome: a mind's learned skills (adapters, recall, residency, fitness, gene trials,
//! teach, fine-tuning and the training job board), cut out of continuum-mind
//! (MODULE-ARCHITECTURE.md section 9.2, the Dreaming/learning module). Depends only on the
//! engine (which carries the kernel) and below; continuum-mind re-exports `genome` under its
//! old path.

// Production code holds no unwrap, expect or panic (measured 0, 2026-11-08, campaign 522419e5):
// a fault is a typed error. Tests may use them.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub use continuum_engine::*;

pub mod genome;
