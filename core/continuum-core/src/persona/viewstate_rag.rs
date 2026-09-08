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
//! and resolves the turn's room per delivery. A **node-scoped** kind (the bench
//! board — one global fold, `room() == None`) keeps [`ViewStateRagSource::new`].
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
//! is a follow-up, not a claim to make here in the meantime.
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

use super::rag_budget::{
    room_scope_allows, ContinuationCursor, RagContext, RagDelivery, RagItem, RagSource,
    ResolutionPreference,
};

/// A positron `ViewState` kind that knows how to render itself for a MIND.
///
/// Deliberately tiny: identity (`KIND`), a label, the verb that shows the whole
/// thing, and the atomic units. Everything else — budgeting, packing, cursors,
/// token counting, the honest-empty case — is the adapter's job, written once.
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
    fn units(&self, viewer: uuid::Uuid) -> Vec<String>;

    /// Which room this view describes, when it describes one.
    ///
    /// Room-scoped kinds (roster, chat, kanban, wall) answer `Some`, and the
    /// adapter runs them through [`room_scope_allows`] — the ONE room gate every
    /// room-scoped source already shares — so a turn in room B can never receive
    /// room A's people. Node-scoped kinds (the bench board, serving, metrics)
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
fn estimate_tokens(text: &str) -> u32 {
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
    /// rather than a room (the bench board is a single global fold), which are the
    /// same kinds whose [`RagRenderable::room`] answers `None`.
    Node(Substrate),
    /// Resolved per delivery from the turn's room. The registry is held, never a
    /// single room's handle: binding one room at construction is what made a
    /// citizen abstain on every turn outside it (#3862 — the roster was bound to
    /// `identity.default_room` and the citizen worked elsewhere, so
    /// `room_scope_allows` correctly refused 662 of 873 ticks).
    PerRoom(std::sync::Arc<continuum_positron::scoping::PerRoomSubstrates>),
}

impl<V: RagRenderable> ViewStateRagSource<V> {
    /// A NODE-scoped source: one substrate, every turn. Use only for a kind whose
    /// [`RagRenderable::room`] returns `None`.
    pub fn new(substrate: Substrate) -> Self {
        Self {
            binding: SubstrateBinding::Node(substrate),
            _kind: std::marker::PhantomData,
        }
    }

    /// A ROOM-scoped source: reads the store of whatever room the turn is in.
    ///
    /// This is the constructor a room-scoped kind must use. It takes the REGISTRY,
    /// not a room, precisely so no caller can bind a room at construction time.
    pub fn per_room(
        rooms: std::sync::Arc<continuum_positron::scoping::PerRoomSubstrates>,
    ) -> Self {
        Self {
            binding: SubstrateBinding::PerRoom(rooms),
            _kind: std::marker::PhantomData,
        }
    }

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
                    crate::probe!(
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

    /// Pack as many whole units as fit. Shared by `deliver` and the continuation
    /// path so a resumed delivery can never use different packing rules than the
    /// first one.
    fn pack(&self, units: Vec<String>, budget: u32, from: usize) -> (Vec<RagItem>, u32, usize) {
        let mut items = Vec::new();
        let mut used = 0u32;
        let mut next = from;
        for unit in units.into_iter().skip(from) {
            let cost = estimate_tokens(&unit);
            if used + cost > budget {
                break;
            }
            used += cost;
            next += 1;
            items.push(RagItem {
                content: unit,
                tokens: cost,
                metadata: serde_json::json!({ "kind": V::KIND, "block": V::BLOCK }),
            });
        }
        (items, used, next)
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
            // A cursor ONLY when there is genuinely more — an unconditional cursor
            // would have the allocator resume a source with nothing left, spending
            // a future turn's budget on an empty delivery.
            continuation: (next < total).then(|| ContinuationCursor {
                persona_id: _ctx.persona_id,
                source_id: V::KIND.to_string(),
                // The resume state IS the next unit index — the allocator never
                // inspects it, so keeping it a plain offset is the whole cursor.
                opaque: serde_json::json!({ "next": next }),
            }),
            resolution_used: resolution,
        }
    }

    async fn deliver_continuation(
        &self,
        _ctx: &RagContext,
        cursor: ContinuationCursor,
        budget: u32,
    ) -> Option<RagDelivery> {
        // A cursor from a DIFFERENT source is not ours to interpret — the trait
        // documents this as a stale-cursor case, and guessing would render one
        // kind's content under another's label.
        if cursor.source_id != V::KIND {
            return None;
        }
        // Substrate-side identity check the trait REQUIRES: a cursor issued for
        // another persona must never resume here.
        if cursor.persona_id != _ctx.persona_id {
            return None;
        }
        let from = cursor.opaque.get("next")?.as_u64()? as usize;
        let view = self.current(_ctx)?;
        if !room_scope_allows(view.room(), _ctx, V::KIND) {
            return None;
        }
        let units = view.units(_ctx.persona_id);
        let total = units.len();
        if from >= total {
            return None;
        }
        let (items, tokens_used, next) = self.pack(units, budget, from);
        if items.is_empty() {
            return None;
        }
        Some(RagDelivery {
            source_id: V::KIND.to_string(),
            items,
            tokens_used,
            continuation: (next < total).then(|| ContinuationCursor {
                persona_id: _ctx.persona_id,
                source_id: V::KIND.to_string(),
                opaque: serde_json::json!({ "next": next }),
            }),
            resolution_used: ResolutionPreference::Raw,
        })
    }
}

// ─────────────────────── OUTLIER A: the roster (people) ───────────────────────

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
    fn floor_tokens() -> u32 {
        10
    }

    fn units(&self, viewer: uuid::Uuid) -> Vec<String> {
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
                line
            })
            .collect()
    }

    /// The roster describes ONE room, so it rides the shared room gate.
    fn room(&self) -> Option<uuid::Uuid> {
        Some(self.room_id)
    }
}

// ──────────────── OUTLIER B: the benchmark board (numbers, no identity) ────────

/// A benchmark run's live rows, rendered for a mind from the SAME `BenchViewState`
/// the academy rail renders.
///
/// Chosen as outlier B precisely because it is maximally unlike the roster: no
/// identity, no presence, numeric state that changes every act, and a per-row
/// verdict. If ONE adapter serves both without forcing, the seam is proven and
/// chat / kanban / wall / serving / nav / foundry are registrations, not builds.
///
/// It is also the benchmarks-as-activity payoff: a citizen standing in the run's
/// room can perceive the run's state through the same pipe the human's screen
/// uses, which is the acceptance test
/// ([[benchmarks-must-be-positronic-activities-not-a-parallel-subsystem]]).
impl RagRenderable for continuum_positron::bench::BenchViewState {
    const KIND: &'static str = continuum_positron::bench::BenchViewState::KIND;
    const BLOCK: &'static str = "benchmark runs";
    const EXPAND: Option<&'static str> = Some("benchmark/runs");
    /// One run row, measured: id + instance + phase + a score fraction ~ 18 tokens.
    fn floor_tokens() -> u32 {
        18
    }

    /// Node-scoped: the bench board describes THIS NODE, not the people in a room,
    /// so every reader sees the identical text and `_viewer` is deliberately unused.
    fn units(&self, _viewer: uuid::Uuid) -> Vec<String> {
        // Rounds first: the lifecycle truth a citizen orients on before the
        // per-run rows (#371) — the SAME scoreboard the human's screen renders,
        // which is the positronic-parity acceptance test.
        let rounds = self.rounds.iter().map(|r| {
            format!(
                "round {} {} {}: {}/{} settled, {} remaining ({})",
                &r.round_id[..8.min(r.round_id.len())],
                r.benchmark,
                r.stage,
                r.settled,
                r.dispatched,
                r.remaining,
                r.driver,
            )
        });
        rounds
            .chain(self.runs.iter().map(|row| {
                let mut line = format!("{} {}", row.run_id, row.phase);
                if let Some(instance) = row.instance.as_deref() {
                    line.push_str(&format!(" · {instance}"));
                }
                if let Some(solver) = row.solver.as_deref() {
                    line.push_str(&format!(" · {solver}"));
                }
                if let (Some(attempt), Some(max)) = (row.attempt, row.max_attempts) {
                    line.push_str(&format!(" · attempt {attempt}/{max}"));
                }
                if let Some(f2p) = row.fail_to_pass.as_deref() {
                    line.push_str(&format!(" · f2p {f2p}"));
                }
                // An infra error is NOT a capability result, and a citizen reading
                // the board must be able to tell them apart — the same
                // absence-vs-zero distinction the grade wire carries.
                if let Some(err) = row.infra_error.as_deref() {
                    line.push_str(&format!(" · UNGRADEABLE ({err})"));
                } else if let Some(resolved) = row.resolved {
                    line.push_str(if resolved { " · RESOLVED" } else { " · unresolved" });
                }
                line
            }))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use continuum_positron::bench::{BenchRunRow, BenchViewState};
    use continuum_positron::{RosterViewState, SenderKind, StateBuilder};
    use uuid::Uuid;

    use continuum_positron::scoping::PerRoomSubstrates;
    use std::sync::Arc;

    /// The room every roster fixture below is stamped with, and the one [`ctx`]
    /// puts the turn in.
    const ROOM: u128 = 0xaa;

    /// Store a roster for `room` in that room's OWN substrate and hand back the
    /// registry — the shape production uses. Tests go through the registry rather
    /// than a bare `Substrate` so they exercise the live binding: a roster test
    /// that constructs a single substrate cannot fail when the source binds the
    /// wrong room, which is exactly how #3862 stayed green.
    fn roster_registry_for(room: u128, names: &[&str]) -> Arc<PerRoomSubstrates> {
        let rooms = Arc::new(PerRoomSubstrates::new());
        let builder = StateBuilder::standalone();
        let roster = names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                crate::ipc::positron_source::test_roster_slot(
                    Uuid::from_u128(i as u128 + 1),
                    name,
                    SenderKind::Agent,
                )
            })
            .collect();
        rooms
            .for_room(Uuid::from_u128(room))
            .store(builder.session(RosterViewState {
                room_id: Uuid::from_u128(room),
                roster,
            }));
        rooms
    }

    fn roster_registry(names: &[&str]) -> Arc<PerRoomSubstrates> {
        roster_registry_for(ROOM, names)
    }

    /// A turn context stamped with the SAME room the fixtures build, so the
    /// shared room gate allows delivery (an unstamped ctx would pass too, but
    /// stamping is what a live turn does).
    fn ctx() -> RagContext {
        RagContext::for_persona_in_room(Uuid::from_u128(0x9), 0, Uuid::from_u128(ROOM))
    }

    // what this catches: the mind's roster line carries NO liveness flag — `[ready]`/
    // `[busy]` flips per heartbeat inside the STABLE system prefix and forfeited the
    // whole prefix (hit_rate 0.0 on 57/57 turns, prompts diverging at the flag,
    // 2026-09-04). Name, kind and role stay; presence is the live view's job.
    #[test]
    fn the_roster_line_carries_no_liveness_flag() {
        let mut slot = crate::ipc::positron_source::test_roster_slot(
            Uuid::from_u128(7),
            "Anwen",
            SenderKind::Agent,
        );
        slot.availability = Some("busy".to_string());
        let view = RosterViewState { room_id: Uuid::from_u128(0xaa), roster: vec![slot] };
        let units = RagRenderable::units(&view, Uuid::from_u128(7));
        assert_eq!(units.len(), 1);
        assert!(units[0].starts_with("Anwen"), "{units:?}");
        assert!(!units[0].contains("[busy]") && !units[0].contains('['), "{units:?}");
    }

    /// what this catches (card bb387821): the citizen's OWN row rendering
    /// indistinguishably from her peers'. Measured on IntelMac in BOTH directions with
    /// correct prompts — Paige, told "You are Paige" five times plus an explicit
    /// NOT-list, answered "I am Saoirse"; Saoirse answered "My name is Paige". The
    /// structured list beat the prose. Asserts the marker lands on exactly the reader's
    /// row and on no other, and that she is still LISTED (removing her would undercount
    /// who is in the room).
    #[test]
    fn a_citizens_own_row_is_marked_and_only_hers() {
        let me = Uuid::from_u128(2);
        let view = RosterViewState {
            room_id: Uuid::from_u128(0xaa),
            roster: vec![
                crate::ipc::positron_source::test_roster_slot(
                    Uuid::from_u128(1),
                    "Paige",
                    SenderKind::Agent,
                ),
                crate::ipc::positron_source::test_roster_slot(me, "Saoirse", SenderKind::Agent),
            ],
        };
        let units = RagRenderable::units(&view, me);
        assert_eq!(units.len(), 2, "she stays listed: {units:?}");
        let mine: Vec<&String> = units.iter().filter(|u| u.contains("YOU")).collect();
        assert_eq!(mine.len(), 1, "exactly one row marked: {units:?}");
        assert!(mine[0].starts_with("Saoirse"), "the READER's row: {units:?}");
        assert!(
            units.iter().any(|u| u.starts_with("Paige") && !u.contains("YOU")),
            "the peer stays unmarked: {units:?}"
        );
    }

    /// what this catches: marking by position or by accident rather than by identity.
    /// A viewer who is not in this roster at all — a human reading a room she has not
    /// joined, or a stale persona_id — must see NO marker, not the first row marked.
    #[test]
    fn a_viewer_absent_from_the_roster_marks_nobody() {
        let view = RosterViewState {
            room_id: Uuid::from_u128(0xaa),
            roster: vec![
                crate::ipc::positron_source::test_roster_slot(
                    Uuid::from_u128(1),
                    "Paige",
                    SenderKind::Agent,
                ),
                crate::ipc::positron_source::test_roster_slot(
                    Uuid::from_u128(2),
                    "Saoirse",
                    SenderKind::Agent,
                ),
            ],
        };
        let units = RagRenderable::units(&view, Uuid::from_u128(0xdead));
        assert_eq!(units.len(), 2);
        assert!(!units.iter().any(|u| u.contains("YOU")), "{units:?}");
    }

    /// what this catches: THE defect. A peer present in the room must reach the
    /// citizen's grounding — measured live at ZERO occurrences while the browser
    /// rendered the same peer fine, because the two read different code. If this
    /// regresses, citizens go blind to each other again and nothing else fails.
    #[tokio::test]
    async fn a_present_peer_reaches_the_citizens_grounding() {
        let source: ViewStateRagSource<RosterViewState> =
            ViewStateRagSource::per_room(roster_registry(&["Anwen", "Asha"]));
        let delivery = source.deliver(&ctx(), 500, ResolutionPreference::Raw).await;
        let rendered = delivery
            .items
            .iter()
            .map(|i| i.content.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("Anwen"), "peer missing: {rendered}");
        assert!(rendered.contains("Asha"), "peer missing: {rendered}");
        assert!(delivery.tokens_used <= 500);
    }

    /// what this catches: the all-or-nothing failure `floor_tokens` exists to
    /// prevent. Under a budget that fits one member, the citizen must learn about
    /// ONE person — not receive an empty block, and not overrun the window.
    #[tokio::test]
    async fn a_tight_budget_yields_fewer_units_never_a_chopped_one() {
        let source: ViewStateRagSource<RosterViewState> =
            ViewStateRagSource::per_room(roster_registry(&["Anwen", "Asha", "Anon"]));
        // 8 tokens, against ~4-token member lines: fits two, not three. (12 was
        // the first guess and it fit ALL THREE exactly — the packing was right and
        // the test's arithmetic was wrong, which is worth saying out loud because
        // "make the failing assert pass" is how a real invariant gets weakened.)
        let delivery = source.deliver(&ctx(), 8, ResolutionPreference::Raw).await;
        assert!(!delivery.items.is_empty(), "a fitting unit must be delivered");
        assert!(delivery.items.len() < 3, "budget 8 cannot fit all three");
        assert!(
            delivery.tokens_used <= 8,
            "the allocator's contract is tokens_used <= budget, got {}",
            delivery.tokens_used
        );
        assert!(
            delivery.continuation.is_some(),
            "undelivered members must be resumable, not silently dropped"
        );
    }

    /// what this catches: an unconditional cursor. If a fully-delivered source
    /// still handed back a cursor, the allocator would spend a later turn's budget
    /// resuming a source with nothing left to say.
    #[tokio::test]
    async fn a_complete_delivery_offers_no_continuation() {
        let source: ViewStateRagSource<RosterViewState> =
            ViewStateRagSource::per_room(roster_registry(&["Anwen"]));
        let delivery = source.deliver(&ctx(), 500, ResolutionPreference::Raw).await;
        assert_eq!(delivery.items.len(), 1);
        assert!(delivery.continuation.is_none());
    }

    /// what this catches (regression for #3862): a room-scoped source that binds ONE
    /// room and answers with it forever. The roster was constructed as
    /// `for_room(identity.default_room)`, so a citizen working anywhere else got her
    /// default room's view, the shared room gate correctly refused it, and she stood
    /// in a room full of people seeing nobody — 662 abstains in 873 ticks, bound
    /// #general, every turn in #continuum.
    ///
    /// The registry here holds BOTH rooms, which is what production looks like:
    /// `positron_source::sink` writes every room's view unconditionally. So this
    /// fails on the old binding not because the other room's data is missing, but
    /// because the source never asked for it.
    #[tokio::test]
    async fn the_roster_follows_the_turn_into_a_room_that_is_not_the_first_one() {
        const OTHER: u128 = 0xbb;
        let rooms = roster_registry(&["Anwen"]);
        // The SECOND room's own roster, stored beside the first — different people.
        let builder = StateBuilder::standalone();
        rooms
            .for_room(Uuid::from_u128(OTHER))
            .store(builder.session(RosterViewState {
                room_id: Uuid::from_u128(OTHER),
                roster: vec![crate::ipc::positron_source::test_roster_slot(
                    Uuid::from_u128(42),
                    "Saoirse",
                    SenderKind::Agent,
                )],
            }));

        let source: ViewStateRagSource<RosterViewState> = ViewStateRagSource::per_room(rooms);
        let in_other = RagContext::for_persona_in_room(
            Uuid::from_u128(0x9),
            0,
            Uuid::from_u128(OTHER),
        );
        let delivery = source.deliver(&in_other, 500, ResolutionPreference::Raw).await;
        let rendered = delivery
            .items
            .iter()
            .map(|i| i.content.clone())
            .collect::<Vec<_>>()
            .join("
");
        assert!(
            rendered.contains("Saoirse"),
            "a turn in a room must be grounded in THAT room's people: {rendered}"
        );
        assert!(
            !rendered.contains("Anwen"),
            "and never in another room's: {rendered}"
        );
    }

    /// what this catches (review of #3879): an INSERTING read. `for_room` creates
    /// the entry it is asked about; this source now looks up a room on EVERY
    /// delivery, so using it here would let a citizen wandering rooms accrete a
    /// permanently-empty substrate per distinct room — invisible until a node has
    /// been up a week, and it would inflate `room_count`, an ops read, with rooms
    /// nothing ever projected. Asserts the read path leaves the map's size alone.
    #[tokio::test]
    async fn reading_a_room_that_was_never_projected_does_not_create_one() {
        let rooms = roster_registry(&["Anwen"]);
        assert_eq!(rooms.room_count(), 1, "fixture projects exactly one room");
        let source: ViewStateRagSource<RosterViewState> =
            ViewStateRagSource::per_room(Arc::clone(&rooms));
        for n in 0..5u128 {
            let elsewhere = RagContext::for_persona_in_room(
                Uuid::from_u128(0x9),
                0,
                Uuid::from_u128(0xd0 + n),
            );
            let delivery = source.deliver(&elsewhere, 500, ResolutionPreference::Raw).await;
            assert!(delivery.items.is_empty(), "an unprojected room has no view");
        }
        assert_eq!(
            rooms.room_count(),
            1,
            "five deliveries into unprojected rooms must not create five substrates"
        );
    }

    /// what this catches: the honest absence on a roomless turn. Background
    /// consolidation and idle ticks carry no room, and a room-scoped source has no
    /// room whose people it could report. It must render nothing rather than reach
    /// for whatever room happens to be in the registry — the fallback that would
    /// silently reintroduce exactly the cross-room bleed above.
    #[tokio::test]
    async fn a_roomless_turn_gets_no_roster_rather_than_an_arbitrary_rooms() {
        let source: ViewStateRagSource<RosterViewState> =
            ViewStateRagSource::per_room(roster_registry(&["Anwen"]));
        let roomless = RagContext::for_persona(Uuid::from_u128(0x9), 0);
        let delivery = source.deliver(&roomless, 500, ResolutionPreference::Raw).await;
        assert!(delivery.items.is_empty(), "{:?}", delivery.items);
        assert_eq!(delivery.tokens_used, 0);
        assert!(delivery.continuation.is_none());
    }

    /// what this catches: a fabricated block. An empty room must render NOTHING —
    /// a header with no members under it reads as "presence unknown" and is the
    /// same lie as the framing that promised presence and delivered none.
    #[tokio::test]
    async fn an_empty_view_renders_no_block_rather_than_an_empty_header() {
        let source: ViewStateRagSource<RosterViewState> =
            ViewStateRagSource::per_room(Arc::new(PerRoomSubstrates::new()));
        let delivery = source.deliver(&ctx(), 500, ResolutionPreference::Raw).await;
        assert!(delivery.items.is_empty());
        assert_eq!(delivery.tokens_used, 0);
        assert!(delivery.continuation.is_none());
    }

    /// what this catches (THE OUTLIER TEST): one adapter serving a maximally
    /// different kind. The bench board has no identity, no presence, and numeric
    /// per-row verdicts. If this needed ANY change to the adapter, the abstraction
    /// was wrong and every later kind would need one too.
    #[tokio::test]
    async fn the_same_adapter_renders_a_benchmark_board_without_forcing() {
        let substrate = Substrate::new();
        let builder = StateBuilder::standalone();
        substrate.store(builder.session(BenchViewState {
            sample_interval_ms: 5000,
            rounds: vec![],
            runs: vec![
                BenchRunRow {
                round_id: None,
                solve_room: None,
                solve_room_name: None,
                    run_id: "r1".into(),
                    instance: Some("sympy__sympy-21055".into()),
                    solver: Some("Asha".into()),
                    phase: "active".into(),
                    stalled: false,
                    attempt: Some(2),
                    max_attempts: Some(3),
                    age_secs: 42,
                    acts: Some(10),
                    patch_bytes: Some(1295),
                    resolved: Some(false),
                    fail_to_pass: Some("0/1".into()),
                    pass_to_pass: Some("13/13".into()),
                    failed_tests: vec![],
                    infra_error: None,
                },
                BenchRunRow {
                round_id: None,
                solve_room: None,
                solve_room_name: None,
                    run_id: "r2".into(),
                    instance: Some("requests__requests-863".into()),
                    solver: Some("Atlas".into()),
                    phase: "ungradeable".into(),
                    stalled: false,
                    attempt: Some(1),
                    max_attempts: Some(3),
                    age_secs: 90,
                    acts: Some(4),
                    patch_bytes: Some(402),
                    resolved: None,
                    fail_to_pass: None,
                    pass_to_pass: None,
                    failed_tests: vec![],
                    infra_error: Some("era pytest cannot run".into()),
                },
            ],
        }));

        let source: ViewStateRagSource<BenchViewState> = ViewStateRagSource::new(substrate);
        let delivery = source.deliver(&ctx(), 500, ResolutionPreference::Raw).await;
        let rendered = delivery
            .items
            .iter()
            .map(|i| i.content.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("sympy__sympy-21055"), "{rendered}");
        assert!(rendered.contains("Asha"), "{rendered}");
        // An infra failure must read as ABSENCE, never as a capability zero.
        assert!(rendered.contains("UNGRADEABLE"), "{rendered}");
        assert!(
            !rendered.contains("r2 · unresolved"),
            "an ungradeable run must not also read as an unresolved attempt: {rendered}"
        );
    }
}
