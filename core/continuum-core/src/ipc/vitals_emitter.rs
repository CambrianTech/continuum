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
            let registry = persona_workspace::global();
            for (id, _name) in registry.roster() {
                let Some(cycle) = registry.get(&id) else {
                    continue;
                };
                // ACTIVITY: the cycle-tick delta since we last sampled. First
                // sample seeds `last` = now → delta 0 (honest "no history yet").
                let ticks = cycle.cycle_count();
                let delta = ticks.saturating_sub(last_ticks.get(&id).copied().unwrap_or(ticks));
                last_ticks.insert(id, ticks);

                let genome_len = cycle.genome().len();
                let mut vitals = BTreeMap::new();
                vitals.insert("activity".to_string(), pct_u64(delta, ACT_FULL_SCALE_TICKS));
                // Omit genome entirely when the persona has none paged in — an
                // honest missing meter, not a 0% fabricated one.
                if genome_len > 0 {
                    vitals.insert("genome".to_string(), pct_usize(genome_len, GEN_FULL_SCALE_GENES));
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
        }
    });
}
