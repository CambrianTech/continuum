//! `embedding/*` — pure vector-math commands over caller-provided embeddings.
//!
//! Stateless. The SIMD math kernels (the ONE vectorized cosine path, shared by
//! every embedding kind) live in [`crate::modules::embedding`]; these commands are
//! thin typed wrappers that put that math on the ONE registry → the persona tool
//! surface, `uu`, the SDK, and the grid ACL. Each self-registers via
//! `register_stateless_command!` — no module `commands()` wiring, no `match` arm.
//!
//! Embedding *generation* is NOT here — it is adapter-routed (`/v1/embeddings`,
//! task #40). These commands score vectors the adapter already produced. They
//! replace the dead `embedding/*` IPC arms the retired TS persona runtime drove
//! (zero live Rust/TS callers); the binary `similarity-matrix` wire shape was
//! dropped with them (no consumer needed the raw f32 bytes) in favor of clean
//! typed JSON.

pub mod cluster;
pub mod similarity;
pub mod similarity_matrix;
pub mod top_k;
