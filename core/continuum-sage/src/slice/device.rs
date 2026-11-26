//! The hardware a lane simulation runs on, as measured costs (Joel, 2026-11-26: "Can we record or simulate
//! anything? How do we optimize? Ultimately we want to best all throughput"). A device answers what the lane's
//! work costs on it; one adapter per memory model, built from a node's own measurements, so a recorded day
//! replays on the device it ran on, or on another to ask what that one would do.
//!
//! - [`Unified`]: one memory the CPU and GPU share (the M5). Binding a parked mind is a pointer: nothing moves.
//! - [`Dedicated`]: a discrete card (the 5090). A parked mind's KV is cached out to host memory, and binding her
//!   copies it back over the link.

use std::time::Duration;

/// A prefill chunk: `tokens` fresh tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prefill {
    pub tokens: u64,
}

/// Bringing a parked mind onto the stream: `bytes` of her KV, with `ahead` of lane time the device had to fetch her
/// before the switch (the slice that just ran, during which the lane already knew who comes next).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bind {
    pub bytes: u64,
    pub ahead: Duration,
}

/// What a device charges for the lane's work. A decode step is each mind's own (measured per mind, it depends on
/// her model and context), so it stays on her load.
pub trait Device {
    /// Its name, as a report labels it.
    fn name(&self) -> &'static str;
    fn prefill(&self, ask: Prefill) -> Duration;
    fn bind(&self, ask: Bind) -> Duration;
    /// Her KV's whole copy onto the device, however much of it a slice hid: what the lane measures as her fetch.
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
    fn copy(&self, _: u64) -> Duration {
        Duration::ZERO
    }
    /// A pointer: nothing moves (the recurrent switch went 3.16 ms to 0.01 ms when it became one, 2026-11-25).
    fn bind(&self, _: Bind) -> Duration {
        Duration::ZERO
    }
}

/// How a discrete card brings a parked mind back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fetch {
    /// A copy the switch waits on, whole (today's fork: no verb overlaps it).
    Synchronous,
    /// Pinned async DMA started when the lane knows who comes next, overlapped with the running slice; the switch
    /// waits only for what the slice did not cover (THE-LANE-ON-SAGE.md: "pinned async DMA overlapped with decode").
    Prefetch,
}

/// A discrete card: a parked mind's KV lives in host memory and comes back over the link.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dedicated {
    pub prefill: PrefillRate,
    /// Host to device, measured (pinned DMA).
    pub link_bytes_per_s: f64,
    pub fetch: Fetch,
}

impl Device for Dedicated {
    fn name(&self) -> &'static str {
        match self.fetch {
            Fetch::Synchronous => "dedicated, synchronous fetch",
            Fetch::Prefetch => "dedicated, prefetch",
        }
    }
    fn prefill(&self, ask: Prefill) -> Duration {
        self.prefill.of(ask)
    }
    fn copy(&self, bytes: u64) -> Duration {
        Duration::from_secs_f64(bytes as f64 / self.link_bytes_per_s.max(f64::MIN_POSITIVE))
    }
    fn bind(&self, ask: Bind) -> Duration {
        let copy = self.copy(ask.bytes);
        match self.fetch {
            Fetch::Synchronous => copy,
            Fetch::Prefetch => copy.saturating_sub(ask.ahead),
        }
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
    pub fn rtx_5090(link_bytes_per_s: f64, fetch: Fetch) -> Self {
        Self { prefill: PrefillRate { tokens_per_s: 16_031.0 / 42.296 }, link_bytes_per_s, fetch }
    }
}
