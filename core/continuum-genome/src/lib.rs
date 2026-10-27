//! continuum-genome: a mind's learned skills (adapters, recall, residency, fitness, gene trials,
//! teach, fine-tuning and the training job board), cut out of continuum-mind
//! (MODULE-ARCHITECTURE.md section 9.2, the Dreaming/learning module). Depends only on the
//! engine (which carries the kernel) and below; continuum-mind re-exports `genome` under its
//! old path.

pub use continuum_engine::*;

pub mod genome;
