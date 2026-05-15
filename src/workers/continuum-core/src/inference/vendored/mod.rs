//! Vendored model implementations from candle-transformers.
//!
//! We vendor these to fix bugs in the upstream library that haven't been released yet.
//! Each vendored file documents what was changed and why.

pub mod compact_llama;
#[cfg(feature = "metal")]
pub mod metal_deltanet;
pub mod quantized_llama;
pub mod qwen2;
