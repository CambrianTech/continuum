//! Record: what the engine did, measured and published off the engine thread (CONTINUUM-ENGINE
//! 4.16). The engine hands one [`Fact`] per step and per finished turn to its outbox and goes on.
//! This stage merges the steps into busy spans over a window and publishes the [`EngineSnapshot`]:
//! the engine's busy share and its longest idle gap. The gate is at least 800 permille busy and no
//! gap over 2,000 ms while a mind is resident. It also writes each turn's probe row.
//!
//! A step's span is the engine thread's time on it (bind, prefill chunk or token, including the
//! wait for the device), so the share is the engine's occupancy. A window is one turn: it closes
//! when her turn ends, so a fully idle engine reports its gap with the next turn, measured exactly.

use std::time::{Duration, Instant};

use continuum_flow::{Qos, Stage};
use continuum_persona_engine::protocol::EngineSnapshot;
use tokio::sync::watch;

/// The busy share of `length`, in permille. A step that straddled the window's start counts only up
/// to the window's length, so the share never passes 1000.
fn busy_permille(busy: Duration, length: Duration) -> u16 {
    if length.is_zero() {
        return 0;
    }
    let share = busy.min(length).as_micros() * 1000 / length.as_micros();
    share as u16 // at most 1000, by the min above
}

/// A gap in whole milliseconds; one longer than u32 ms (49 days) reads as the longest a snapshot
/// can say.
fn saturating_ms(gap: Duration) -> u32 {
    gap.as_millis().min(u128::from(u32::MAX)) as u32 // at most u32::MAX by the min: 49 days, which the gate fails either way
}

/// A span in whole milliseconds, the one conversion every turn row uses. Past u64 ms (585 million
/// years) it saturates.
pub(super) fn whole_ms(span: Duration) -> u64 {
    span.as_millis().min(u128::from(u64::MAX)) as u64 // at most u64::MAX by the min, unreachable for a real span
}

/// What the engine reports.
pub(super) enum Fact {
    /// The engine worked from `started` to `ended`.
    Busy { started: Instant, ended: Instant },
    /// A turn reached a transition (admitted, first frame).
    Transition(super::transition::Transition),
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
    pub shared_prefix_tokens: u64,
    pub start: &'static str,
    pub boundary: Option<u64>,
    pub learned_boundary: Option<u64>,
    pub checkpoint_ms: u64,
    pub wait_ms: Option<u64>,
    pub longest_service_gap_ms: u64,
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
            shared_prefix_tokens = self.shared_prefix_tokens,
            start = self.start,
            boundary = ?self.boundary,
            learned_boundary = ?self.learned_boundary,
            checkpoint_ms = self.checkpoint_ms,
            wait_ms = ?self.wait_ms,
            longest_service_gap_ms = self.longest_service_gap_ms,
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
        let snapshot = EngineSnapshot {
            incarnation: self.incarnation,
            frame: self.steps,
            observed_at_ms: continuum_base::time::EpochMs::now().millis(),
            gpu_busy_permille: busy_permille(self.busy, length),
            longest_idle_ms: saturating_ms(self.longest_idle),
            minds: Vec::new(),
        };
        continuum_base::probe!(
            class = "persona.engine.occupancy",
            busy_permille = u64::from(snapshot.gpu_busy_permille),
            longest_idle_ms = u64::from(snapshot.longest_idle_ms),
            steps = self.steps,
            window = ?length,
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

    async fn handle(&mut self, fact: Fact) {
        match fact {
            Fact::Busy { started, ended } => {
                self.steps += 1;
                let started = started.max(self.last_end); // steps arrive in order; overlap counts once
                self.longest_idle = self.longest_idle.max(started.saturating_duration_since(self.last_end));
                self.busy += ended.saturating_duration_since(started);
                self.last_end = self.last_end.max(ended);
            }
            // A window is one turn: from the last turn's end through this one's last step, so the
            // idle gap before her turn counts against the share as much as the work in it.
            Fact::Transition(transition) => transition.write(),
            Fact::Turn(row) => {
                row.write();
                self.close_window(self.last_end);
            }
        }
    }
}
