//! Cognition's extension slots: the verbs cognition calls that the persona layer owns. Persona
//! fills them at boot (`persona::cognition_extensions::install`); cognition names the contract
//! here and never imports the owner. The shape is `continuum-kernel`'s `runtime::extensions`:
//! one `RwLock<Option<…>>` per slot with an `install_*` beside it; a second install replaces
//! the first.
//!
//! An empty slot is LOUD, never a silent fallback: a call through one emits
//! `cognition.extension.missing` naming the slot, then does the no-op its doc states (or
//! returns `None`, which the caller already handles as a typed absence). A process that never
//! booted the persona layer (a unit test, a tool) meets the probe; a production core that
//! forgot an install shows it on the first call.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use parking_lot::RwLock;
use uuid::Uuid;

/// A boxed future, so an async verb can sit in a slot as a plain `fn`.
pub type SlotFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

fn missing(slot: &'static str) {
    continuum_base::probe!(
        class = "cognition.extension.missing",
        slot = slot,
        "cognition called a verb the persona layer owns and nothing installed it; the slot's documented no-op ran"
    );
}

// ─── Her room voice ──────────────────────────────────────────────────────────────────────

/// Publish `text` into `room` as `persona`, through her airc citizen. `None` when she has no
/// airc runtime on this node (nothing to speak through); `Some(Err)` when the publish failed.
pub type RoomVoice = fn(persona: Uuid, room: Uuid, text: String) -> SlotFuture<Option<Result<(), String>>>;

static ROOM_VOICE: RwLock<Option<RoomVoice>> = RwLock::new(None);

pub fn install_room_voice(voice: RoomVoice) {
    *ROOM_VOICE.write() = Some(voice);
}

/// Speak into a room as her. Empty slot: probed, and nothing is published (`None`).
pub async fn publish_in_room(persona: Uuid, room: Uuid, text: String) -> Option<Result<(), String>> {
    let Some(voice) = *ROOM_VOICE.read() else {
        missing("room_voice");
        return None;
    };
    voice(persona, room, text).await
}

// ─── The card carry ──────────────────────────────────────────────────────────────────────

/// Wake the one carry owner, which commits and pushes every resident's card work.
pub type CardCarry = fn();

static CARD_CARRY: RwLock<Option<CardCarry>> = RwLock::new(None);

pub fn install_card_carry(carry: CardCarry) {
    *CARD_CARRY.write() = Some(carry);
}

/// Ask for a carry. Empty slot: probed, and nothing is carried until the next owner wake.
pub fn request_card_carry() {
    match *CARD_CARRY.read() {
        Some(carry) => carry(),
        None => missing("card_carry"),
    }
}

// ─── Training credit ─────────────────────────────────────────────────────────────────────

/// Where cognition's judgments become training credit (persona's training producer).
pub trait CreditSink: Send + Sync + 'static {
    /// Stage a gated echo (`marker`: the gate that refused it) as a speech-discipline example.
    fn speech_discipline(&self, persona_id: Uuid, persona_name: String, base_model: String, burst: String, marker: &'static str);
    /// Settle the staged credit of every card that worked `instance`.
    fn settle_instance(&self, instance: &str, passed: bool);
    /// Settle one card's staged credit.
    fn settle_card(&self, card: Uuid, passed: bool) -> SlotFuture<()>;
}

static CREDIT_SINK: RwLock<Option<Arc<dyn CreditSink>>> = RwLock::new(None);

pub fn install_credit_sink(sink: Arc<dyn CreditSink>) {
    *CREDIT_SINK.write() = Some(sink);
}

fn credit_sink(slot: &'static str) -> Option<Arc<dyn CreditSink>> {
    let sink = CREDIT_SINK.read().clone();
    if sink.is_none() {
        missing(slot);
    }
    sink
}

/// Empty slot: probed, and the echo is not staged.
pub fn produce_speech_discipline(persona_id: Uuid, persona_name: String, base_model: String, burst: String, marker: &'static str) {
    if let Some(sink) = credit_sink("credit_sink.speech_discipline") {
        sink.speech_discipline(persona_id, persona_name, base_model, burst, marker);
    }
}

/// Empty slot: probed, and the instance's credit stays staged.
pub fn settle_instance_credit(instance: &str, passed: bool) {
    if let Some(sink) = credit_sink("credit_sink.settle_instance") {
        sink.settle_instance(instance, passed);
    }
}

/// Empty slot: probed, and the card's credit stays staged.
pub async fn settle_card_credit(card: Uuid, passed: bool) {
    if let Some(sink) = credit_sink("credit_sink.settle_card") {
        sink.settle_card(card, passed).await;
    }
}

// ─── Her pulse and her perception marks ──────────────────────────────────────────────────

/// Stamp that she is thinking right now (`now_ms`), the claim-renewal gate's witness.
pub type CognitionPulse = fn(persona: Uuid, now_ms: u64);

static COGNITION_PULSE: RwLock<Option<CognitionPulse>> = RwLock::new(None);

pub fn install_cognition_pulse(pulse: CognitionPulse) {
    *COGNITION_PULSE.write() = Some(pulse);
}

/// Empty slot: probed, and no stamp is written (renewal reads her older stamp).
pub fn touch_cognition_pulse(persona: Uuid, now_ms: u64) {
    match *COGNITION_PULSE.read() {
        Some(pulse) => pulse(persona, now_ms),
        None => missing("cognition_pulse"),
    }
}

/// Mark a room line perceived in her mind state: `room`, its `lamport` and `event`, at `now_ms`.
pub type MarkPerceived = fn(persona: Uuid, room: Uuid, lamport: u64, event: Option<Uuid>, now_ms: u64);

static MARK_PERCEIVED: RwLock<Option<MarkPerceived>> = RwLock::new(None);

pub fn install_mark_perceived(mark: MarkPerceived) {
    *MARK_PERCEIVED.write() = Some(mark);
}

/// Empty slot: probed, and her cursor does not move (the line reads unperceived).
pub fn mark_perceived(persona: Uuid, room: Uuid, lamport: u64, event: Option<Uuid>, now_ms: u64) {
    match *MARK_PERCEIVED.read() {
        Some(mark) => mark(persona, room, lamport, event, now_ms),
        None => missing("mark_perceived"),
    }
}

// ─── Her model binding ───────────────────────────────────────────────────────────────────

/// Whether the persona named `persona_name` under `continuum_root` is bound to an off-box
/// (remote) brain; `Err` when her binding cannot be read.
pub type RemoteBinding = fn(continuum_root: &Path, persona_name: &str) -> Result<bool, String>;

static REMOTE_BINDING: RwLock<Option<RemoteBinding>> = RwLock::new(None);

pub fn install_remote_binding(binding: RemoteBinding) {
    *REMOTE_BINDING.write() = Some(binding);
}

/// Empty slot: probed, and the binding reads as unreadable (`Err`), which keeps her current one.
pub fn is_remote_bound(continuum_root: &Path, persona_name: &str) -> Result<bool, String> {
    match *REMOTE_BINDING.read() {
        Some(binding) => binding(continuum_root, persona_name),
        None => {
            missing("remote_binding");
            Err("no remote-binding reader installed".to_string())
        }
    }
}

// ─── The fleet's quiesce lease ───────────────────────────────────────────────────────────

/// The whole fleet suspended for a measurement, released when dropped.
pub struct FleetLease {
    /// How many residents it suspended.
    pub count: usize,
    _held: Box<dyn Send + Sync>,
}

impl FleetLease {
    pub fn new(count: usize, held: Box<dyn Send + Sync>) -> Self {
        Self { count, _held: held }
    }
}

/// Quiesce every resident; `None` when no fleet is up.
pub type QuiesceFleet = fn() -> Option<FleetLease>;

static QUIESCE_FLEET: RwLock<Option<QuiesceFleet>> = RwLock::new(None);

pub fn install_quiesce_fleet(quiesce: QuiesceFleet) {
    *QUIESCE_FLEET.write() = Some(quiesce);
}

/// Empty slot: probed, and nothing is quiesced (`None`, the no-fleet answer).
pub fn quiesce_fleet() -> Option<FleetLease> {
    match *QUIESCE_FLEET.read() {
        Some(quiesce) => quiesce(),
        None => {
            missing("quiesce_fleet");
            None
        }
    }
}

// ─── The card board ──────────────────────────────────────────────────────────────────────

/// The room whose board holds `card`, read through `persona`'s airc citizen; `None` when no
/// board she stands in shows it (or she has no citizen here).
pub type CardRoom = fn(persona: Uuid, card: Uuid) -> SlotFuture<Option<Uuid>>;

static CARD_ROOM: RwLock<Option<CardRoom>> = RwLock::new(None);

pub fn install_card_room(lookup: CardRoom) {
    *CARD_ROOM.write() = Some(lookup);
}

/// Empty slot: probed, and no board is found (`None`).
pub async fn room_holding_card(persona: Uuid, card: Uuid) -> Option<Uuid> {
    let Some(lookup) = *CARD_ROOM.read() else {
        missing("card_room");
        return None;
    };
    lookup(persona, card).await
}

// ─── The pinned workspace map ────────────────────────────────────────────────────────────

/// The workspace-map grounding source for `persona`, rooted at a pinned `root`.
pub type PinnedWorkspaceMap = fn(persona: Uuid, root: &Path) -> Arc<dyn crate::cognition::rag_budget::RagSource>;

static PINNED_WORKSPACE_MAP: RwLock<Option<PinnedWorkspaceMap>> = RwLock::new(None);

pub fn install_pinned_workspace_map(source: PinnedWorkspaceMap) {
    *PINNED_WORKSPACE_MAP.write() = Some(source);
}

/// Empty slot: probed, and `None`: the map keeps the root it was built with.
pub fn pinned_workspace_map(persona: Uuid, root: &Path) -> Option<Arc<dyn crate::cognition::rag_budget::RagSource>> {
    match *PINNED_WORKSPACE_MAP.read() {
        Some(source) => Some(source(persona, root)),
        None => {
            missing("pinned_workspace_map");
            None
        }
    }
}
