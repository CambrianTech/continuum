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
//! - `queue`    — the persona's staged UNREAD depth summed across its
//!   `(persona, room)` entries in the shared digest ready-buffer — the honest
//!   revival of the legacy tile's QUE bar (`PersonaInbox.size()` in the old
//!   Node core). Always radiated, 0 included: the reference tile draws QUE as
//!   an empty track at idle, never a missing row.
//! - `genome`   — the count of paged-in LoRA genes (`genome().len()`); omitted
//!   for a base persona with none (an honest empty, never a fabricated bar).
//!   The gene NAMES ride the same update's `genes` list so the tile can name
//!   each lit segment, not just count them.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use uuid::Uuid;

use continuum_positron::Loadout;

use crate::cognition::channel_digest_region::DigestBuffer;
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
/// Staged unread elements that read as a FULL queue bar. The legacy inbox
/// surfaced load as `size / maxSize`; here the digest region stages up to one
/// channel window per (persona, room) and a persona several rooms behind with
/// ~8 unread turns is visibly saturated — the bar is a load glance, not a
/// precision gauge (the raw count rides the tooltip via the meter value).
const QUE_FULL_SCALE_UNREAD: usize = 8;

/// The ACT PULSE: per-persona executed-act counter, bumped by the act-observe
/// path each time a tool act completes. A HELD-WORK turn is one long service
/// cycle — the cycle-delta reads 0 exactly while she works hardest (Joel,
/// 2026-08-31: "AIs look still greyed" during a live benchmark round). Acts
/// are the true thinking-right-now signal inside a turn; activity radiates the
/// LOUDER of the two deltas.
static ACT_PULSE: std::sync::LazyLock<std::sync::Mutex<HashMap<Uuid, u64>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

/// Record `n` completed acts for `persona` — called by the act-observe apply
/// path; cheap (one map bump under a short lock, no allocation on the hot path
/// beyond the entry).
/// FACULTY PULSES — the axes the persona HUD names (`reason`, `recall`, `focus`;
/// see `brainRegions` in the semantic layer). Each is a timestamp (+ count) the
/// emitter reads with a freshness window: the region lights when the faculty
/// fires and fades as the window elapses. Recorded at the ONE seam each event
/// already passes — lane acquired (reasoning), recall surfaced (memory), a
/// directed turn admitted (focus). Before 2026-09-04 the emitter radiated
/// `activity/queue/tps/pfx` while the HUD asked for `reason/recall/act/speed/
/// focus`: four of five regions read "awaiting signal" forever.
static FACULTY_PULSE: std::sync::LazyLock<
    std::sync::Mutex<HashMap<(Uuid, &'static str), (u64, std::time::Instant)>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

fn record_faculty(persona: Uuid, axis: &'static str, n: u64) {
    let mut pulse = FACULTY_PULSE.lock().unwrap_or_else(|e| e.into_inner());
    pulse.insert((persona, axis), (n, std::time::Instant::now()));
}

/// A deliberation lane was acquired — she is reasoning now.
pub fn record_reasoning(persona: Uuid) {
    record_faculty(persona, "reason", 1);
}

/// `n` memories surfaced into this turn.
pub fn record_recall(persona: Uuid, n: u64) {
    record_faculty(persona, "recall", n);
}

/// A DIRECTED turn was admitted — someone addressed her (a human, an agent,
/// or a peer naming her).
pub fn record_focus(persona: Uuid) {
    record_faculty(persona, "focus", 1);
}

/// Level for a faculty pulse: full while fresh, fading linearly to 0 over the
/// window; `None` when nothing fired within it (an honest "awaiting").
fn faculty_level(persona: Uuid, axis: &'static str, window: Duration, full_scale: u64) -> Option<u8> {
    let pulse = FACULTY_PULSE.lock().unwrap_or_else(|e| e.into_inner());
    let (n, at) = pulse.get(&(persona, axis))?;
    let age = at.elapsed();
    if age >= window {
        return None;
    }
    let fresh = 1.0 - age.as_secs_f64() / window.as_secs_f64();
    let base = pct_u64(*n, full_scale) as f64;
    Some((base * fresh).round().clamp(0.0, 100.0) as u8)
}

const REASON_WINDOW: Duration = Duration::from_secs(150);
const RECALL_WINDOW: Duration = Duration::from_secs(90);
const FOCUS_WINDOW: Duration = Duration::from_secs(120);
const RECALL_FULL_SCALE: u64 = 6;

/// The last vitals sampled per persona — what `persona/vitals` answers, so a
/// page opened cold (a deep link to a citizen who is not in the viewer's focused
/// room) reads the SAME map the roster tiles draw, from the one emitter.
static LAST_VITALS: std::sync::LazyLock<
    std::sync::Mutex<HashMap<Uuid, (BTreeMap<String, u8>, std::time::Instant)>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

/// The most recent vitals map for `persona` and how long ago it was sampled.
pub fn last_vitals(persona: Uuid) -> Option<(BTreeMap<String, u8>, Duration)> {
    let last = LAST_VITALS.lock().unwrap_or_else(|e| e.into_inner());
    last.get(&persona).map(|(v, at)| (v.clone(), at.elapsed()))
}

fn remember_vitals(persona: Uuid, vitals: &BTreeMap<String, u8>) {
    let mut last = LAST_VITALS.lock().unwrap_or_else(|e| e.into_inner());
    last.insert(persona, (vitals.clone(), std::time::Instant::now()));
}

pub fn record_acts(persona: Uuid, n: u64) {
    let mut pulse = ACT_PULSE.lock().unwrap_or_else(|e| e.into_inner());
    *pulse.entry(persona).or_insert(0) += n;
}

/// Executed-acts-per-interval that read as a FULL activity bar (a solve turn
/// lands ~1-3 acts per generation; 4 in a 2s window is flat-out).
const ACT_PULSE_FULL_SCALE: u64 = 4;

/// THE SPEED PULSE: per-persona decode + prefill tokens/sec from the last
/// completed generation (llama-server's own `timings`), recorded by the
/// inference adapter at stream close. Radiated as `tps`/`pfx` vitals while
/// fresh — the roster tile draws them as micro-speedometers (Joel:
/// "speedometers not massive text; thin lines and meters").
static SPEED_PULSE: std::sync::LazyLock<
    std::sync::Mutex<HashMap<Uuid, (f64, f64, std::time::Instant)>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

/// Record the last generation's speeds for `persona` (uuid-string; non-uuid
/// callers — CLI, probes — are silently not a tile and skip).
pub fn record_speed(persona: &str, decode_tps: f64, prefill_tps: f64) {
    let Ok(id) = Uuid::parse_str(persona) else {
        return;
    };
    let mut pulse = SPEED_PULSE.lock().unwrap_or_else(|e| e.into_inner());
    pulse.insert(id, (decode_tps, prefill_tps, std::time::Instant::now()));
}

/// A speed sample older than this stops radiating — a stale needle is a lie.
const SPEED_FRESH: Duration = Duration::from_secs(20);
/// Decode t/s that reads as a pegged needle (Ornith depth-tax band tops ~40).
const TPS_FULL_SCALE: u64 = 40;
/// Prefill/ingest t/s that reads as pegged (M-series ingest ~300-800).
const PFX_FULL_SCALE: u64 = 800;

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
    digests: &DigestBuffer,
    last_ticks: &mut HashMap<Uuid, u64>,
) -> Vec<PersonaVitalsUpdate> {
    // One snapshot of the staged digests for the whole sweep — the per-persona
    // unread sum reads this Vec, not the DashMap N times.
    let staged = digests.entries();
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
        // The act pulse (held-work turns): drain this persona's executed-act
        // count since the last sample and let the LOUDER signal drive the bar.
        let act_pulse = {
            let mut pulse = ACT_PULSE.lock().unwrap_or_else(|e| e.into_inner());
            pulse.remove(&id).unwrap_or(0)
        };

        // QUEUE: the staged unread depth across every channel the digest
        // region pre-staged for this persona — the legacy QUE bar's honest
        // revival (perceived-but-not-yet-processed work). Always radiated,
        // 0 included: the reference tile draws QUE as an empty track at idle.
        let queued: usize = staged
            .iter()
            .filter(|((persona, _room), _)| *persona == id)
            .map(|(_, digest)| digest.unread().len())
            .sum();

        // Speed needles: radiate while fresh, drop when stale (honest-absent).
        let speed = {
            let pulse = SPEED_PULSE.lock().unwrap_or_else(|e| e.into_inner());
            pulse
                .get(&id)
                .filter(|(_, _, at)| at.elapsed() < SPEED_FRESH)
                .map(|(d, p, _)| (*d, *p))
        };
        let genome = cycle.genome();
        let mut vitals = BTreeMap::new();
        vitals.insert(
            "activity".to_string(),
            pct_u64(delta, ACT_FULL_SCALE_TICKS)
                .max(pct_u64(act_pulse, ACT_PULSE_FULL_SCALE)),
        );
        vitals.insert(
            "queue".to_string(),
            pct_usize(queued, QUE_FULL_SCALE_UNREAD),
        );
        if let Some((decode, prefill)) = speed {
            let tps = pct_u64(decode as u64, TPS_FULL_SCALE);
            vitals.insert("tps".to_string(), tps);
            // `speed` is the HUD's name for the same axis (CNS detail row).
            vitals.insert("speed".to_string(), tps);
            vitals.insert("pfx".to_string(), pct_u64(prefill as u64, PFX_FULL_SCALE));
        }
        // The motor region reads `act` — the act pulse alone, not blended with
        // the cycle tick like `activity` is.
        if act_pulse > 0 {
            vitals.insert("act".to_string(), pct_u64(act_pulse, ACT_PULSE_FULL_SCALE));
        }
        if let Some(v) = faculty_level(id, "reason", REASON_WINDOW, 1) {
            vitals.insert("reason".to_string(), v);
        }
        if let Some(v) = faculty_level(id, "recall", RECALL_WINDOW, RECALL_FULL_SCALE) {
            vitals.insert("recall".to_string(), v);
        }
        if let Some(v) = faculty_level(id, "focus", FOCUS_WINDOW, 1) {
            vitals.insert("focus".to_string(), v);
        }
        // Omit genome entirely when the persona has none paged in — an
        // honest missing meter, not a 0% fabricated one.
        if !genome.is_empty() {
            vitals.insert(
                "genome".to_string(),
                pct_usize(genome.len(), GEN_FULL_SCALE_GENES),
            );
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

        // #266 KV REUSE: the fraction of this persona's prompt tokens the lane
        // served from cache instead of re-prefilling. Prefill is ~96% of persona
        // compute, so this meter reads "how much of her thinking is being paid for
        // twice". High = her identity+tool prefix stayed resident; low = the lane is
        // re-encoding the same thousands of tokens every act.
        //
        // Omitted entirely until a generation has actually reported timings — a
        // citizen who has not spoken radiates NO meter rather than a fabricated 0%,
        // the same honest-empty rule as a dark cognition axis and an un-paged genome.
        // That distinction matters here more than elsewhere: 0% is also the value of
        // a REAL total-miss, so fabricating it on an idle persona would manufacture
        // the exact alarm this meter exists to raise.
        if let Some(rate) = cycle.kv_reuse() {
            // `rate` is cached/(cached+prefill) so it cannot exceed 1.0; the clamp is
            // belt-and-braces against a future accounting change silently producing a
            // >100 that would wrap the u8 meter into a small number and read as HEALTHY.
            vitals.insert(
                "kv_reuse".to_string(),
                (rate * 100.0).round().clamp(0.0, 100.0) as u8,
            );
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
            // The NAMES of the paged-in genes, in page-in order — what lets
            // the tile's genome segments carry a real tooltip per slot
            // instead of an anonymous count. Empty for a base persona.
            genes: genome.into_iter().map(|g| g.name).collect(),
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
        // Dedup on the WHOLE radiated readout — vitals, loadout AND genes.
        // Loadout moves only on a model re-home; genes on a page-in/out swap
        // (which can keep the COUNT stable while the names change). Keying
        // dedup on all three means any of them moving still radiates.
        let mut last_emitted: HashMap<Uuid, (BTreeMap<String, u8>, Option<Loadout>, Vec<String>)> =
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
                let digests = crate::cognition::channel_substrate::global_channel_digest_buffer();
                sample_vitals(&registry, &digests, &mut last_ticks)
            }));
            match sampled {
                Ok(updates) => {
                    for update in updates {
                        remember_vitals(update.member_id, &update.vitals);
                        // Change-dedup: a stable persona radiates nothing (vitals,
                        // loadout AND genes unchanged).
                        if last_emitted.get(&update.member_id).map(|(v, l, g)| {
                            v == &update.vitals && l == &update.loadout && g == &update.genes
                        }) == Some(true)
                        {
                            continue;
                        }
                        last_emitted.insert(
                            update.member_id,
                            (
                                update.vitals.clone(),
                                update.loadout.clone(),
                                update.genes.clone(),
                            ),
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
    // what this catches: a faculty pulse lights its axis while fresh and reads
    // ABSENT (not zero) once the window elapses — the HUD's "awaiting signal" is
    // an honest no-signal, never a stale full bar. Recorded at the seams the
    // deliberation/recall/admission paths already pass.
    #[test]
    fn a_faculty_pulse_lights_then_goes_absent() {
        let p = Uuid::from_u128(0x77);
        assert!(faculty_level(p, "reason", Duration::from_secs(60), 1).is_none());
        record_reasoning(p);
        assert_eq!(faculty_level(p, "reason", Duration::from_secs(60), 1), Some(100));
        record_recall(p, 3);
        assert_eq!(faculty_level(p, "recall", Duration::from_secs(60), 6), Some(50));
        assert!(faculty_level(p, "recall", Duration::from_millis(0), 6).is_none());
        record_focus(p);
        assert_eq!(faculty_level(p, "focus", Duration::from_secs(60), 1), Some(100));
    }

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

        let updates = sample_vitals(&registry, &DigestBuffer::new(), &mut last_ticks);
        assert_eq!(
            updates.len(),
            1,
            "one resident persona → one radiated update"
        );
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
        assert_eq!(
            update.vitals.get("queue"),
            Some(&0),
            "queue is ALWAYS radiated (0 with nothing staged — the reference tile's empty QUE track)"
        );
        assert!(
            !update.vitals.contains_key("genome"),
            "no paged-in genes → no genome meter (honest-absent)"
        );
        assert!(update.genes.is_empty(), "no paged-in genes → no gene names");
        let loadout = update
            .loadout
            .as_ref()
            .expect("a bound cycle radiates a loadout");
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

    // what this catches: the QUE revival — the radiator sums the persona's
    // staged UNREAD depth across its (persona, room) digest entries and scales
    // it against QUE_FULL_SCALE_UNREAD. If the sum ever filters on the wrong
    // key side (room instead of persona) or reads elements instead of
    // unread(), the tile's QUE bar silently lies about load.
    #[test]
    fn queue_vital_sums_staged_unread_across_the_personas_channels() {
        use crate::cognition::channel_digest::ChannelDigestBuilder;
        use crate::cognition::channel_element::ChannelElementCache;
        use crate::cognition::embedding::{CachingEmbeddingProvider, LexicalEmbedder};
        use crate::runtime::ready_buffer::ReadyBuffer as _;
        use airc_lib::RoomId;

        let peer_id = Uuid::new_v4();
        let registry = registry_with(peer_id);

        // Stage digests in TWO rooms for this persona (3 + 2 unread) and one
        // for a DIFFERENT persona (must not leak into the sum).
        let cache = Arc::new(ChannelElementCache::new(Arc::new(
            CachingEmbeddingProvider::new(Arc::new(LexicalEmbedder::new())),
        )));
        let builder = ChannelDigestBuilder::new(cache);
        let digests = DigestBuffer::new();
        let stage = |persona: Uuid, room: RoomId, texts: &[&str]| {
            let events = texts
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    crate::cognition::channel_digest::test_event_in(room, t, i as u64 + 1)
                })
                .collect();
            let digest = builder.build_from_events(persona, room.as_uuid(), events, 0, 0);
            digests.publish((persona, room.as_uuid()), Arc::new(digest));
        };
        stage(peer_id, RoomId::new(), &["a", "b", "c"]);
        stage(peer_id, RoomId::new(), &["d", "e"]);
        stage(Uuid::new_v4(), RoomId::new(), &["not", "mine"]);

        let updates = sample_vitals(&registry, &digests, &mut HashMap::new());
        let update = &updates[0];
        // 5 unread of QUE_FULL_SCALE_UNREAD (8) → 62%.
        assert_eq!(
            update.vitals.get("queue"),
            Some(&62),
            "queue must sum THIS persona's staged unread (3+2 of 8 = 62%), other personas excluded"
        );
    }

    // what this catches: gene NAMES ride the radiated update in page-in order,
    // and the genome meter appears alongside — the tile's segment tooltips are
    // real adapter names, never fabricated labels.
    #[test]
    fn paged_in_genes_radiate_their_names() {
        let peer_id = Uuid::new_v4();
        let registry = registry_with(peer_id);
        let cycle = registry.get(&peer_id).unwrap();
        cycle.page_in(vec![
            crate::ai::types::ActiveAdapterRequest {
                name: "rust-hands".into(),
                path: "/tmp/rust-hands.gguf".into(),
                domain: "code".into(),
                scale: 1.0,
            },
            crate::ai::types::ActiveAdapterRequest {
                name: "tool-fluency".into(),
                path: "/tmp/tool-fluency.gguf".into(),
                domain: "tools".into(),
                scale: 1.0,
            },
        ]);

        let updates = sample_vitals(&registry, &DigestBuffer::new(), &mut HashMap::new());
        let update = &updates[0];
        assert_eq!(
            update.genes,
            vec!["rust-hands".to_string(), "tool-fluency".to_string()],
            "gene names radiate in page-in order"
        );
        assert_eq!(
            update.vitals.get("genome"),
            Some(&33),
            "2 of 6 full-scale genes → 33% genome meter, consistent with the names list"
        );
    }
}
