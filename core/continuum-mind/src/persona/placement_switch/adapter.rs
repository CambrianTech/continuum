//! Adapter state and application of placement transitions.
use super::*;
use crate::ai::adapter::{
    AIProviderAdapter, AdapterCapabilities, ApiStyle, GenerationChunk, InferenceDevice,
    LoRAAdapterInfo, LoRACapabilities,
};
use crate::ai::types::{
    EmbeddingRequest, EmbeddingResponse, HealthStatus, ModelInfo, TextGenerationRequest,
    TextGenerationResponse,
};
use crate::inference::airc_remote::AircRemoteInferenceAdapter;
use crate::cognition::home::PersonaHome;
use crate::persona::inference_profile::PersonaInferenceProfile;
use crate::persona::model_override::PersonaModelOverride;
use crate::persona::supervisor::PersonaAdapterFactory;
use airc_lib::Airc;
use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use uuid::Uuid;


/// No home capability known yet: every u8 rank is a real value, so the sentinel lives
/// outside the u8 range.
const HOME_CAPABILITY_UNKNOWN: u16 = u16::MAX;

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
    capacity_changes: tokio::sync::watch::Sender<()>,
    unavailable: Arc<dyn AIProviderAdapter>,
    moved_at_ms: AtomicU64,
    /// Has a retirement of her bound seat already been SAID? The retirement itself stays
    /// idempotent and unconditional (the self-seat path needs the second call — the first
    /// runs while she is still Remote and cannot drop the lane, the second does once she
    /// is Home). Only the RECEIPT is once-per-change: a PARKED mind on a starving seat is
    /// re-asked every tick by design, and a probe that fires on every tick is noise that
    /// buries the change it was added to report — the third time that shape bit this
    /// session (Cormac on #4307).
    retire_said: std::sync::atomic::AtomicBool,
    /// The capability of the plan she is served on at HOME, remembered from the last tick
    /// this node published one ([`HOME_CAPABILITY_UNKNOWN`] until then). An opportunity is
    /// never a step down from it (card 60d90c95); read even while this node's plan is
    /// momentarily absent, which is exactly when a mind was moved off a 27B onto a 1.5B.
    home_capability: std::sync::atomic::AtomicU16,
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
        let mut me = Self::home(
            persona_id,
            persona_name,
            None,
            local_factory,
            profile,
            airc,
            home,
        );
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
            capacity_changes: tokio::sync::watch::channel(()).0,
            unavailable: Arc::new(NoSeat(persona_name.clone())),
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
            retire_said: std::sync::atomic::AtomicBool::new(false),
            home_capability: std::sync::atomic::AtomicU16::new(HOME_CAPABILITY_UNKNOWN),
        }
    }

    /// HER airc — the wire her generates ride (`go_remote` builds the transport on it, in
    /// her current room, where the seat's core listens). A reservation ask must ride the
    /// same wire: sent on the OPERATOR's airc it left in the operator's current room (the
    /// academy on the M5, 2026-09-19 01:4xZ) where the seat never hears it — six asks,
    /// six "unanswered within 30 s", every return refused, every spill impossible. Fail
    /// closed was the right default; this is the right wire. `None` = no airc on this node.
    pub fn airc_handle(&self) -> Option<Arc<Airc>> {
        self.airc.as_ref().and_then(|c| c.get().cloned())
    }

    /// Remember the capability of the plan this node serves her on (see `home_capability`).
    pub fn note_home_capability(&self, rank: u8) {
        self.home_capability.store(u16::from(rank), Ordering::Relaxed);
    }
    /// Her home capability, `None` until this node has published a plan in this life.
    pub fn home_capability(&self) -> Option<u8> {
        u8::try_from(self.home_capability.load(Ordering::Relaxed)).ok()
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
            let over = PersonaModelOverride::new_remote(
                model,
                Some("placement:fleet".to_string()),
                now_ms,
                &peer.to_string(),
            );
            if let Err(e) = over.write(home) {
                continuum_base::probe!(
                    class = "persona.placement.override_unpersisted",
                    persona = %self.persona_name,
                    error = %e,
                    "offload is live this session but not recorded — a reboot brings her home"
                );
            }
        }
        Ok(())
    }

    /// Her bound seat was this node (card 2500d2f1): retire the durable override that
    /// named it. See [`Self::retire_bound_seat`] — this is one of its two callers.
    pub fn retire_self_seat(&self) {
        self.retire_bound_seat(SELF_SEAT_REASON);
    }

    /// The seat cannot hold her turn (2026-09-21): retire the durable override the same
    /// way, so the NEXT BOOT does not bear her back onto it.
    ///
    /// Why this is not a fall-home. [`Self::come_home_for_good`] already draws the line —
    /// a fall-home deliberately KEEPS the seat to return to, because the seat going dark
    /// or cold is a temporary outage. A window that cannot hold her turn is not an
    /// outage; `decide` says so in its own words ("this is not a flap between machines,
    /// it is a lane that cannot serve") and refuses to return her to it. But that refusal
    /// lived only in the running process: the override on disk survived, and each deploy
    /// bound her to the same seat again. Measured 2026-09-21 on the IntelMac — Benchy,
    /// Aiko and Atlas off seat e85a5bb3, EIGHT `fell_home` events across four deploys
    /// against ZERO returns, clustering 28 s and 69 s after two of those deploys.
    ///
    /// Retiring is not exile. The override is a BINDING, not a verdict on the node: a
    /// mind sitting home past her cooldown is a chooser candidate again (see the
    /// `at_home` push below), so if that seat's window grows to hold her the chooser
    /// re-binds her on its own. That is what makes this safe against a seat read narrow
    /// mid-relaunch, when the grow-back ladder has not yet restored the window — the cost
    /// of retiring too eagerly is one chooser pass, while the cost of keeping a stale
    /// binding is a citizen walked home on every deploy forever.
    pub fn retire_starving_seat(&self) {
        self.retire_bound_seat(SEAT_STARVES_REASON);
    }

    /// Retire the durable override naming her bound seat, and — once she is home — the
    /// remote lane and the peer, so she is home-born from here and the next boot does not
    /// bear her onto a seat this run already proved is none.
    /// Idempotent; the override is a record of the old confusion, never an assignment.
    fn retire_bound_seat(&self, why: &'static str) {
        let Some(peer) = self.peer() else {
            return;
        };
        // THE WORK IS UNCONDITIONAL, THE RECEIPT IS ONCE-PER-CHANGE. The self-seat path
        // needs to be re-asked: the first call runs while she is still Remote and cannot
        // drop the lane, the second drops it once she is Home. So the clear stays
        // idempotent and repeated — only the probe below is gated, on something having
        // actually changed.
        let dropped_lane = self.seat() == Seat::Home;
        if dropped_lane {
            *self.remote.write().unwrap_or_else(|p| p.into_inner()) = None; // JUSTIFIED unwrap_or_else: a poisoned lock still holds the value; the switch is bookkeeping, never truth
            *self.peer.write().unwrap_or_else(|p| p.into_inner()) = None; // JUSTIFIED unwrap_or_else: a poisoned lock still holds the value; the switch is bookkeeping, never truth
        }
        let cleared = match &self.home {
            Some(home) => PersonaModelOverride::clear(home).map_err(|e| e.to_string()),
            None => Ok(()),
        };
        // A PARKED mind on a starving seat is re-asked every tick by design (Park repeats;
        // that is why `apply` returns no room line for it). Saying it every tick would
        // bury the one transition worth reading, so the receipt fires on the first sight
        // and again when the lane actually drops — never on the ticks between.
        let first = !self
            .retire_said
            .swap(true, std::sync::atomic::Ordering::Relaxed);
        if !(first || dropped_lane) {
            return;
        }
        continuum_base::probe!(
            class = "persona.placement.seat_retired",
            persona = %self.persona_name,
            peer = %peer,
            why,
            cleared = cleared.is_ok(),
            error = %cleared.err().unwrap_or_default(), // JUSTIFIED unwrap_or_default: probe label only
            at_home = self.seat() == Seat::Home,
            "this run proved her bound seat is no seat — the override naming it is retired, and her remote lane once she is home"
        );
    }

    /// Come home FOR GOOD (an opportunity move to this node): the seat, the remote lane
    /// and the peer go, and the durable override that named the seat is retired, so the
    /// next boot bears her home — unlike a fall-home, which keeps the seat to return to.
    pub fn come_home_for_good(&self, now_ms: u64) {
        self.set_seat(Seat::Home, now_ms);
        *self.remote.write().unwrap_or_else(|p| p.into_inner()) = None; // JUSTIFIED unwrap_or_else: a poisoned lock still holds the value; the switch is bookkeeping, never truth
        *self.peer.write().unwrap_or_else(|p| p.into_inner()) = None; // JUSTIFIED unwrap_or_else: a poisoned lock still holds the value; the switch is bookkeeping, never truth
        if let Some(home) = &self.home {
            if let Err(e) = PersonaModelOverride::clear(home) {
                continuum_base::probe!(
                    class = "persona.placement.override_unpersisted",
                    persona = %self.persona_name,
                    error = %e,
                    "she is home for good this session but the override naming her old seat could not be cleared — a reboot bears her there once more"
                );
            }
        }
    }

    /// Build (once) or fetch her home adapter. `None` = this node cannot host her.
    async fn local(&self) -> Option<&Arc<dyn AIProviderAdapter>> {
        self.local
            .get_or_init(|| async {
                match self.local_factory.build_adapter(&self.profile).await {
                    Ok(a) => Some(a),
                    Err(e) => {
                        continuum_base::probe!(
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
        let remote = || {
            self.remote_adapter()
                .map(|r| r as Arc<dyn AIProviderAdapter>)
        };
        match self.seat() {
            Seat::Home => home().or_else(remote),
            Seat::Remote => remote().or_else(home),
        }
        .unwrap_or_else(|| self.unavailable.clone()) // JUSTIFIED unwrap_or_else: no seat at all answers by NAME on every generation, never silently
    }

    fn set_seat(&self, seat: Seat, now_ms: u64) {
        self.seat.store(seat as u8, Ordering::Relaxed);
        self.moved_at_ms.store(now_ms, Ordering::Relaxed);
        self.capacity_changes.send_replace(());
    }

    /// Apply one decided move. Returns the org-room line for a move, `None` for Stay/Park
    /// (Park is a probe only — it repeats every tick and would flood the room).
    pub async fn apply(&self, mv: &PlacementMove, now_ms: u64, fit: SeatFit) -> Option<String> {
        match mv {
            PlacementMove::Stay => None,
            PlacementMove::Park { reason } => {
                continuum_base::probe!(
                    class = "persona.placement.parked",
                    persona = %self.persona_name,
                    peer = %self.peer().map(|p| p.to_string()).unwrap_or_default(), // JUSTIFIED unwrap_or_default: probe label only

                    reason = *reason,
                    seat_window = ?fit.window,
                    requirement = ?fit.requirement,
                    "her remote seat is dark and this node has no lane for her — parked, not downgraded"
                );
                None
            }
            PlacementMove::FallHome { reason } => {
                if self.local().await.is_none() {
                    return None;
                }
                self.set_seat(Seat::Home, now_ms);
                crate::cognition::health_ledger::note_move(false);
                continuum_base::probe!(
                    class = "persona.placement.fell_home",
                    persona = %self.persona_name,
                    peer = %self.peer().map(|p| p.to_string()).unwrap_or_default(), // JUSTIFIED unwrap_or_default: probe label only

                    reason = *reason,
                    seat_window = ?fit.window,
                    requirement = ?fit.requirement,
                    "her remote seat went dark — her turns are SERVED by this node's lane until the seat beacons again; residency is unchanged"
                );
                // "RUNS HERE" MEANT TWO THINGS AND COST A PEER A MEASUREMENT (2026-09-21).
                // A reader — me, who had shipped a change to this very file that day —
                // took it as "she is RESIDENT on this node". It means her inference
                // ROUTES to this node's local adapter; residency is a different fact
                // that `persona/instances/list` answers. The line the substrate uses to
                // tell someone where they are must not be ambiguous about the one thing
                // it exists to say.
                Some(format!(
                    "[placement] {} fell home from {} ({}) — her turns are SERVED here (not resident) until the seat beacons again",
                    self.persona_name,
                    self.peer().map(|p| p.to_string()).unwrap_or_default(), // JUSTIFIED unwrap_or_default: line text only
                    reason
                ))
            }
            PlacementMove::ReturnRemote => {
                self.set_seat(Seat::Remote, now_ms);
                crate::cognition::health_ledger::note_move(false);
                continuum_base::probe!(
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
    async fn generate_text(
        &self,
        _request: TextGenerationRequest,
    ) -> Result<TextGenerationResponse, String> {
        Err(format!(
            "{}: no seat — her home adapter could not be built and no remote lane is bound",
            self.0
        ))
    }
}

#[async_trait]
impl AIProviderAdapter for PlacementSwitch {
    async fn prepare_generation(&self) {
        if self.seat() != Seat::Remote {
            return;
        }
        // Typed, pre-dispatch evidence that the seat cannot serve this act. Do not spend
        // another act asking the same peer, or wait for the next placement beacon:
        // - a cold breaker (deadline misses in a row);
        // - the seat refused her last request with every lane busy (card 03660a77:
        //   IntelMac 10-23, Paige asked a full M5 13 times before the placement tick
        //   moved her). Cleared only once she has actually moved: with no home lane, or
        //   this future dropped mid-build, the refusal stands for her next act.
        // FallHome retains the remote adapter and its breaker; ordinary placement policy
        // (a beacon offering a slot, the move cooldown) still owns returning her. This
        // changes inference routing only: RemoteInferenceRequest carries a prompt and
        // returns text, while residency, hands and workspace remain here, so it needs no
        // workspace migration. The synchronous request path records the existing
        // fell_home probe; unlike the grid tick, it does not publish an org-room line.
        let overloaded = self
            .remote_adapter()
            .filter(|remote| remote.seat_overloaded());
        let reason = if self.lane_cold() {
            "remote circuit breaker cold before generation"
        } else if overloaded.is_some() {
            SEAT_OVERLOADED_REASON
        } else {
            return;
        };
        let now = continuum_base::time::EpochMs::now().millis();
        let _ = self
            .apply(&PlacementMove::FallHome { reason }, now, SeatFit::default())
            .await;
        if self.seat() == Seat::Home {
            if let Some(remote) = overloaded {
                remote.clear_seat_overloaded();
            }
        } else {
            // No home lane: she stays on the seat, said so by name. The refusal stands.
            let _ = self
                .apply(&PlacementMove::Park { reason }, now, SeatFit::default())
                .await;
        }
    }

    fn generation_target(&self) -> Option<Arc<dyn AIProviderAdapter>> {
        Some(self.current())
    }

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
    fn warm_ahead(&self, page: crate::inference::slots::PageKey) {
        self.current().warm_ahead(page)
    }
    fn capacity_changed(
        &self,
        observed: Option<u32>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        let mut routes = self.capacity_changes.subscribe();
        let target = self.current();
        Box::pin(async move {
            loop {
                let changed = target.capacity_changed(observed);
                if !Arc::ptr_eq(&target, &self.current()) || self.live_served_window() != observed {
                    return;
                }
                tokio::select! {
                    result = routes.changed() => {
                        if result.is_err() { return std::future::pending::<()>().await; }
                    }
                    _ = changed => return,
                }
            }
        })
    }

    fn live_served_window(&self) -> Option<u32> {
        self.current().live_served_window()
    }
    /// Asked of the adapter her request actually goes to, like every other capability here: a
    /// home lane on our fork's gateway may take her learned tool head, a remote seat may not.
    /// Left to the trait default, every resident read `false` behind this switch, so the head
    /// stayed a shadow on the M5 after #5491 deployed (`delib.tool_head.epoch sent=false`,
    /// Aiko 2026-10-27 00:2xZ, engine 1c6681593 serving).
    fn accepts_tail_tools(&self, model: &str) -> bool {
        self.current().accepts_tail_tools(model)
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
    async fn generate_text(
        &self,
        request: TextGenerationRequest,
    ) -> Result<TextGenerationResponse, String> {
        self.current().generate_text(request).await
    }
    async fn generate_stream(
        &self,
        request: TextGenerationRequest,
        sink: tokio::sync::mpsc::UnboundedSender<GenerationChunk>,
    ) -> Result<TextGenerationResponse, String> {
        self.current().generate_stream(request, sink).await
    }
    async fn create_embedding(
        &self,
        request: EmbeddingRequest,
    ) -> Result<EmbeddingResponse, String> {
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
