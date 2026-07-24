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

use continuum_positron::Loadout;

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

/// One tick's radiated readouts from `registry` — the SAMPLE half of the
/// radiator, extracted from the spawn loop so its two contracts are pinned by
/// unit tests without a bus or the process-global registry:
///
/// - **The id-join**: `PersonaVitalsUpdate.member_id` IS the registry key, which
///   the live spawn path sets to the persona's **airc peer id**
///   (`supervisor`: `register_from_cfg` with `persona_id: identity.peer_id.as_uuid()`)
///   — the SAME uuid space `roster_slot_from_member` writes into
///   `RosterSlotView.member_id`. That equality is what lets
///   `positron_source::apply_vitals` fold a radiated readout into the member's
///   roster slot by id. If either side ever keys a different id space, every
///   live tile silently loses its vitals (the exact regression class of card
///   2661a1b1).
/// - **The loadout**: model id + effective window from the live binding, param
///   count resolved from the registry row by that id.
///
/// Change-dedup deliberately stays in the caller (it is emit policy, not a
/// sampling fact), so this returns EVERY resident persona's current readout.
pub(crate) fn sample_vitals(
    registry: &persona_workspace::PersonaWorkspaceRegistry,
    last_ticks: &mut HashMap<Uuid, u64>,
) -> Vec<PersonaVitalsUpdate> {
    let mut updates = Vec::new();
    for (id, _name) in registry.roster() {
        let Some(cycle) = registry.get(&id) else {
            continue;
        };
        // ACTIVITY: the cycle-tick delta since we last sampled. First
        // sample seeds `last` = now → delta 0 ("no history yet").
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

        // LOADOUT: the model backing this persona — the display strip
        // (`model · size · ctx`). Model id + EFFECTIVE window come from
        // the live binding (`model_loadout` resolves a None binding id to the
        // adapter's default — the served model); the parameter COUNT is resolved
        // from the registry row by that id (GGUF-hydrated #74) — NEVER sniffed
        // from the model name ([[models-are-infinite-decide-on-capability-not-name]]).
        // A `0` param count (unhydrated row) and a `0` window collapse
        // to absent — honest-unknown, never a fabricated `0B`/`0 ctx`.
        let loadout = cycle.model_loadout().map(|(model_id, ctx)| {
            let params = model_id
                .as_deref()
                .and_then(|mid| crate::model_registry::try_global().and_then(|r| r.model(mid)))
                .map(|m| m.parameter_count)
                .filter(|&c| c > 0);
            Loadout {
                model: model_id,
                params,
                context_window: (ctx > 0).then_some(ctx),
            }
        });

        updates.push(PersonaVitalsUpdate {
            member_id: id,
            vitals,
            loadout,
        });
    }
    updates
}

/// Spawn the radiator on `rt`. Every [`EMIT_INTERVAL`] it samples each resident
/// persona's live cognition tempo + genome load and publishes `persona:vitals`
/// for those whose readouts changed. Runs for the process lifetime.
pub fn spawn_vitals_emitter(rt: &tokio::runtime::Handle, bus: Arc<MessageBus>) {
    rt.spawn(async move {
        let mut ticker = tokio::time::interval(EMIT_INTERVAL);
        let mut last_ticks: HashMap<Uuid, u64> = HashMap::new();
        // Dedup on the WHOLE radiated readout — vitals AND loadout. Loadout
        // moves only on a model re-home; keying dedup on both means a re-home
        // with otherwise-stable vitals still radiates the new loadout.
        let mut last_emitted: HashMap<Uuid, (BTreeMap<String, u8>, Option<Loadout>)> =
            HashMap::new();
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
                sample_vitals(&registry, &mut last_ticks)
            }));
            match sampled {
                Ok(updates) => {
                    for update in updates {
                        // Change-dedup: a stable persona radiates nothing (vitals
                        // AND loadout unchanged).
                        if last_emitted
                            .get(&update.member_id)
                            .map(|(v, l)| v == &update.vitals && l == &update.loadout)
                            == Some(true)
                        {
                            continue;
                        }
                        last_emitted.insert(
                            update.member_id,
                            (update.vitals.clone(), update.loadout.clone()),
                        );
                        match serde_json::to_value(&update) {
                            Ok(payload) => bus.publish_async_only(PERSONA_VITALS, payload),
                            Err(e) => {
                                tracing::warn!(
                                    persona = %update.member_id,
                                    error = %e,
                                    "persona vitals serialize failed"
                                )
                            }
                        }
                    }
                }
                Err(_) => {
                    tracing::warn!(
                        "vitals radiator tick panicked (likely a poisoned registry lock) — \
                         skipping, retrying next interval"
                    );
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
    use crate::cognition::persona_workspace::{PersonaBrainConfig, PersonaWorkspaceRegistry};
    use crate::persona::admission_state::AdmissionState;
    use crate::persona::recall_metadata::RecallMetadataRegistry;

    /// A minimal resident persona in a LOCAL registry (never the process
    /// global), keyed by `peer_id` — the same uuid the spawn path would take
    /// from `identity.peer_id.as_uuid()`.
    fn registry_with(peer_id: Uuid) -> std::sync::Arc<PersonaWorkspaceRegistry> {
        let registry = std::sync::Arc::new(PersonaWorkspaceRegistry::new());
        registry.get_or_build(PersonaBrainConfig {
            persona_id: peer_id,
            persona_name: "Asha".to_string(),
            system_prompt: "You are Asha.".to_string(),
            admission: std::sync::Arc::new(AdmissionState::new(std::sync::Arc::new(
                RecallMetadataRegistry::new(),
            ))),
            adapter: std::sync::Arc::new(HeuristicInferenceAdapter::new()),
            capacity: None,
            grounding_sources: Vec::new(),
            embedder: None,
            tool_executor: None,
            context_window: crate::cognition::serving_plan::MIN_SERVE_CTX,
            defer_recall: false,
            defer_grounding: false,
            suppress_recall: false,
        });
        registry
    }

    // what this catches: the roster id-join — the radiator MUST key
    // `PersonaVitalsUpdate.member_id` by the registry key, which the live spawn
    // path sets to the persona's airc peer id (the SAME uuid
    // `roster_slot_from_member` puts in `RosterSlotView.member_id`). If the
    // radiator ever keys a different id space, `positron_source::apply_vitals`
    // folds into nothing and every live tile silently loses vitals + loadout
    // (regression class of card 2661a1b1). Also pins the always-on `activity`
    // meter (present even at 0 — the one honest always-visible bar) and the
    // loadout's effective-model resolution (a boot binding with `model: None`
    // still names the served model via the adapter default).
    #[test]
    fn sampled_vitals_key_the_registry_peer_id_and_carry_the_effective_loadout() {
        let peer_id = Uuid::new_v4();
        let registry = registry_with(peer_id);
        let mut last_ticks = HashMap::new();

        let updates = sample_vitals(&registry, &mut last_ticks);
        assert_eq!(updates.len(), 1, "one resident persona → one radiated update");
        let update = &updates[0];
        assert_eq!(
            update.member_id, peer_id,
            "member_id must be the registry key (the airc peer id the roster slot carries)"
        );
        assert_eq!(
            update.vitals.get("activity"),
            Some(&0),
            "activity is ALWAYS radiated (0 at idle — honest, and the tile's always-visible bar)"
        );
        assert!(
            !update.vitals.contains_key("genome"),
            "no paged-in genes → no genome meter (honest-absent)"
        );
        let loadout = update.loadout.as_ref().expect("a bound cycle radiates a loadout");
        let expected_model = registry
            .get(&peer_id)
            .unwrap()
            .current_adapter()
            .unwrap()
            .default_model()
            .to_string();
        assert_eq!(
            loadout.model.as_deref(),
            Some(expected_model.as_str()),
            "a None binding id resolves to the adapter's default — the served model name"
        );
        assert_eq!(
            loadout.context_window,
            Some(crate::cognition::serving_plan::MIN_SERVE_CTX),
            "the effective served window rides the loadout"
        );
    }
}
