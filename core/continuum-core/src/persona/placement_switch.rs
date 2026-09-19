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
//!
//! STAGE B (2026-09-15, Joel via Astra: "placement must work automatically through
//! the airc design, not manual binding IDs"): EVERY persona runs on a switch, Home
//! first. Once per grid tick the pass reads the peers' beacons (what each serves, its
//! lanes, its residents) and, when this node has more minds than lanes, moves the
//! least-served minds onto a peer's free lanes whose served model ranks at least as
//! high ([`choose_offloads`], pure). The move is a bound remote lane + a persisted
//! override, so a reboot keeps it; the fall-home/return rule above then owns it.

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
use crate::persona::home::PersonaHome;
use crate::persona::model_override::PersonaModelOverride;
use airc_lib::Airc;
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
/// The bound a seat's MEASURED queue is judged against when this node has no measured
/// queue of its own: the unattended time-to-first-token budget. A seat whose median
/// leased-in wait is past it is not capacity, whatever its rank (card c84d885a, S1b).
pub const SEAT_QUEUE_BOUND_MS: u64 = crate::inference::prefill_rate::UNATTENDED_TTFT.as_millis() as u64;
/// The fall-home reason a queued seat earns.
pub const SEAT_QUEUED_REASON: &str = "seat queued: its measured lane wait is past the bound";

/// PURE: is a seat's measured queue too long to be capacity for a node whose own median
/// lane wait is `home_wait_p50_ms` (`None` = unmeasured)? An UNMEASURED seat (no samples)
/// is never judged queued here — the absence-read-as-a-value guard; its live
/// `free_slots_live` and the grant it must give are what bound it. Cormac's rule: offload
/// only when the remote turn is expected FASTER — a measured seat is capacity only when
/// its wait is under this node's own, or under the budget when this node has none.
pub fn seat_queued(wait_p50_ms: u64, wait_samples: u32, home_wait_p50_ms: Option<u64>) -> bool {
    wait_samples > 0 && wait_p50_ms >= home_wait_p50_ms.unwrap_or(SEAT_QUEUE_BOUND_MS)
}
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
    /// Has the seat's PEER ever served (or is it brand new)? A fresh beacon proves the
    /// peer is ALIVE, not that it will ANSWER; without this a rescued citizen is
    /// returned to a peer that has never served a single request (card ad96f5d1). From
    /// the peer breaker's `served_ok` — the summed-success mirror of `lane_cold`.
    pub peer_served_ok: bool,
    /// The bound seat's own median leased-in wait from its beacon; `None` = unmeasured or
    /// no beacon. Measured and past the bound → she is a candidate again (S1b).
    pub seat_wait_p50_ms: Option<u64>,
    /// This node's own median lane wait; `None` = unmeasured. "Home, if home is faster."
    pub home_wait_p50_ms: Option<u64>,
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
    // A measured seat whose queue is past the bound (this node's own wait, else the
    // unattended TTFT budget) is not capacity: she comes home if home is faster, and
    // `follow_the_fleet` may re-choose her for a seat that is (card c84d885a, S1b —
    // Cormac: "a bound switch is never re-chosen" was the defect).
    let queued = i
        .seat_wait_p50_ms
        .is_some_and(|w| seat_queued(w, 1, i.home_wait_p50_ms));
    match i.seat {
        Seat::Remote => {
            let reason = if i.lane_cold {
                "breaker cold: three deadline misses in a row"
            } else if dark {
                "seat silent: no capacity beacon"
            } else if queued {
                SEAT_QUEUED_REASON
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
            // A fresh beacon proves the peer is ALIVE; `peer_served_ok` proves it will
            // ANSWER (or has never been tried). Without the service term a rescued
            // citizen is returned to a never-serving peer the moment the 5-minute cold
            // window lapses — the flap card ad96f5d1 measured.
            if fresh && !i.lane_cold && cooled && i.peer_served_ok && !queued {
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
    /// The remote seat, when one is bound (an override at build, or an offload).
    peer: std::sync::RwLock<Option<Uuid>>,
    remote: std::sync::RwLock<Option<Arc<AircRemoteInferenceAdapter>>>,
    local: tokio::sync::OnceCell<Option<Arc<dyn AIProviderAdapter>>>,
    local_factory: Arc<dyn PersonaAdapterFactory>,
    profile: PersonaInferenceProfile,
    /// The wire a remote lane rides; absent = this node cannot offload (tests, no airc).
    airc: Option<Arc<tokio::sync::OnceCell<Arc<Airc>>>>,
    /// Her home dir, for persisting an offload as her override.
    home: Option<PersonaHome>,
    seat: AtomicU8,
    moved_at_ms: AtomicU64,
    /// Fixed strings the adapter trait hands out by reference.
    provider_label: String,
    name_label: String,
    home_model: String,
}

impl PlacementSwitch {
    /// A switch born on a REMOTE seat (her durable override named a peer at build).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        persona_id: Uuid,
        persona_name: String,
        peer: Uuid,
        remote: Arc<AircRemoteInferenceAdapter>,
        local_factory: Arc<dyn PersonaAdapterFactory>,
        profile: PersonaInferenceProfile,
        airc: Option<Arc<tokio::sync::OnceCell<Arc<Airc>>>>,
        home: Option<PersonaHome>,
    ) -> Self {
        let mut me = Self::home(persona_id, persona_name, None, local_factory, profile, airc, home);
        *me.peer.get_mut().unwrap_or_else(|p| p.into_inner()) = Some(peer); // JUSTIFIED unwrap_or_else: a poisoned lock still holds the value; the switch is bookkeeping, never truth
        *me.remote.get_mut().unwrap_or_else(|p| p.into_inner()) = Some(remote); // JUSTIFIED unwrap_or_else: a poisoned lock still holds the value; the switch is bookkeeping, never truth
        me.seat = AtomicU8::new(Seat::Remote as u8);
        me
    }

    /// A switch born at HOME — every persona gets one (stage B). `local` = her local
    /// adapter when already built; `None` = built on first need from the factory.
    pub fn home(
        persona_id: Uuid,
        persona_name: String,
        local: Option<Arc<dyn AIProviderAdapter>>,
        local_factory: Arc<dyn PersonaAdapterFactory>,
        profile: PersonaInferenceProfile,
        airc: Option<Arc<tokio::sync::OnceCell<Arc<Airc>>>>,
        home: Option<PersonaHome>,
    ) -> Self {
        let cell = tokio::sync::OnceCell::new();
        if let Some(l) = local {
            let _ = cell.set(Some(l));
        }
        Self {
            persona_id,
            provider_label: "placement-switch".to_string(),
            name_label: format!("placement switch for {persona_name}"),
            home_model: profile.model_id.clone(),
            persona_name,
            peer: std::sync::RwLock::new(None),
            remote: std::sync::RwLock::new(None),
            local: cell,
            local_factory,
            profile,
            airc,
            home,
            seat: AtomicU8::new(Seat::Home as u8),
            moved_at_ms: AtomicU64::new(0),
        }
    }

    pub fn seat(&self) -> Seat {
        if self.seat.load(Ordering::Relaxed) == Seat::Home as u8 { Seat::Home } else { Seat::Remote }
    }
    pub fn peer(&self) -> Option<Uuid> {
        *self.peer.read().unwrap_or_else(|p| p.into_inner()) // JUSTIFIED unwrap_or_else: a poisoned lock still holds the value; the switch is bookkeeping, never truth
    }
    pub fn persona_id(&self) -> Uuid {
        self.persona_id
    }
    fn remote_adapter(&self) -> Option<Arc<AircRemoteInferenceAdapter>> {
        self.remote.read().unwrap_or_else(|p| p.into_inner()).clone() // JUSTIFIED unwrap_or_else: a poisoned lock still holds the value; the switch is bookkeeping, never truth
    }
    pub fn persona_name(&self) -> &str {
        &self.persona_name
    }
    pub fn moved_at_ms(&self) -> u64 {
        self.moved_at_ms.load(Ordering::Relaxed)
    }
    pub fn lane_cold(&self) -> bool {
        self.remote_adapter().map(|r| r.is_cold()).unwrap_or(false) // JUSTIFIED unwrap_or: no remote lane = nothing to be cold
    }

    /// Bind a remote lane on `peer` serving `model` and move there (stage B). Persists
    /// the move as her override so a reboot keeps it. Fails loudly without a wire.
    pub fn go_remote(&self, peer: Uuid, model: &str, now_ms: u64) -> Result<(), String> {
        let airc = self
            .airc
            .as_ref()
            .and_then(|c| c.get().cloned())
            .ok_or_else(|| "no airc handle on this node — cannot bind a remote lane".to_string())?;
        let transport = crate::inference::airc_remote::AircLiveTransport::new(airc, peer);
        let adapter = Arc::new(
            AircRemoteInferenceAdapter::new(transport)
                .with_target_peer(peer.to_string())
                .with_model(model.to_string())
                .with_persona(self.persona_name.clone()),
        );
        *self.remote.write().unwrap_or_else(|p| p.into_inner()) = Some(adapter); // JUSTIFIED unwrap_or_else: a poisoned lock still holds the value; the switch is bookkeeping, never truth
        *self.peer.write().unwrap_or_else(|p| p.into_inner()) = Some(peer); // JUSTIFIED unwrap_or_else: a poisoned lock still holds the value; the switch is bookkeeping, never truth
        self.set_seat(Seat::Remote, now_ms);
        if let Some(home) = &self.home {
            let over = PersonaModelOverride::new_remote(model, Some("placement:fleet".to_string()), now_ms, &peer.to_string());
            if let Err(e) = over.write(home) {
                crate::probe!(
                    class = "persona.placement.override_unpersisted",
                    persona = %self.persona_name,
                    error = %e,
                    "offload is live this session but not recorded — a reboot brings her home"
                );
            }
        }
        Ok(())
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

    fn current(&self) -> Arc<dyn AIProviderAdapter> {
        let home = || self.local.get().and_then(|l| l.clone());
        let remote = || self.remote_adapter().map(|r| r as Arc<dyn AIProviderAdapter>);
        match self.seat() {
            Seat::Home => home().or_else(remote),
            Seat::Remote => remote().or_else(home),
        }
        .unwrap_or_else(|| Arc::new(NoSeat(self.persona_name.clone()))) // JUSTIFIED unwrap_or_else: no seat at all answers by NAME on every generation, never silently
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
                    peer = %self.peer().map(|p| p.to_string()).unwrap_or_default(), // JUSTIFIED unwrap_or_default: probe label only

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
                    peer = %self.peer().map(|p| p.to_string()).unwrap_or_default(), // JUSTIFIED unwrap_or_default: probe label only

                    reason = *reason,
                    "her remote seat went dark — her brain runs on this node until the seat beacons again"
                );
                Some(format!(
                    "[placement] {} fell home from {} ({}) — runs here until the seat beacons again",
                    self.persona_name,
                    self.peer().map(|p| p.to_string()).unwrap_or_default(), // JUSTIFIED unwrap_or_default: line text only
                    reason
                ))
            }
            PlacementMove::ReturnRemote => {
                self.set_seat(Seat::Remote, now_ms);
                crate::probe!(
                    class = "persona.placement.returned",
                    persona = %self.persona_name,
                    peer = %self.peer().map(|p| p.to_string()).unwrap_or_default(), // JUSTIFIED unwrap_or_default: probe label only

                    "her remote seat is beaconing again — her brain goes back off-box; her next turn is the proof"
                );
                Some(format!(
                    "[placement] {} returned to {} — the seat beacons again; her next turn is the proof",
                    self.persona_name,
                    self.peer().map(|p| p.to_string()).unwrap_or_default() // JUSTIFIED unwrap_or_default: line text only
                ))
            }
        }
    }
}

/// The adapter a switch answers with when it has NEITHER seat yet (a home adapter
/// that could not be built and no remote bound). Every generation fails by name.
struct NoSeat(String);

#[async_trait]
impl AIProviderAdapter for NoSeat {
    fn provider_id(&self) -> &str {
        "placement-switch/no-seat"
    }
    fn name(&self) -> &str {
        "no seat"
    }
    fn default_model(&self) -> &str {
        ""
    }
    async fn generate_text(&self, _request: TextGenerationRequest) -> Result<TextGenerationResponse, String> {
        Err(format!("{}: no seat — her home adapter could not be built and no remote lane is bound", self.0))
    }
}

#[async_trait]
impl AIProviderAdapter for PlacementSwitch {
    fn provider_id(&self) -> &str {
        &self.provider_label
    }
    fn name(&self) -> &str {
        &self.name_label
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
    /// Her home model: the request carries the lane's model on the wire when remote.
    fn default_model(&self) -> &str {
        &self.home_model
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

/// What a peer's beacon says it can host (stage B input, from `CapacityOffer`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerOffer {
    pub peer: Uuid,
    pub served_model: Option<String>,
    pub lanes: u32,
    pub residents: u32,
    pub beacon_age_ms: u64,
    /// The seat's OWN free slots (lanes minus everything in flight, local and leased-in),
    /// from its beacon (card c84d885a, S1). `lanes − residents` is roster arithmetic that
    /// every node evaluated independently and so spilled onto the same slot; this is the
    /// seat's admission truth. 0 on an older beacon = no free slot offered.
    pub free_slots_live: u32,
    /// The seat's own median receipt-to-first-progress wait for leased-in generates, ms,
    /// with its sample count — unmeasured (0 samples) is never a fast seat.
    pub lane_wait_p50_ms: u64,
    pub lane_wait_samples: u32,
}

/// This node's own shape for the chooser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalShape {
    /// This node's own median lane wait, ms; `None` = unmeasured. `Some(0)` — a median
    /// under a millisecond — means the lanes here are not contended and nobody moves.
    pub lane_wait_p50_ms: Option<u64>,
    pub resident: u32,
    pub lanes: u32,
    /// Measured capability of the local served model, or the planner's unmeasured proxy cap.
    pub rank: u8,
}

/// The pure chooser: which minds move to which peer. `candidates` = minds at Home with
/// their lane grants this hour (fewest first = most starved). A node with more minds than
/// lanes borrows a fresh peer's FREE lanes (lanes − its residents) when that peer's served
/// model ranks at least as high; never more minds than the node is over by.
pub fn choose_offloads(
    local: LocalShape,
    peers: &[PeerOffer],
    rank_of: &(dyn Fn(&str) -> Option<u8> + Sync),
    candidates: &[(Uuid, u64)],
) -> Vec<(Uuid, Uuid, String)> {
    let mut over = local.resident.saturating_sub(local.lanes) as usize;
    // No queue at home (a measured median under a millisecond) = nothing to gain by
    // moving anyone, however many minds share the lanes (Cormac's shape 3: offload only
    // when the remote turn is expected faster). Unmeasured home = minds over lanes is
    // the only evidence there is; move on it.
    let home_queued = local.lane_wait_p50_ms != Some(0);
    if over == 0 || candidates.is_empty() || !home_queued {
        return Vec::new();
    }
    let mut minds: Vec<(Uuid, u64)> = candidates.to_vec();
    minds.sort_by_key(|(_, grants)| *grants);
    let mut next = minds.into_iter();
    let mut out = Vec::new();
    for p in peers {
        if over == 0 {
            break;
        }
        if p.beacon_age_ms > REMOTE_SEAT_FRESH_MS {
            continue;
        }
        let Some(model) = p.served_model.as_deref() else { continue };
        let Some(rank) = rank_of(model) else { continue };
        if rank < local.rank {
            continue;
        }
        if seat_queued(p.lane_wait_p50_ms, p.lane_wait_samples, local.lane_wait_p50_ms) {
            continue;
        }
        // THE SEAT'S live grant-able count (lanes − in flight − already granted), not
        // `lanes − residents` roster arithmetic every node once evaluated on its own. The
        // grant itself is acquired by the caller before she moves.
        let mut free = p.free_slots_live as usize;
        while free > 0 && over > 0 {
            let Some((mind, _)) = next.next() else { return out };
            out.push((mind, p.peer, model.to_string()));
            free -= 1;
            over -= 1;
        }
    }
    out
}

/// One pass: read beacon ages, decide per switch (fall home / return), then offload a
/// starved node's tail minds onto peers' free lanes. Returns the org-room lines. Called
/// from the grid module's tick (60 s) — no task of its own.
pub async fn follow_the_fleet(
    now_ms: u64,
    peers: Vec<PeerOffer>,
    local: LocalShape,
    rank_of: &(dyn Fn(&str) -> Option<u8> + Sync),
) -> Vec<String> {
    let heard: std::collections::HashMap<Uuid, u64> =
        peers.iter().map(|p| (p.peer, p.beacon_age_ms)).collect();
    let offers: std::collections::HashMap<Uuid, &PeerOffer> = peers.iter().map(|p| (p.peer, p)).collect();
    let home_wait_p50_ms = {
        let (p50, samples) = crate::cognition::resource_admission::local_lane_wait_p50_ms();
        (samples > 0).then_some(p50)
    };
    let mut lines = Vec::new();
    let mut at_home: Vec<(Uuid, u64)> = Vec::new();
    for sw in switches() {
        let cooled = now_ms.saturating_sub(sw.moved_at_ms()) >= MOVE_COOLDOWN_MS;
        let Some(peer) = sw.peer() else {
            // Home-born, never bound: a candidate for an offload when eligible.
            if cooled {
                at_home.push((sw.persona_id(), crate::modules::citizen_health::lane_grants_of(sw.persona_id())));
            }
            continue;
        };
        let inputs = PlacementInputs {
            seat: sw.seat(),
            beacon_age_ms: heard.get(&peer).copied(),
            lane_cold: sw.lane_cold(),
            local_available: match sw.seat() {
                Seat::Remote => sw.local_available().await,
                Seat::Home => true,
            },
            since_last_move_ms: now_ms.saturating_sub(sw.moved_at_ms()),
            // The peer's summed service evidence — the return gate's mirror of
            // `lane_cold` (card ad96f5d1). breaker_for creates a pristine (served_ok =
            // true) entry for a peer never seen, so a new seat is not self-sealed.
            peer_served_ok: crate::inference::airc_remote::peer_breaker::breaker_for(
                &peer.to_string(),
            )
            .served_ok(),
            seat_wait_p50_ms: offers
                .get(&peer)
                .and_then(|o| (o.lane_wait_samples > 0).then_some(o.lane_wait_p50_ms)),
            home_wait_p50_ms,
        };
        let mut mv = decide(inputs);
        // A RETURN ASKS FOR ITS SLOT LIKE A SPILL DOES (2026-09-19 00:3xZ): the 5090's beacon
        // first carried its measured wait, read QUEUED, and the IntelMac brought all seven
        // bound minds home in one tick — correct. The moment that seat's wait dips under
        // the bound, `decide` says ReturnRemote for all seven at once, and a return re-bound
        // without a grant would rebuild the very queue they left: the flap at fleet scale.
        // The seat grants what it has (`free_slots_live` − outstanding grants); the rest
        // stay home until its beacon shows room. Refused or unanswered = Stay, receipted.
        if mv == PlacementMove::ReturnRemote {
            let asked = match crate::persona::operator_peer::operator_airc() {
                Some(a) => crate::persona::placement_reservation::request_reservation(&a, peer, sw.persona_id()).await,
                None => Err("no airc handle on this node — cannot ask the seat".to_string()),
            };
            match asked {
                Ok(g) if g.granted => crate::probe!(
                    class = "placement.return.leased",
                    persona = %sw.persona_name(),
                    peer = %peer,
                    until_ms = g.until_ms,
                    seat_free_after = g.free_slots_live,
                    seat_wait_p50_ms = g.lane_wait_p50_ms,
                    "the seat granted her slot back — returning"
                ),
                Ok(g) => {
                    crate::probe!(
                        class = "placement.return.refused",
                        persona = %sw.persona_name(),
                        peer = %peer,
                        reason = %g.reason,
                        seat_free = g.free_slots_live,
                        seat_outstanding = g.outstanding,
                        "the seat has no slot for her return this tick — she stays home"
                    );
                    mv = PlacementMove::Stay;
                }
                Err(e) => {
                    crate::probe!(
                        class = "placement.return.refused",
                        persona = %sw.persona_name(),
                        peer = %peer,
                        reason = %e,
                        "the seat did not answer the return ask in time — she stays home"
                    );
                    mv = PlacementMove::Stay;
                }
            }
        }
        if let Some(line) = sw.apply(&mv, now_ms).await {
            lines.push(line);
        }
        // A bound switch sitting HOME past her cooldown is a chooser candidate again —
        // the chooser may bind her to a seat that is capacity now (S1b). Her old seat is
        // only re-chosen if it is; `decide` above already refused a queued return.
        if mv == PlacementMove::Stay && sw.seat() == Seat::Home && cooled {
            at_home.push((sw.persona_id(), crate::modules::citizen_health::lane_grants_of(sw.persona_id())));
        }
    }
    let moves = choose_offloads(local, &peers, rank_of, &at_home);
    if !moves.is_empty() {
        let by_id: std::collections::HashMap<Uuid, Arc<PlacementSwitch>> =
            switches().into_iter().map(|s| (s.persona_id(), s)).collect();
        let airc = crate::persona::operator_peer::operator_airc();
        for (mind, peer, model) in moves {
            let Some(sw) = by_id.get(&mind) else { continue };
            // A SLOT IS A LEASE THE SEAT GRANTS: ask before she moves. Refused or
            // unanswered = she stays home this tick, receipted (S1b).
            let asked = match airc.as_ref() {
                Some(a) => crate::persona::placement_reservation::request_reservation(a, peer, mind).await,
                None => Err("no airc handle on this node — cannot ask the seat".to_string()),
            };
            let grant = match asked {
                Ok(g) if g.granted => g,
                Ok(g) => {
                    crate::probe!(
                        class = "placement.spill.refused",
                        persona = %sw.persona_name(),
                        peer = %peer,
                        reason = %g.reason,
                        seat_free = g.free_slots_live,
                        seat_outstanding = g.outstanding,
                        seat_wait_p50_ms = g.lane_wait_p50_ms,
                        seat_wait_samples = g.lane_wait_samples,
                        "the seat refused the slot its beacon offered — she stays home"
                    );
                    continue;
                }
                Err(e) => {
                    crate::probe!(
                        class = "placement.spill.refused",
                        persona = %sw.persona_name(),
                        peer = %peer,
                        reason = %e,
                        "the seat did not grant a slot in time — she stays home"
                    );
                    continue;
                }
            };
            crate::probe!(
                class = "placement.spill.leased",
                persona = %sw.persona_name(),
                peer = %peer,
                until_ms = grant.until_ms,
                seat_free_after = grant.free_slots_live,
                seat_outstanding = grant.outstanding,
                seat_wait_p50_ms = grant.lane_wait_p50_ms,
                seat_wait_samples = grant.lane_wait_samples,
                "the seat granted a slot — binding her lane"
            );
            match sw.go_remote(peer, &model, now_ms) {
                Ok(()) => {
                    crate::probe!(
                        class = "persona.placement.offloaded",
                        persona = %sw.persona_name(),
                        peer = %peer,
                        model = %model,
                        local_resident = local.resident,
                        local_lanes = local.lanes,
                        "more minds than lanes here — her brain moved to a peer's free lane"
                    );
                    lines.push(format!(
                        "[placement] {} → {} ({}): this node has {} minds on {} lanes; the seat granted a slot ({} left, its wait p50 {} ms over {} samples)",
                        sw.persona_name(), peer, model, local.resident, local.lanes,
                        grant.free_slots_live, grant.lane_wait_p50_ms, grant.lane_wait_samples
                    ));
                }
                Err(e) => crate::probe!(
                    class = "persona.placement.offload_failed",
                    persona = %sw.persona_name(),
                    peer = %peer,
                    error = %e,
                    "could not bind the remote lane — she stays home"
                ),
            }
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(seat: Seat, age: Option<u64>, cold: bool, local: bool, since: u64) -> PlacementInputs {
        // peer_served_ok defaults TRUE here: these cases predate the return-gate service
        // term and assume a serving peer; `a_fresh_but_never_serving_seat_is_not_returned`
        // exercises the false case.
        PlacementInputs { seat, beacon_age_ms: age, lane_cold: cold, local_available: local, since_last_move_ms: since, peer_served_ok: true, seat_wait_p50_ms: None, home_wait_p50_ms: None }
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

    // what this catches (card ad96f5d1, the flap): a fresh beacon proves the peer is
    // ALIVE, not that it will ANSWER. A seat that beacons perfectly but has never served
    // (peer_served_ok = false) must NOT be returned to — otherwise a rescued citizen is
    // sent straight back the moment the 5-minute cold window lapses. Same inputs that
    // returned above, only the service term flipped, must now Stay.
    #[test]
    fn a_fresh_but_never_serving_seat_is_not_returned() {
        let mut i = inputs(Seat::Home, Some(10_000), false, true, MOVE_COOLDOWN_MS);
        assert_eq!(decide(i.clone()), PlacementMove::ReturnRemote, "serving peer: return");
        i.peer_served_ok = false;
        assert_eq!(
            decide(i),
            PlacementMove::Stay,
            "alive-but-never-served peer: hold home, do not feed the dead seat"
        );
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

    // what this catches: the pooled-demand rule — 16 minds on 7 lanes borrow a fresh
    // peer's two free lanes (27B measured 42 ≥ local proxy 40) for the two least-served
    // minds; a stale beacon, a lower-ranked model, or a full peer lends nothing; and a
    // node with lanes to spare moves nobody.
    #[test]
    fn a_node_with_more_minds_than_lanes_borrows_a_peers_free_lanes() {
        let gpu = Uuid::from_u128(0x5090);
        let rank = |m: &str| -> Option<u8> { match m { "qwen-27b" => Some(42), "tiny" => Some(10), _ => None } };
        let peers = vec![
            PeerOffer { peer: gpu, served_model: Some("qwen-27b".into()), lanes: 2, residents: 0, beacon_age_ms: 5_000, free_slots_live: 2, lane_wait_p50_ms: 0, lane_wait_samples: 0 },
            PeerOffer { peer: Uuid::from_u128(1), served_model: Some("tiny".into()), lanes: 4, residents: 0, beacon_age_ms: 5_000, free_slots_live: 4, lane_wait_p50_ms: 0, lane_wait_samples: 0 },
            PeerOffer { peer: Uuid::from_u128(2), served_model: Some("qwen-27b".into()), lanes: 2, residents: 2, beacon_age_ms: 5_000, free_slots_live: 0, lane_wait_p50_ms: 0, lane_wait_samples: 0 },
            PeerOffer { peer: Uuid::from_u128(3), served_model: Some("qwen-27b".into()), lanes: 8, residents: 0, beacon_age_ms: REMOTE_SEAT_FRESH_MS + 1, free_slots_live: 8, lane_wait_p50_ms: 0, lane_wait_samples: 0 },
        ];
        let minds: Vec<(Uuid, u64)> = (1..=16u128).map(|i| (Uuid::from_u128(0x100 + i), 20 - (i as u64 % 5))).collect();
        let moves = choose_offloads(LocalShape { resident: 16, lanes: 7, rank: 40, lane_wait_p50_ms: None }, &peers, &rank, &minds);
        assert_eq!(moves.len(), 2, "two free lanes on the only eligible peer: {moves:?}");
        assert!(moves.iter().all(|(_, p, m)| *p == gpu && m == "qwen-27b"));
        let least = minds.iter().min_by_key(|(_, g)| *g).map(|(m, _)| *m).expect("a mind");
        assert_eq!(moves[0].0, least, "the least-served mind goes first");
        assert!(choose_offloads(LocalShape { resident: 5, lanes: 7, rank: 40, lane_wait_p50_ms: None }, &peers, &rank, &minds).is_empty());
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

    fn offer(peer: u128, free: u32, wait_p50_ms: u64, wait_samples: u32) -> PeerOffer {
        PeerOffer { peer: Uuid::from_u128(peer), served_model: Some("qwen-27b".into()), lanes: 4, residents: 4, beacon_age_ms: 5_000, free_slots_live: free, lane_wait_p50_ms: wait_p50_ms, lane_wait_samples: wait_samples }
    }
    fn local(resident: u32, lanes: u32, home_wait: Option<u64>) -> LocalShape {
        LocalShape { resident, lanes, rank: 40, lane_wait_p50_ms: home_wait }
    }
    fn rank27(m: &str) -> Option<u8> { (m == "qwen-27b").then_some(42) }
    fn minds(n: u128) -> Vec<(Uuid, u64)> { (1..=n).map(|i| (Uuid::from_u128(0x100 + i), i as u64)).collect() }

    // what this catches (card c84d885a, S1b — Cormac's "NO RESERVATION" location): `free`
    // is the seat's LIVE grant-able count, never `lanes − residents`. A seat with 4 lanes
    // and 4 residents that has 2 slots free lends 2; a seat with 4 lanes and no residents
    // that has 0 free (every slot in flight or already granted) lends nothing; an older
    // beacon (field absent → 0) offers nothing until it says otherwise.
    #[test]
    fn free_is_the_seats_live_grant_not_roster_arithmetic() {
        let peers = vec![offer(1, 2, 0, 0), PeerOffer { free_slots_live: 0, residents: 0, ..offer(2, 0, 0, 0) }];
        let moves = choose_offloads(local(8, 2, None), &peers, &rank27, &minds(8));
        assert_eq!(moves.len(), 2, "{moves:?}");
        assert!(moves.iter().all(|(_, p, _)| *p == Uuid::from_u128(1)), "only the seat with live free slots: {moves:?}");
    }

    // what this catches (S1b, Cormac's shape 3): a seat whose MEASURED queue is not under
    // this node's own is not capacity, whatever its rank — and an unmeasured seat is not
    // read as fast, but is not refused on its history either (its live free count and
    // the grant bound it). Every number is relative to the one bound, not a grid value.
    #[test]
    fn a_queued_seat_is_not_capacity_whatever_its_rank() {
        let b = SEAT_QUEUE_BOUND_MS;
        let queued = offer(1, 2, b + 1, 30);
        let quick = offer(2, 2, b / 4, 30);
        let unmeasured = offer(3, 2, 0, 0);
        let go = |home: Option<u64>, peers: &[PeerOffer]| -> Vec<Uuid> {
            choose_offloads(local(8, 2, home), peers, &rank27, &minds(1)).into_iter().map(|(_, p, _)| p).collect()
        };
        // Home unmeasured: the budget is the bound.
        assert!(go(None, &[queued.clone()]).is_empty(), "past the budget: skipped");
        assert_eq!(go(None, &[quick.clone()]), vec![quick.peer], "under the budget: capacity");
        assert_eq!(go(None, &[unmeasured.clone()]), vec![unmeasured.peer], "unmeasured is not judged on history");
        // Home measured: the remote must be FASTER than home.
        let home = Some(b / 2);
        assert!(go(home, &[offer(4, 2, b / 2, 30)]).is_empty(), "equal to home: not faster, skipped");
        assert_eq!(go(home, &[quick.clone()]), vec![quick.peer], "faster than home: capacity");
        assert!(go(Some(b / 8), &[quick.clone()]).is_empty(), "slower than a quick home: skipped");
        // No queue at home at all: nobody moves, however many minds share the lanes.
        assert!(go(Some(0), &[quick.clone(), unmeasured.clone()]).is_empty(), "a median under a millisecond is no queue");
        // The queued seat is passed over FOR the quick one, not just skipped.
        assert_eq!(go(None, &[queued, quick.clone()]), vec![quick.peer]);
    }

    // what this catches (S1b, Cormac's shape 2): a bound switch whose seat's measured wait
    // is past the bound falls home when home is faster (measured) or when the seat is past
    // the budget (home unmeasured); a queued seat is never RETURNED to; cooldown still
    // holds; and an unmeasured seat wait changes nothing.
    #[test]
    fn a_bound_seat_whose_measured_wait_passes_the_bound_falls_home_and_is_not_returned_to() {
        let b = SEAT_QUEUE_BOUND_MS;
        let mut i = inputs(Seat::Remote, Some(5_000), false, true, MOVE_COOLDOWN_MS);
        i.seat_wait_p50_ms = Some(b + 1);
        assert_eq!(decide(i.clone()), PlacementMove::FallHome { reason: SEAT_QUEUED_REASON }, "past the budget, home unmeasured");
        i.seat_wait_p50_ms = Some(b / 2);
        assert_eq!(decide(i.clone()), PlacementMove::Stay, "under the budget, home unmeasured: stay");
        i.home_wait_p50_ms = Some(b / 4);
        assert_eq!(decide(i.clone()), PlacementMove::FallHome { reason: SEAT_QUEUED_REASON }, "home is faster: come home");
        i.home_wait_p50_ms = Some(b);
        assert_eq!(decide(i.clone()), PlacementMove::Stay, "home is slower: the seat is still the better lane");
        i.home_wait_p50_ms = Some(b / 4);
        i.since_last_move_ms = MOVE_COOLDOWN_MS - 1;
        assert_eq!(decide(i.clone()), PlacementMove::Stay, "cooldown holds");
        i.since_last_move_ms = MOVE_COOLDOWN_MS;
        i.local_available = false;
        assert_eq!(decide(i.clone()), PlacementMove::Park { reason: SEAT_QUEUED_REASON }, "no home lane: parked, loud");
        // Home-seated, the seat fresh and serving — but queued: no return.
        let mut h = inputs(Seat::Home, Some(10_000), false, true, MOVE_COOLDOWN_MS);
        assert_eq!(decide(h.clone()), PlacementMove::ReturnRemote);
        h.seat_wait_p50_ms = Some(b + 1);
        assert_eq!(decide(h.clone()), PlacementMove::Stay, "a queued seat is not returned to");
        h.seat_wait_p50_ms = None;
        assert_eq!(decide(h), PlacementMove::ReturnRemote, "unmeasured changes nothing");
    }
}
