//! Metal DeltaNet kernel — stub for fused recurrence dispatch.
//!
//! The .metal shader is drafted (deltanet_recurrence.metal).
//! This module will compile it at runtime and dispatch via candle's Metal device.
//! For now it's a stub that signals the caller to use the CPU path.

use candle_core::{Result, Tensor};

/// Run the fused DeltaNet recurrence on Metal.
/// Returns Err to signal the caller to fall back to CPU.
///
/// When implemented, this will:
/// 1. Compile deltanet_recurrence.metal (cached via OnceLock)
/// 2. Extract raw Metal buffers from input tensors
/// 3. Dispatch deltanet_recurrence_single (seq_len=1) or _prefill (seq_len>1)
/// 4. Return the output tensor on the Metal device
pub fn deltanet_recurrence_metal(
    _q: &Tensor,
    _k: &Tensor,
    _v: &Tensor,
    _g: &Tensor,
    _beta: &Tensor,
    _state: &mut Tensor,
    _s_k: usize,
    _s_v: usize,
    _num_heads: usize,
    _seq_len: usize,
) -> Result<Tensor> {
    candle_core::bail!("Metal DeltaNet kernel not yet wired — use CPU path")
}
