//! **RAG is a RenderTarget** — a positron `ViewState` renders to a MIND the same
//! way it renders to eyes, from ONE definition.
//!
//! ## The law this makes real
//!
//! `docs/architecture/ACTIVITY-ROOM-PATTERNS.md` has said it since it was written:
//!
//! > "the same transform serves a human's eyes and a persona's mind, because **RAG
//! > is a render target, not a separate pipeline**" … "the human's UI and the
//! > persona's grounding are the same projection rendered two ways — **they cannot
//! > drift**, because there is one definition" … "**Never render the two from
//! > separate code.**"
//!
//! And the code rendered them from separate code anyway. The human's roster comes
//! from `RosterViewState` on the served `Substrate`; the citizen's came from
//! `persona::room_roster_source`, a second reader with its own fetch, its own
//! freshness, and its own failure modes. Same for the board (`KanbanViewState` vs
//! `room_board_source`) and the wall. Three parallel pairs, one of which was
//! measured delivering a peer's name **zero times** into a citizen's prompt while
//! the browser showed that peer just fine.
//!
//! ## The shape
//!
//! ```text
//!                    ┌── web (DOM) ───────────── pixels
//!   ViewState ──────▶┼── terminal (cells) ────── text
//!   (ONE definition) └── RAG (this module) ───── the citizen's mind
//! ```
//!
//! One generic adapter ([`ViewStateRagSource`]) + one tiny [`RagRenderable`] impl
//! per kind. Adding an activity's grounding is an impl, never a new source: the
//! same "registrations, not builds" property the doc demands of the web side.
//!
//! ## Why this is also the freshness fix
//!
//! The adapter reads the **same `Substrate` the WS server serves**. There is no
//! second fold to lag behind, so the staleness class (#346: a citizen trusting an
//! empty board while the room announcement was fresh) cannot recur here by
//! construction — a citizen and a browser are looking at one cache. `Substrate`
//! is a snapshot cache with interior sharing, so this is a read, not a fetch: no
//! daemon I/O on the compose path.
//!
//! ## Room scope: use [`ViewStateRagSource::per_room`], never a bound room
//!
//! **Superseded 2026-09-07 — the section that stood here said "do not swap a live
//! source onto this, yet", because a single `Substrate` is keyed by kind alone and
//! could hold only the FOCUSED room's roster.** That prerequisite has since been
//! met: `PerRoomSubstrates` gives every room its own substrate, and
//! `ipc/positron_source.rs::sink` writes EVERY room's view into it unconditionally,
//! mirroring only the focused room onto the node substrate. The old text is left
//! described rather than deleted because it was read as current during the #3862
//! diagnosis and sent it one hop sideways.
//!
//! What replaced it is a rule with a sharper edge, learned by shipping the bug the
//! old text warned about in a different shape: the source was bound with
//! `for_room(identity.default_room)`, one handle resolved at construction. The room
//! gate then abstained on every turn the citizen took anywhere else — 662 of 873
//! ticks, bound #general, every turn in #continuum — and she stood among people
//! seeing nobody. Personas are first-class MULTI-room subscribers
//! ([[personas-are-first-class-multi-room-subscribers]]); a room fixed at boot is
//! not a room they stay in.
//!
//! So: a **room-scoped** kind (one whose [`RagRenderable::room`] answers `Some`)
//! is constructed with [`ViewStateRagSource::per_room`], which takes the REGISTRY
//! and resolves the turn's room per delivery. A **node-scoped** kind (one global
//! fold, `room() == None`) keeps [`ViewStateRagSource::new`].
//! The two constructors exist so the right binding is the obvious one and the
//! wrong one has to be chosen deliberately: `per_room` takes the registry, so it
//! CANNOT be handed a single room.
//!
//! **They do not make the wrong binding impossible, and this comment used to say
//! they did.** `new` is public and generic over any `V: RagRenderable`, so
//! `ViewStateRagSource::<RosterViewState>::new(substrate)` — precisely the #3862
//! binding — still compiles. Nothing in the type system refuses it; only this
//! paragraph does, and a comment asserting a guarantee the compiler does not
//! enforce is the exact shape that cost 662 ticks (caught in review of #3879).
//! Making it genuinely unstatable means splitting the scope into the type — a
//! room-scoped and a node-scoped marker, `new` bounded on one and `per_room` on
//! the other, so `room()` returning `Some`/`None` stops being a convention. That
//! is issue #3881 (with a compile-fail test as its acceptance), not a claim to
//! make here in the meantime.
//!
//! ## Density, not truncation
//!
//! Units are ordered most-salient-first and packed to the budget, so a tight
//! window yields FEWER units rather than a chopped block — the "degrade, never
//! all-or-nothing" property `floor_tokens` exists to protect. `#256`'s PX density
//! is exactly this dial, and it belongs to the renderer (here), not the projection.

use async_trait::async_trait;
use serde::de::DeserializeOwned;

use continuum_positron::Substrate;

use crate::cognition::rag_budget::{
    room_scope_allows, RagContext, RagDelivery, RagItem, RagSource,
    ResolutionPreference,
};

/// One atomic projection: visible text and its machine-readable provenance are
/// admitted together. Metadata must describe this unit, not smuggle omitted rows
/// past packing. The adapter alone computes token costs and the truncation stamp.
#[derive(Debug)]
pub struct RagRenderUnit {
    pub content: String,
    pub metadata: serde_json::Map<String, serde_json::Value>,
}

impl From<String> for RagRenderUnit {
    fn from(content: String) -> Self {
        Self {
            content,
            metadata: serde_json::Map::new(),
        }
    }
}

/// A positron `ViewState` rendered for a mind. Kinds own atomic projections;
/// budgeting, packing, cursors and token counting belong to the shared adapter.
pub trait RagRenderable: DeserializeOwned + Send + Sync + 'static {
    /// The positron state kind this renders (e.g. `RosterViewState::KIND`). The
    /// SAME const the web renderer subscribes to — that shared const is what makes
    /// "one definition" enforceable rather than aspirational.
    const KIND: &'static str;

    /// The grounding block label a citizen sees, e.g. `"who is here"`.
    const BLOCK: &'static str;

    /// The verb that yields this in full when the budget could only fit part.
    /// Spelled exactly as a citizen would type it ([[command-names-must-be-accurate]]).
    const EXPAND: Option<&'static str>;

    /// The smallest complete statement this kind can make, in tokens. Same
    /// contract as [`RagSource::floor_tokens`] — measured, not aspirational.
    ///
    /// A FUNCTION, not an associated const, to match how every other RagSource
    /// states its floor (`room_board_source::floor_tokens` returns 32, the roster
    /// and doctrine sources return 0). Shipped first as `const FLOOR_TOKENS: u32`,
    /// which put a second SHAPE on one contract and tripped the de-hardcode guard
    /// (`context_budget::no_new_hardcoded_context_or_prompt_size_constant_anywhere_in_the_crate`,
    /// which scans `const`s whose name contains TOKEN for bare literals). The guard
    /// was right to fire on a fresh shape in the file that is meant to be the
    /// template every future ViewState source is copied from — one contract, one
    /// spelling.
    ///
    /// This is a per-UNIT content floor ("one roster line"), not a context bound:
    /// it scales with what a unit costs to say, not with the served window, which
    /// is why it is a measured constant here and a fraction nowhere.
    fn floor_tokens() -> u32;

    /// Source-owned appetite, shared with non-ViewState grounding sources.
    fn budget_claim(window: u32, headroom: u32) -> crate::cognition::rag_budget::RagSourceBudget {
        crate::cognition::rag_budget::RagSourceBudget::retrieval(
            Self::KIND,
            Self::floor_tokens(),
            window,
            headroom,
        )
    }

    /// Atomic units, **most-salient first**. Each must stand alone: the adapter
    /// packs a prefix of this list and drops the rest, so unit `n` may never
    /// depend on unit `n+1` having been included.
    ///
    /// An empty vec is the honest "nothing to say" and renders NO block — never a
    /// header with nothing under it ([[fallbacks-are-illegal-fail-loud]]: an empty
    /// roster must read as empty, not as a fabricated presence).
    ///
    /// `viewer` is the persona this render is FOR (`RagContext::persona_id`). A view
    /// that lists people needs it to say which one is the reader; kinds that describe
    /// the node ignore it.
    ///
    /// It is an ARGUMENT rather than something the renderable holds because one
    /// `ViewStateRagSource<V>` serves every persona subscribed to that kind — the
    /// view is shared, the reader is not. Shipped first as `units(&self)`, which made
    /// the self-marker below impossible to write: the renderable had no "me" to
    /// compare a slot against, and the adapter (which DID hold `persona_id`, for the
    /// continuation cursor) was one frame too far out to fix it without
    /// post-processing another module's line format.
    fn units(&self, viewer: uuid::Uuid) -> Vec<RagRenderUnit>;

    /// Which room this view describes, when it describes one.
    ///
    /// Room-scoped kinds (roster, chat, kanban, wall) answer `Some`, and the
    /// adapter runs them through [`room_scope_allows`] — the ONE room gate every
    /// room-scoped source already shares — so a turn in room B can never receive
    /// room A's people. Node-scoped kinds (serving, metrics)
    /// answer `None`: they describe the NODE, not a room, and gating them on a
    /// room would blank them on every turn.
    ///
    /// Defaulted to `None` because node-scope is the safe answer: a kind that
    /// forgot to override renders where it is asked, rather than silently
    /// abstaining everywhere.
    fn room(&self) -> Option<uuid::Uuid> {
        None
    }
}

/// Rough token estimate. The allocator's contract is `tokens_used <= budget`, so
/// this errs HIGH: over-estimating costs a unit, under-estimating overruns the
/// window, and only one of those corrupts a turn.
///
/// A shared helper rather than a per-source guess — the divergent hand-rolled
/// estimates are exactly how sources ended up asking for 12-80x their real size.
pub(crate) fn estimate_tokens(text: &str) -> u32 {
    // ~3.5 chars/token is conservative for English prose with punctuation; the
    // ceil keeps a one-word unit from estimating as free.
    ((text.len() as f32 / 3.5).ceil() as u32).max(1)
}

/// THE adapter: any [`RagRenderable`] `ViewState` is a [`RagSource`].
///
/// Generic over the kind, so N kinds cost N small impls and zero new plumbing.
pub struct ViewStateRagSource<V: RagRenderable> {
    /// Which substrate this kind reads. See [`SubstrateBinding`] — a room-scoped
    /// kind resolves the TURN's room per delivery; a node-scoped one holds one
    /// handle for the life of the source.
    binding: SubstrateBinding,
    _kind: std::marker::PhantomData<V>,
}

/// Where a [`ViewStateRagSource`] reads its view from.
///
/// The distinction is not an optimization — it is the difference between a citizen
/// seeing the room she is IN and seeing a room she is not in. `PerRoomSubstrates`
/// already stores every room's view on every projection (`positron_source.rs`
/// writes `rooms.for_room(room_id)` unconditionally and mirrors only the FOCUSED
/// room onto the node substrate), so the per-room store is populated for rooms
/// nothing has ever read.
enum SubstrateBinding {
    /// ONE substrate for every turn. Correct only for kinds that describe the NODE
    /// rather than a room (a single global fold), which are the
    /// same kinds whose [`RagRenderable::room`] answers `None`.
    Node(Substrate),
    /// Resolved per delivery from the turn's room. The registry is held, never a
    /// single room's handle: binding one room at construction is what made a
    /// citizen abstain on every turn outside it (#3862 — the roster was bound to
    /// `identity.default_room` and the citizen worked elsewhere, so
    /// `room_scope_allows` correctly refused 662 of 873 ticks).
    PerRoom(std::sync::Arc<continuum_positron::scoping::PerRoomSubstrates>),
}

/// A kind that describes THE NODE, not a room: serving, metrics.
/// Its [`RagRenderable::room`] answers `None`, and it is the only shape
/// [`ViewStateRagSource::new`] accepts.
///
/// This marker exists because a doc comment was carrying the rule. #3879 claimed
/// "the two constructors make the wrong binding unstatable"; they did not --
/// `new` was generic over every `RagRenderable`, so
/// `ViewStateRagSource::<RosterViewState>::new(substrate)` (which IS the #3862
/// defect) compiled fine and only this paragraph refused it. A comment asserting
/// a guarantee the compiler does not enforce is the exact shape that cost 662
/// abstains, so the guarantee moved into the type system (#3881).
pub trait NodeScopedView: RagRenderable {}

/// A kind that describes ONE ROOM: the roster, and every per-room view after it.
/// Its [`RagRenderable::room`] answers `Some`, and it can ONLY be constructed
/// through [`ViewStateRagSource::per_room`], which takes the registry and
/// resolves the turn's room per delivery.
///
/// The invariant, in @IntelMac's words and credited to them: ANY SOURCE HOLDING A
/// BOUND `room_id` MUST CONSULT THE TURN'S ROOM BEFORE READING IT. A room-scoped
/// kind cannot be handed a single `Substrate`, so it cannot hold a bound room at
/// all -- the invariant holds by construction rather than by review.
pub trait RoomScopedView: RagRenderable {}

/// THE ACCEPTANCE FOR #3881, as a compile-fail proof rather than a claim.
///
/// This is the #3862 defect written out: handing a ROOM-scoped kind a single
/// `Substrate`, so it reads one room forever while citizens work in others. It
/// compiled before this change, and the only thing refusing it was a paragraph.
///
/// ```compile_fail
/// use continuum_mind::persona::viewstate_rag::ViewStateRagSource;
/// use continuum_positron::{RosterViewState, Substrate};
/// // `new` is bound on NodeScopedView; the roster is RoomScopedView.
/// let _ = ViewStateRagSource::<RosterViewState>::new(Substrate::new());
/// ```
///
/// And the mirror, which MUST still build -- a guard that refuses everything is
/// not a guard, and this is the half that catches an over-tight bound:
///
/// ```
/// use continuum_mind::persona::viewstate_rag::{NodeScopedView, RagRenderable, RagRenderUnit, ViewStateRagSource};
/// use continuum_positron::Substrate;
/// // No production kind is node-scoped, so the mirror carries its own: a
/// // kind that genuinely describes the node still constructs with one substrate.
/// #[derive(serde::Deserialize)]
/// struct NodeTemperature { celsius: f32 }
/// impl RagRenderable for NodeTemperature {
///     const KIND: &'static str = "node-temperature";
///     const BLOCK: &'static str = "node temperature";
///     const EXPAND: Option<&'static str> = None;
///     fn floor_tokens() -> u32 { 4 }
///     fn units(&self, _viewer: uuid::Uuid) -> Vec<RagRenderUnit> { vec![format!("{} C", self.celsius).into()] }
/// }
/// impl NodeScopedView for NodeTemperature {}
/// let _ = ViewStateRagSource::<NodeTemperature>::new(Substrate::new());
/// ```
///
/// A doctest is used deliberately: `trybuild` would be a new dev-dependency for
/// one assertion, and `compile_fail` is in the toolchain already. The cost is
/// that it runs only under `cargo test --doc`, and CI ran `--lib` only -- so
/// this guard would have merged and never executed once. `.github/workflows/
/// continuum-rust-tests.yml` gained a step that runs it by name and asserts
/// `2 passed`, because a filter matching NOTHING exits 0 and a renamed struct
/// would otherwise leave a green step testing nothing.
pub struct BindingIsUnstatable;

impl<V: RagRenderable> ViewStateRagSource<V> {
    /// Read + deserialize the current view, or `None` when the kind has never been
    /// stored (a cold boot before the first projection) or the payload does not
    /// match this build's shape. Both are honest absences: no block is rendered.
    ///
    /// A room-scoped source on a turn with NO room (`ctx.airc_room == None` —
    /// background consolidation, an idle tick) also answers `None`, and says so:
    /// there is no room whose people it could honestly report. That is an absence,
    /// not a fallback to some other room's store.
    fn current(&self, ctx: &RagContext) -> Option<V> {
        let substrate = match &self.binding {
            SubstrateBinding::Node(s) => s.clone(),
            // `read_room`, NOT `for_room`: this is a READ path that now runs with
            // whatever room the turn is in, and `for_room` inserts on lookup. An
            // inserting read would let a citizen wandering rooms accrete a
            // permanently-empty substrate per distinct room and inflate
            // `room_count` with rooms nothing ever projected (caught in review of
            // #3879). A room with no projection has no view — the same honest
            // absence as a room whose kind is missing.
            SubstrateBinding::PerRoom(rooms) => match ctx.airc_room.as_ref() {
                Some(room) => rooms.read_room(room.as_uuid())?,
                None => {
                    continuum_base::probe!(
                        class = "rag.room_scope.no_room",
                        source = %V::KIND,
                        persona_id = %ctx.persona_id,
                        "room-scoped view has no room to read: this turn carries no room context"
                    );
                    return None;
                }
            },
        };
        let envelope = substrate.cache().get(V::KIND)?;
        serde_json::from_value(envelope.payload.clone()).ok()
    }

    /// Pack as many whole units as fit starting at index `from`. Ruling-a:
    /// only `deliver` calls this now, always from zero — every turn
    /// re-derives fresh, so packing rules can never diverge between calls.
    fn pack(
        &self,
        units: Vec<RagRenderUnit>,
        budget: u32,
        from: usize,
    ) -> (Vec<RagItem>, u32, usize) {
        let mut items = Vec::new();
        let mut used = 0u32;
        let mut next = from;
        for mut unit in units.into_iter().skip(from) {
            let cost = estimate_tokens(&unit.content);
            if used + cost > budget {
                break;
            }
            used += cost;
            next += 1;
            unit.metadata
                .insert("kind".into(), serde_json::json!(V::KIND));
            unit.metadata
                .insert("block".into(), serde_json::json!(V::BLOCK));
            items.push(RagItem {
                content: unit.content,
                tokens: cost,
                metadata: serde_json::Value::Object(unit.metadata),
            });
        }
        (items, used, next)
    }
}

impl<V: RagRenderable + NodeScopedView> ViewStateRagSource<V> {
    /// A NODE-scoped source: one substrate, every turn. Use only for a kind whose
    /// [`RagRenderable::room`] returns `None`.
    pub fn new(substrate: Substrate) -> Self {
        Self {
            binding: SubstrateBinding::Node(substrate),
            _kind: std::marker::PhantomData,
        }
    }
}

impl<V: RagRenderable + RoomScopedView> ViewStateRagSource<V> {
    /// A ROOM-scoped source: reads the store of whatever room the turn is in.
    ///
    /// This is the constructor a room-scoped kind must use. It takes the REGISTRY,
    /// not a room, precisely so no caller can bind a room at construction time.
    pub fn per_room(rooms: std::sync::Arc<continuum_positron::scoping::PerRoomSubstrates>) -> Self {
        Self {
            binding: SubstrateBinding::PerRoom(rooms),
            _kind: std::marker::PhantomData,
        }
    }
}

#[async_trait]
impl<V: RagRenderable> RagSource for ViewStateRagSource<V> {
    fn source_id(&self) -> &'static str {
        V::KIND
    }

    fn expand_command(&self) -> Option<&'static str> {
        V::EXPAND
    }

    fn floor_tokens(&self) -> u32 {
        V::floor_tokens()
    }

    fn budget_claim(&self, window: u32, headroom: u32) -> crate::cognition::rag_budget::RagSourceBudget {
        V::budget_claim(window, headroom)
    }


    async fn deliver(
        &self,
        _ctx: &RagContext,
        budget: u32,
        resolution: ResolutionPreference,
    ) -> RagDelivery {
        let view = self.current(_ctx);
        // The ONE room gate, not a second copy of it: a room-scoped kind whose
        // view describes a DIFFERENT room than this turn abstains, with the same
        // probe every other room-scoped source emits.
        let units = match view {
            Some(v) if room_scope_allows(v.room(), _ctx, V::KIND) => v.units(_ctx.persona_id),
            _ => Vec::new(),
        };
        let total = units.len();
        let (items, tokens_used, next) = self.pack(units, budget, 0);
        RagDelivery {
            source_id: V::KIND.to_string(),
            items,
            tokens_used,
            // Ruling a: no resumable handle — the flag says "more exists" and
            // the next deliver re-derives from scratch (a larger budget simply
            // fits more). See rag_budget for the two readers of this flag.
            truncated: next < total,
            resolution_used: resolution,
        }
    }

}

// ─────────────────────── OUTLIER A: the roster (people) ───────────────────────

/// The roster describes ONE room, so it is room-scoped and can only be built
/// through `per_room`. See [`RoomScopedView`].
impl RoomScopedView for continuum_positron::RosterViewState {}

/// Who is present, rendered for a mind from the SAME `RosterViewState` the web
/// roster renders.
///
/// This is the measured defect's cure: a citizen's prompt contained a live peer's
/// name **zero times** while the framing prose promised "who is present", because
/// her roster came from a different reader than the browser's
/// ([[citizens-cannot-see-each-other-the-prompt-promises-presence-and-delivers-nothing]]).
///
/// Unit = one member, because a half-rendered person is not a person. Ordering
/// follows the projection's own order (presence order), so the citizen and the
/// browser list the room the same way.
impl RagRenderable for continuum_positron::RosterViewState {
    const KIND: &'static str = continuum_positron::RosterViewState::KIND;
    const BLOCK: &'static str = "who is here";
    const EXPAND: Option<&'static str> = Some("room/members");
    /// One member line, measured: a name plus a short role runs ~10 tokens. The
    /// floor is ONE PERSON — under any budget that admits this source at all, a
    /// citizen should learn that at least someone is here.
    fn budget_claim(window: u32, headroom: u32) -> crate::cognition::rag_budget::RagSourceBudget {
        super::room_roster_source::roster_budget_claim(Self::KIND, window, headroom)
    }

    fn floor_tokens() -> u32 {
        10
    }

    fn units(&self, viewer: uuid::Uuid) -> Vec<RagRenderUnit> {
        // STABLE ORDER for the mind (2026-09-01, Benchy hit_rate=0.0): this
        // block renders at ~0.4% prompt depth, and "presence order" rotates
        // as heartbeats land — his consecutive prompts diverged at char ~226
        // on exactly this rotation, re-prefilling the whole ~50k tail every
        // turn while peers cached 0.4-0.8. The browser may sort presence-first
        // for human eyes; the MIND's copy sorts by name so the same people are
        // the same bytes. Set parity with the screen is unchanged — only the
        // line order is pinned ([[a-mutating-system-prompt-destroys-kv-reuse]]).
        let mut ordered: Vec<_> = self.roster.iter().collect();
        ordered.sort_by(|a, b| {
            a.display_name
                .cmp(&b.display_name)
                .then_with(|| a.member_id.cmp(&b.member_id))
        });
        ordered
            .into_iter()
            .map(|slot| {
                // Kind and role are what make a name actionable ("who can I ask?"),
                // so they ride the SAME unit as the name rather than a second block
                // a tight budget would sever from it.
                // WHICH ONE IS HER (2026-09-06, card bb387821): a citizen appeared in
                // her own roster formatted identically to her peers, with no marker.
                // Measured on IntelMac, BOTH directions, both with correct prompts —
                // Paige's prompt said "You are Paige" five times plus an explicit
                // NOT-list and she answered "I am Saoirse"; Saoirse's said "You are
                // Saoirse" and she answered "My name is Paige". The prose asserted an
                // identity five times while the STRUCTURED list beside it presented two
                // interchangeable agents, and the structure won. Marking the reader
                // costs four characters and removes the ambiguity the prose was losing
                // to. She stays IN the list — a citizen is present in her own room, and
                // deleting her row would make "who is here" undercount the room.
                let mut line = if slot.member_id == viewer {
                    format!("{} ({:?}, YOU)", slot.display_name, slot.kind)
                } else {
                    format!("{} ({:?})", slot.display_name, slot.kind)
                };
                if let Some(role) = slot.role_label.as_deref().filter(|r| !r.is_empty()) {
                    line.push_str(&format!(" — {role}"));
                }
                // NO liveness flag in the mind's copy (2026-09-04): `[ready]`/`[busy]`
                // flips with every heartbeat, and this block sits inside the STABLE
                // system prefix — consecutive prompts of one citizen diverged at char
                // ~9.7k of a 10.1k system on exactly this flag, forfeiting the whole
                // prefix (hit_rate 0.0 on 57/57 turns). Who is here, their kind and
                // role, are the stable facts; whether a peer is busy this second is
                // presence — the live view carries it, and it belongs in the volatile
                // tail if a turn ever needs it ([[a-mutating-system-prompt-destroys-kv-reuse]]).
                let mut unit = RagRenderUnit::from(line);
                unit.metadata
                    .insert("peer_id".into(), serde_json::json!(slot.member_id));
                unit.metadata
                    .insert("display_name".into(), serde_json::json!(slot.display_name));
                unit
            })
            .collect()
    }

    /// The roster describes ONE room, so it rides the shared room gate.
    fn room(&self) -> Option<uuid::Uuid> {
        Some(self.room_id)
    }
}

mod tests;
