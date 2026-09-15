//! PlacementSwitch — a persona's brain follows the fleet.
//!
//! A persona bound to a REMOTE seat (`PersonaModelOverride.remote_peer`) used to be
//! bound for the life of her adapter: when the tower went dark her breaker went cold
//! and she sat refused for the window, and when the tower came back nothing re-offered
//! her. Both moves were done BY HAND on 2026-09-07 (Kira + Mathis, the 5090). Joel,
//! 2026-09-14: "towers all going on and offline all the time while we maintain
//! persistent intelligence and ideally reestablish the dynamic grid when internet is
//! restored or tower is rebooted."
//!
//! The switch is the adapter she runs on. It holds her REMOTE lane and, built on first
//! need, her HOME adapter from the same local factory every resident uses; the seat she
//! is on is one atomic. [`decide`] is the pure rule (unit-tested); [`follow_the_fleet`]
//! applies it once per grid tick from the capacity ledger's beacon ages and the
//! breaker's cold state. Every move is a probe AND a line for the org room — this is a
//! decided, receipted, reversible move, not the silent substitution
//! [[fallbacks-are-illegal-fail-loud]] forbids. A node with no local lane PARKS her
//! (probe, no move) instead of downgrading.
//!
//! Return to the remote seat needs no synthetic probe: the beacon being fresh and the
//! breaker warm is the admission; her next real turn is the proof, and if it dies at
//! the deadline three times the breaker trips and she falls home again after the
//! cooldown. Hysteresis: one move per persona per [`MOVE_COOLDOWN_MS`].

use crate::ai::adapter::{
    AIProviderAdapter, AdapterCapabilities, ApiStyle, GenerationChunk, InferenceDevice,
    LoRAAdapterInfo, LoRACapabilities,
};
use crate::ai::types::{
    EmbeddingRequest, EmbeddingResponse, HealthStatus, ModelInfo, TextGenerationRequest,
    TextGenerationResponse,
};
use crate::inference::airc_remote::AircRemoteInferenceAdapter;
use crate::persona::inference_profile::PersonaInferenceProfile;
use crate::persona::supervisor::PersonaAdapterFactory;
use async_trait::async_trait;
use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, LazyLock};
use uuid::Uuid;

/// A remote seat whose beacon is older than this is DARK for placement purposes —
/// nine missed 10 s beacons. Much shorter than the fleet fold's 6 h "stale": a lane
/// decision is per turn, a fleet verdict is per day.
pub const REMOTE_SEAT_SILENT_MS: u64 = 90_000;
/// A seat heard this recently is BACK; three beacons in a row.
pub const REMOTE_SEAT_FRESH_MS: u64 = 30_000;
/// At most one move per persona per cooldown — a flapping tower moves her twice an hour,
/// never twice a minute.
pub const MOVE_COOLDOWN_MS: u64 = 600_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Seat {
    Remote = 0,
    Home = 1,
}

/// What the pure rule sees.
#[derive(Debug, Clone, Copy)]
pub struct PlacementInputs {
    pub seat: Seat,
    /// `None` = the seat's beacon was never heard this boot.
    pub beacon_age_ms: Option<u64>,
    pub lane_cold: bool,
    pub local_available: bool,
    pub since_last_move_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlacementMove {
    FallHome { reason: &'static str },
    ReturnRemote,
    /// The seat is dark and this node has nothing to run her on: say so, move nothing.
    Park { reason: &'static str },
    Stay,
}

/// The rule. Pure so the four scenarios are hand-computed tests.
pub fn decide(i: PlacementInputs) -> PlacementMove {
    let dark = i.beacon_age_ms.map(|a| a > REMOTE_SEAT_SILENT_MS).unwrap_or(true); // JUSTIFIED unwrap_or: never heard = dark, by definition
    let cooled = i.since_last_move_ms >= MOVE_COOLDOWN_MS;
    match i.seat {
        Seat::Remote => {
            let reason = if i.lane_cold {
                "breaker cold: three deadline misses in a row"
            } else if dark {
                "seat silent: no capacity beacon"
            } else {
                return PlacementMove::Stay;
            };
            if !cooled {
                return PlacementMove::Stay;
            }
            if i.local_available {
                PlacementMove::FallHome { reason }
            } else {
                PlacementMove::Park { reason }
            }
        }
        Seat::Home => {
            let fresh = i.beacon_age_ms.map(|a| a < REMOTE_SEAT_FRESH_MS).unwrap_or(false); // JUSTIFIED unwrap_or: never heard = not fresh
            if fresh && !i.lane_cold && cooled {
                PlacementMove::ReturnRemote
            } else {
                PlacementMove::Stay
            }
        }
    }
}

/// The adapter a remote-bound persona runs on: her remote lane, her home adapter (built
/// on first fall-home), and the seat she is on.
pub struct PlacementSwitch {
    persona_id: Uuid,
    persona_name: String,
    peer: Uuid,
    remote: Arc<AircRemoteInferenceAdapter>,
    local: tokio::sync::OnceCell<Option<Arc<dyn AIProviderAdapter>>>,
    local_factory: Arc<dyn PersonaAdapterFactory>,
    profile: PersonaInferenceProfile,
    seat: AtomicU8,
    moved_at_ms: AtomicU64,
}

impl PlacementSwitch {
    pub fn new(
        persona_id: Uuid,
        persona_name: String,
        peer: Uuid,
        remote: Arc<AircRemoteInferenceAdapter>,
        local_factory: Arc<dyn PersonaAdapterFactory>,
        profile: PersonaInferenceProfile,
    ) -> Self {
        Self {
            persona_id,
            persona_name,
            peer,
            remote,
            local: tokio::sync::OnceCell::new(),
            local_factory,
            profile,
            seat: AtomicU8::new(Seat::Remote as u8),
            moved_at_ms: AtomicU64::new(0),
        }
    }

    pub fn seat(&self) -> Seat {
        if self.seat.load(Ordering::Relaxed) == Seat::Home as u8 { Seat::Home } else { Seat::Remote }
    }
    pub fn peer(&self) -> Uuid {
        self.peer
    }
    pub fn persona_name(&self) -> &str {
        &self.persona_name
    }
    pub fn moved_at_ms(&self) -> u64 {
        self.moved_at_ms.load(Ordering::Relaxed)
    }
    pub fn lane_cold(&self) -> bool {
        self.remote.is_cold()
    }

    /// Build (once) or fetch her home adapter. `None` = this node cannot host her.
    async fn local(&self) -> Option<&Arc<dyn AIProviderAdapter>> {
        self.local
            .get_or_init(|| async {
                match self.local_factory.build_adapter(&self.profile).await {
                    Ok(a) => Some(a),
                    Err(e) => {
                        crate::probe!(
                            class = "persona.placement.no_home_lane",
                            persona = %self.persona_name,
                            error = %e,
                            "this node cannot build a home adapter for her — she can only be parked"
                        );
                        None
                    }
                }
            })
            .await
            .as_ref()
    }

    /// Whether a home adapter exists (built) or could be built. Builds it if never tried.
    pub async fn local_available(&self) -> bool {
        self.local().await.is_some()
    }

    fn current(&self) -> &dyn AIProviderAdapter {
        match self.seat() {
            Seat::Home => match self.local.get().and_then(|l| l.as_ref()) {
                Some(l) => l.as_ref(),
                None => self.remote.as_ref(),
            },
            Seat::Remote => self.remote.as_ref(),
        }
    }

    fn set_seat(&self, seat: Seat, now_ms: u64) {
        self.seat.store(seat as u8, Ordering::Relaxed);
        self.moved_at_ms.store(now_ms, Ordering::Relaxed);
    }

    /// Apply one decided move. Returns the org-room line for a move, `None` for Stay/Park
    /// (Park is a probe only — it repeats every tick and would flood the room).
    pub async fn apply(&self, mv: &PlacementMove, now_ms: u64) -> Option<String> {
        match mv {
            PlacementMove::Stay => None,
            PlacementMove::Park { reason } => {
                crate::probe!(
                    class = "persona.placement.parked",
                    persona = %self.persona_name,
                    peer = %self.peer,
                    reason = *reason,
                    "her remote seat is dark and this node has no lane for her — parked, not downgraded"
                );
                None
            }
            PlacementMove::FallHome { reason } => {
                if self.local().await.is_none() {
                    return None;
                }
                self.set_seat(Seat::Home, now_ms);
                crate::probe!(
                    class = "persona.placement.fell_home",
                    persona = %self.persona_name,
                    peer = %self.peer,
                    reason = *reason,
                    "her remote seat went dark — her brain runs on this node until the seat beacons again"
                );
                Some(format!(
                    "[placement] {} fell home from {} ({}) — runs here until the seat beacons again",
                    self.persona_name, self.peer, reason
                ))
            }
            PlacementMove::ReturnRemote => {
                self.set_seat(Seat::Remote, now_ms);
                crate::probe!(
                    class = "persona.placement.returned",
                    persona = %self.persona_name,
                    peer = %self.peer,
                    "her remote seat is beaconing again — her brain goes back off-box; her next turn is the proof"
                );
                Some(format!(
                    "[placement] {} returned to {} — the seat beacons again; her next turn is the proof",
                    self.persona_name, self.peer
                ))
            }
        }
    }
}

#[async_trait]
impl AIProviderAdapter for PlacementSwitch {
    fn provider_id(&self) -> &str {
        self.current().provider_id()
    }
    fn name(&self) -> &str {
        self.current().name()
    }
    fn capabilities(&self) -> AdapterCapabilities {
        self.current().capabilities()
    }
    fn api_style(&self) -> ApiStyle {
        self.current().api_style()
    }
    fn warm_ahead(&self, persona: Uuid, room: Uuid) {
        self.current().warm_ahead(persona, room)
    }
    fn live_served_window(&self) -> Option<u32> {
        self.current().live_served_window()
    }
    fn default_model(&self) -> &str {
        self.current().default_model()
    }
    fn served_model_ids(&self) -> Vec<String> {
        self.current().served_model_ids()
    }
    async fn warmup(&self) -> Result<(), String> {
        self.current().warmup().await
    }
    async fn generate_stream_checked(
        &self,
        request: TextGenerationRequest,
        sink: tokio::sync::mpsc::UnboundedSender<GenerationChunk>,
    ) -> Result<TextGenerationResponse, crate::ai::inference_error::InferenceError> {
        self.current().generate_stream_checked(request, sink).await
    }
    async fn generate_text(&self, request: TextGenerationRequest) -> Result<TextGenerationResponse, String> {
        self.current().generate_text(request).await
    }
    async fn generate_stream(
        &self,
        request: TextGenerationRequest,
        sink: tokio::sync::mpsc::UnboundedSender<GenerationChunk>,
    ) -> Result<TextGenerationResponse, String> {
        self.current().generate_stream(request, sink).await
    }
    async fn create_embedding(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse, String> {
        self.current().create_embedding(request).await
    }
    async fn health_check(&self) -> HealthStatus {
        self.current().health_check().await
    }
    async fn get_available_models(&self) -> Vec<ModelInfo> {
        self.current().get_available_models().await
    }
    fn model_metadata(&self, model_id: &str) -> Option<ModelInfo> {
        self.current().model_metadata(model_id)
    }
    fn lora_capabilities(&self) -> LoRACapabilities {
        self.current().lora_capabilities()
    }
    async fn apply_lora(&self, adapter_id: &str) -> Result<(), String> {
        self.current().apply_lora(adapter_id).await
    }
    async fn remove_lora(&self, adapter_id: &str) -> Result<(), String> {
        self.current().remove_lora(adapter_id).await
    }
    fn list_lora_adapters(&self) -> Vec<LoRAAdapterInfo> {
        self.current().list_lora_adapters()
    }
    fn device_type(&self) -> InferenceDevice {
        self.current().device_type()
    }
    fn supported_model_prefixes(&self) -> Vec<&'static str> {
        self.current().supported_model_prefixes()
    }
    fn is_production_capable(&self) -> bool {
        self.current().is_production_capable()
    }
}

/// Every live switch on this node, by persona. The factory registers at bind; the grid
/// tick walks it. A despawned persona's switch stays until her id is re-bound (a stale
/// entry only ever costs one ledger read per tick).
static SWITCHES: LazyLock<DashMap<Uuid, Arc<PlacementSwitch>>> = LazyLock::new(DashMap::new);

pub fn register(switch: Arc<PlacementSwitch>) {
    SWITCHES.insert(switch.persona_id, switch);
}

pub fn switches() -> Vec<Arc<PlacementSwitch>> {
    SWITCHES.iter().map(|r| Arc::clone(r.value())).collect()
}

/// One pass: read beacon ages, decide per switch, apply, return the org-room lines.
/// Called from the grid module's tick (60 s) — no task of its own.
pub async fn follow_the_fleet(now_ms: u64) -> Vec<String> {
    let heard: std::collections::HashMap<Uuid, u64> = crate::capacity::gossip::global_ledger()
        .heard_offers_with_age()
        .into_iter()
        .map(|(peer, _offer, heard_at_ms)| (peer, now_ms.saturating_sub(heard_at_ms)))
        .collect();
    let mut lines = Vec::new();
    for sw in switches() {
        let inputs = PlacementInputs {
            seat: sw.seat(),
            beacon_age_ms: heard.get(&sw.peer()).copied(),
            lane_cold: sw.lane_cold(),
            local_available: match sw.seat() {
                // Only ask (and build) when a fall-home is actually on the table.
                Seat::Remote => sw.local_available().await,
                Seat::Home => true,
            },
            since_last_move_ms: now_ms.saturating_sub(sw.moved_at_ms()),
        };
        let mv = decide(inputs);
        if let Some(line) = sw.apply(&mv, now_ms).await {
            lines.push(line);
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(seat: Seat, age: Option<u64>, cold: bool, local: bool, since: u64) -> PlacementInputs {
        PlacementInputs { seat, beacon_age_ms: age, lane_cold: cold, local_available: local, since_last_move_ms: since }
    }

    // what this catches: the 2026-09-07 hand moves, as the rule — a dark seat (no
    // beacon for 90 s, or the breaker cold) with a home lane available → fall home;
    // a fresh seat while home → return. The two silent failure shapes are named.
    #[test]
    fn a_dark_seat_falls_home_and_a_fresh_seat_returns() {
        assert_eq!(
            decide(inputs(Seat::Remote, Some(REMOTE_SEAT_SILENT_MS + 1), false, true, MOVE_COOLDOWN_MS)),
            PlacementMove::FallHome { reason: "seat silent: no capacity beacon" }
        );
        assert_eq!(
            decide(inputs(Seat::Remote, Some(5_000), true, true, MOVE_COOLDOWN_MS)),
            PlacementMove::FallHome { reason: "breaker cold: three deadline misses in a row" }
        );
        assert_eq!(
            decide(inputs(Seat::Remote, None, false, true, MOVE_COOLDOWN_MS)),
            PlacementMove::FallHome { reason: "seat silent: no capacity beacon" },
            "never heard = dark"
        );
        assert_eq!(decide(inputs(Seat::Home, Some(10_000), false, true, MOVE_COOLDOWN_MS)), PlacementMove::ReturnRemote);
    }

    // what this catches: a node with no lane for her PARKS (loud, no move) — never the
    // silent local downgrade [[fallbacks-are-illegal-fail-loud]] forbids (the Intel case).
    #[test]
    fn a_node_without_a_home_lane_parks_instead_of_downgrading() {
        assert_eq!(
            decide(inputs(Seat::Remote, None, false, false, MOVE_COOLDOWN_MS)),
            PlacementMove::Park { reason: "seat silent: no capacity beacon" }
        );
    }

    // what this catches: a flapping tower cannot move her twice inside the cooldown, and
    // a live seat is left alone (the common tick).
    #[test]
    fn cooldown_and_a_live_seat_both_hold_still() {
        assert_eq!(decide(inputs(Seat::Remote, None, false, true, MOVE_COOLDOWN_MS - 1)), PlacementMove::Stay);
        assert_eq!(decide(inputs(Seat::Home, Some(1_000), false, true, MOVE_COOLDOWN_MS - 1)), PlacementMove::Stay);
        assert_eq!(decide(inputs(Seat::Remote, Some(4_000), false, true, MOVE_COOLDOWN_MS)), PlacementMove::Stay);
        // Back home, the seat is heard but only just (not yet three beacons): stay.
        assert_eq!(decide(inputs(Seat::Home, Some(REMOTE_SEAT_FRESH_MS), false, true, MOVE_COOLDOWN_MS)), PlacementMove::Stay);
        // A fresh beacon while the breaker is still cold: not yet.
        assert_eq!(decide(inputs(Seat::Home, Some(1_000), true, true, MOVE_COOLDOWN_MS)), PlacementMove::Stay);
    }
}
