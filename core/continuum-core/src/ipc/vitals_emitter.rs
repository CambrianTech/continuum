//! Persona-vitals radiator — the EMIT half of design B (the projection FOLD
//! lives in [`crate::ipc::positron_source`]).
//!
//! A periodic task samples the live `WorkspaceCycle` registry — where resident
//! personas actually run — and publishes a `persona:vitals` event per persona
//! whose readouts CHANGED; the chat projection folds each into that member's
//! roster slot by id. Personas emit, the projection folds, so the
//! persona-agnostic presence emitter never learns about personas
//! ([[grid-distributed-cognition]]). Mirrors the own-task + `interval` + bus
//! shape of [`crate::ipc::positron_presence::spawn_presence_emitter`].
//!
//! **Vitals = what the living brain actually computes** (NOT a service_loop-era
//! `PersonaState.energy` the `WorkspaceCycle` never tracks):
//! - `activity` — the `cycle_count()` DELTA over the interval, the true
//!   "thinking-right-now" pulse (idle = 0, busy = high).
//! - `genome`   — the count of paged-in LoRA genes (`genome().len()`); omitted
//!   for a base persona with none (an honest empty, never a fabricated bar).

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use uuid::Uuid;

use crate::cognition::faculty_pulse::CognitionAxis;
use crate::cognition::persona_workspace;
use crate::ipc::positron_source::{PersonaVitalsUpdate, PERSONA_VITALS};
use crate::runtime::message_bus::MessageBus;

/// Sample cadence. Vitals drift over seconds; the change-dedup keeps an idle
/// room quiet on the bus + the chat-state revision well.
const EMIT_INTERVAL: Duration = Duration::from_secs(2);

/// Calibration: service-ticks-per-interval that read as a FULL activity bar. A
/// busy persona services several concern-ticks per `EMIT_INTERVAL`; this scales
/// the per-interval delta to `0..=100`. Tuned against the glass-boxed real tick
/// rate — NEVER guessed blind ([[never-blind-feedback-driven-iteration]]).
const ACT_FULL_SCALE_TICKS: u64 = 6;
/// Paged-in LoRA-gene count that reads as a FULL genome bar.
const GEN_FULL_SCALE_GENES: usize = 6;

fn pct_u64(v: u64, full: u64) -> u8 {
    (v.saturating_mul(100) / full.max(1)).min(100) as u8
}
fn pct_usize(v: usize, full: usize) -> u8 {
    (v.saturating_mul(100) / full.max(1)).min(100) as u8
}

/// Spawn the radiator on `rt`. Every [`EMIT_INTERVAL`] it samples each resident
/// persona's live cognition tempo + genome load and publishes `persona:vitals`
/// for those whose readouts changed. Runs for the process lifetime.
pub fn spawn_vitals_emitter(rt: &tokio::runtime::Handle, bus: Arc<MessageBus>) {
    rt.spawn(async move {
        let mut ticker = tokio::time::interval(EMIT_INTERVAL);
        let mut last_ticks: HashMap<Uuid, u64> = HashMap::new();
        let mut last_vitals: HashMap<Uuid, BTreeMap<String, u8>> = HashMap::new();
        loop {
            ticker.tick().await;
            // Wrap the (synchronous) sample in catch_unwind: the only realistic
            // panic here is a POISONED registry lock — another thread panicked
            // while holding `persona_workspace`'s `cycles` mutex — and one bad
            // tick must NOT silently kill the radiator for the whole process
            // lifetime (CONCURRENCY-STYLE-GUIDE quarantine box). Log + retry next
            // interval. No `.await` inside, so unwind-safety is straightforward.
            // (Unlike the presence emitter, there is no `presence:resync`-style
            // path: the change-dedup means a chat projection that restarts sees no
            // meter on an idle persona until its readout next changes — acceptable,
            // since a missing bar on a genuinely-idle persona is honest, not blank.)
            let sampled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let registry = persona_workspace::global();
                for (id, _name) in registry.roster() {
                    let Some(cycle) = registry.get(&id) else {
                        continue;
                    };
                    // ACTIVITY: the cycle-tick delta since we last sampled. First
                    // sample seeds `last` = now → delta 0 ("no history yet").
                    let ticks = cycle.cycle_count();
                    let delta =
                        ticks.saturating_sub(last_ticks.get(&id).copied().unwrap_or(ticks));
                    last_ticks.insert(id, ticks);

                    let genome_len = cycle.genome().len();
                    let mut vitals = BTreeMap::new();
                    vitals.insert("activity".to_string(), pct_u64(delta, ACT_FULL_SCALE_TICKS));
                    // Omit genome entirely when the persona has none paged in — an
                    // honest missing meter, not a 0% fabricated one.
                    if genome_len > 0 {
                        vitals
                            .insert("genome".to_string(), pct_usize(genome_len, GEN_FULL_SCALE_GENES));
                    }
                    // #186 COGNITION COMPASS: the decaying per-axis firing levels
                    // (Focus/Reason/Recall/Act) the cognition tick + acting seam bumped.
                    // Omit a dark (0) axis so an idle persona radiates no compass — the
                    // tile's diamond triangle stays unlit until that faculty actually
                    // fires (honest empty, never a fabricated glow). Keys are exactly
                    // what the tile's `cognitionDiamond` reads.
                    let levels = cycle.faculty_pulse().levels();
                    for (axis, level) in CognitionAxis::ALL.iter().zip(levels) {
                        if level > 0 {
                            vitals.insert(axis.vital_key().to_string(), level);
                        }
                    }

                    // Change-dedup: a stable persona radiates nothing.
                    if last_vitals.get(&id) == Some(&vitals) {
                        continue;
                    }
                    last_vitals.insert(id, vitals.clone());
                    let update = PersonaVitalsUpdate { member_id: id, vitals };
                    match serde_json::to_value(&update) {
                        Ok(payload) => bus.publish_async_only(PERSONA_VITALS, payload),
                        Err(e) => {
                            tracing::warn!(persona = %id, error = %e, "persona vitals serialize failed")
                        }
                    }
                }
            }));
            if sampled.is_err() {
                tracing::warn!(
                    "vitals radiator tick panicked (likely a poisoned registry lock) — \
                     skipping, retrying next interval"
                );
            }
        }
    });
}
