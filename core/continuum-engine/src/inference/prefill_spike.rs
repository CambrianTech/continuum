//! The served model's per-prefill compute-buffer bytes, published by the serving plan and
//! read by grid placement (`capacity::lease::decide_render_lane`).
//!
//! This was the prefill throttle (#56): a semaphore that let only as many served lanes
//! prefill at once as live free GPU could hold one compute-buffer spike each. One
//! llama-server context reserves its worst-case compute graph once, at load, and packs
//! every slot's tokens into each `llama_decode`, so concurrent prefills in one engine draw
//! no extra transient memory. The throttle guarded nothing and serialized prefill: on the
//! M5 it held 1 permit 22.5% and 2 permits 49.4% of 87 minutes for four minds, and a mind
//! that already held a lane waited a median 24.6 s (p90 83 s) for it before her model call
//! (2026-10-28, build 7ace74020, 161 acts). The fact it carried stays; the gate is gone.

use std::sync::atomic::{AtomicU64, Ordering};

static SPIKE_BYTES: AtomicU64 = AtomicU64::new(0);

/// Publish the served model's per-prefill compute-buffer bytes (0 = unknown).
pub fn publish(spike_bytes: u64) {
    SPIKE_BYTES.store(spike_bytes, Ordering::Release);
}

/// The last published per-prefill bytes; 0 until a plan publishes one.
pub fn published_bytes() -> u64 {
    SPIKE_BYTES.load(Ordering::Acquire)
}
