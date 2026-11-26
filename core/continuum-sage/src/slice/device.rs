//! The hardware a lane simulation runs on, as measured costs (Joel, 2026-11-26: "Can we record or simulate
//! anything? How do we optimize? Ultimately we want to best all throughput"). A device answers what the lane's
//! work costs on it; one adapter per memory model, built from a node's own measurements, so a recorded day
//! replays on the device it ran on, or on another to ask what that one would do. When a parked mind's KV moves
//! is not the device's: the room ([`super::room`]) and its prefetch decision own it.
//!
//! - [`Unified`]: one memory the CPU and GPU share (the M5). A parked mind is viewed where she lies: nothing moves.
//! - [`Dedicated`]: a discrete card (the 5090). A parked mind's KV is in host memory and comes back over the link.

use std::time::Duration;

/// A prefill chunk: `tokens` fresh tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prefill {
    pub tokens: u64,
}

/// What a device charges for the lane's work. A decode step is each mind's own (measured per mind, it depends on
/// her model and context), so it stays on her load.
pub trait Device {
    /// Its name, as a report labels it.
    fn name(&self) -> &'static str;
    fn prefill(&self, ask: Prefill) -> Duration;
    /// One block of `bytes` onto the device: one DMA over the link, or nothing on unified memory.
    fn copy(&self, bytes: u64) -> Duration;
}

/// Tokens per second, measured on a node: `rate` fresh tokens of prefill a second.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrefillRate {
    pub tokens_per_s: f64,
}

impl PrefillRate {
    fn of(&self, ask: Prefill) -> Duration {
        Duration::from_secs_f64(ask.tokens as f64 / self.tokens_per_s.max(f64::MIN_POSITIVE))
    }
}

/// One memory for CPU and GPU: a parked mind's KV is viewed where it lies.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Unified {
    pub prefill: PrefillRate,
}

impl Device for Unified {
    fn name(&self) -> &'static str {
        "unified"
    }
    fn prefill(&self, ask: Prefill) -> Duration {
        self.prefill.of(ask)
    }
    /// A pointer: nothing moves (the recurrent switch went 3.16 ms to 0.01 ms when it became one, 2026-11-25).
    fn copy(&self, _: u64) -> Duration {
        Duration::ZERO
    }
}

/// A discrete card: a parked mind's KV lives in host memory and comes back over the link.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dedicated {
    pub prefill: PrefillRate,
    /// Host to device, measured (pinned DMA).
    pub link_bytes_per_s: f64,
}

impl Device for Dedicated {
    fn name(&self) -> &'static str {
        "dedicated"
    }
    fn prefill(&self, ask: Prefill) -> Duration {
        self.prefill.of(ask)
    }
    fn copy(&self, bytes: u64) -> Duration {
        Duration::from_secs_f64(bytes as f64 / self.link_bytes_per_s.max(f64::MIN_POSITIVE))
    }
}

impl Unified {
    /// The M5 (Qwen3.6-35B-A3B, q4_0 KV, flash attention) at a 64k context: stock llama-bench 741b0175b measured
    /// prefill 968 / 650 / 304 tok/s at depth 0 / 16k / 64k (2026-10-28).
    pub const M5_AT_64K: Unified = Unified { prefill: PrefillRate { tokens_per_s: 304.0 } };
}

impl Dedicated {
    /// The 5090 (Qwen3.8-27B): cold prefill of Kimi's prompt 16,031 fresh tokens in 42,296 ms and Sahar's 16,673 in
    /// 42,065 ms, about 380-400 tok/s (Codex, 2f8bd03a1f window 17:01-17:16Z, 2026-11-26). The link is measured per
    /// node and passed in; it has no default here.
    pub fn rtx_5090(link_bytes_per_s: f64) -> Self {
        Self { prefill: PrefillRate { tokens_per_s: 16_031.0 / 42.296 }, link_bytes_per_s }
    }
}
