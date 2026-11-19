//! Record: what the engine did, measured and published off the engine thread (CONTINUUM-ENGINE
//! 4.16). The engine hands one [`Fact`] per step and per finished turn to its outbox and goes on.
//! This stage merges the steps into busy spans over a window and publishes the [`EngineSnapshot`]:
//! the engine's busy share and its longest idle gap. The gate is at least 800 permille busy and no
//! gap over 2,000 ms while a mind is resident. It also writes each turn's probe row.
//!
//! A step's span is the engine thread's time on it (bind, prefill chunk or token, including the
//! wait for the device), so the share is the engine's occupancy. A window closes on the first step
//! after it ends; a fully idle engine reports its gap with its next step, measured exactly.

use std::time::{Duration, Instant};

use continuum_flow::{Qos, Stage};
use continuum_persona_engine::protocol::EngineSnapshot;
use tokio::sync::watch;

/// The window one snapshot measures.
const WINDOW: Duration = Duration::from_secs(10);

/// What the engine reports.
pub(super) enum Fact {
    /// The engine worked from `started` to `ended`.
    Busy { started: Instant, ended: Instant },
    /// A turn ended.
    Turn(TurnRow),
}

/// One finished turn: where her seconds went. Built by the engine from its own counters (no work),
/// written here.
pub(super) struct TurnRow {
    pub persona: uuid::Uuid,
    pub one_off: bool,
    pub prompt_tokens: u64,
    pub reused_tokens: u64,
    pub start: &'static str,
    pub boundary: u64,
    pub checkpoint_ms: u64,
    pub wait_ms: u64,
    pub switches: u64,
    pub switch_ms: u64,
    pub prefill_ms: u64,
    pub decode_ms: u64,
    pub forward_ms: u64,
    pub graphs_reused: u64,
    pub gpu_ms: u64,
    pub sample_ms: u64,
    pub generated: u64,
    pub total_ms: u64,
}

impl TurnRow {
    fn write(self) {
        continuum_base::probe!(
            class = "persona.engine.turn",
            persona = %self.persona,
            one_off = self.one_off,
            prompt_tokens = self.prompt_tokens,
            reused_tokens = self.reused_tokens,
            start = self.start,
            boundary = self.boundary,
            checkpoint_ms = self.checkpoint_ms,
            wait_ms = self.wait_ms,
            switches = self.switches,
            switch_ms = self.switch_ms,
            prefill_ms = self.prefill_ms,
            decode_ms = self.decode_ms,
            forward_ms = self.forward_ms,
            graphs_reused = self.graphs_reused,
            gpu_ms = self.gpu_ms,
            sample_ms = self.sample_ms,
            generated = self.generated,
            total_ms = self.total_ms,
            "persona engine turn"
        );
    }
}

pub(super) struct Record {
    incarnation: u64,
    steps: u64,
    window_start: Instant,
    busy: Duration,
    last_end: Instant,
    longest_idle: Duration,
    published: watch::Sender<EngineSnapshot>,
}

impl Record {
    pub(super) fn new(incarnation: u64, published: watch::Sender<EngineSnapshot>) -> Self {
        let now = Instant::now();
        Self { incarnation, steps: 0, window_start: now, busy: Duration::ZERO, last_end: now, longest_idle: Duration::ZERO, published }
    }

    /// The snapshot before any window closes.
    pub(super) fn empty(incarnation: u64) -> EngineSnapshot {
        EngineSnapshot { incarnation, frame: 0, observed_at_ms: 0, gpu_busy_permille: 0, longest_idle_ms: 0, minds: Vec::new() }
    }

    fn close_window(&mut self, now: Instant) {
        let length = now.saturating_duration_since(self.window_start);
        let permille = if length.is_zero() { 0 } else { (self.busy.as_micros() * 1000 / length.as_micros()).min(1000) };
        let snapshot = EngineSnapshot {
            incarnation: self.incarnation,
            frame: self.steps,
            observed_at_ms: continuum_base::time::EpochMs::now().millis(),
            gpu_busy_permille: u16::try_from(permille).unwrap_or(1000), // unwrap_or: clamped to 1000 above
            longest_idle_ms: u32::try_from(self.longest_idle.as_millis()).unwrap_or(u32::MAX), // unwrap_or: a gap past u32 ms reads as the longest
            minds: Vec::new(),
        };
        continuum_base::probe!(
            class = "persona.engine.occupancy",
            busy_permille = u64::from(snapshot.gpu_busy_permille),
            longest_idle_ms = u64::from(snapshot.longest_idle_ms),
            steps = self.steps,
            window_ms = u64::try_from(length.as_millis()).unwrap_or(u64::MAX), // unwrap_or: past u64 ms reads as the most
            "engine busy share and longest idle gap over one window (gate: >= 800 permille, <= 2000 ms)"
        );
        self.published.send_replace(snapshot);
        self.window_start = now;
        self.busy = Duration::ZERO;
        self.longest_idle = Duration::ZERO;
    }
}

impl Stage for Record {
    type Input = Fact;
    const QOS: Qos = Qos::Utility;
    /// One fact per engine step: seconds of steps at the M5's rate, many times over.
    const DEPTH: usize = 8192;

    async fn handle(&mut self, fact: Fact) {
        match fact {
            Fact::Busy { started, ended } => {
                self.steps += 1;
                let started = started.max(self.last_end); // steps arrive in order; overlap counts once
                self.longest_idle = self.longest_idle.max(started.saturating_duration_since(self.last_end));
                self.busy += ended.saturating_duration_since(started);
                self.last_end = self.last_end.max(ended);
                if ended.saturating_duration_since(self.window_start) >= WINDOW {
                    self.close_window(ended);
                }
            }
            Fact::Turn(row) => row.write(),
        }
    }
}
