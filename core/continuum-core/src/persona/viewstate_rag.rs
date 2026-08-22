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
//! ## ⚠️ BEFORE YOU SWAP A LIVE SOURCE ONTO THIS — read this first
//!
//! The obvious next move is to replace `room_roster_source` at its binding site
//! (`persona/supervisor.rs`, the sanctioned "RAG sources … bound on the brain at
//! boot" seam) with `ViewStateRagSource<RosterViewState>`. **Do not, yet.**
//!
//! The `Substrate` cache is keyed by **kind alone**. `continuum_positron`'s
//! `revisions.rs` names the fix as a future extension: *"multiple live instances
//! (per-room widgets), the key extends from the bare kind string to a
//! `(room_id, kind)` tuple."* Until that lands, the node substrate holds exactly
//! ONE room's roster — the focused room's.
//!
//! So a swap today would trade a source that is sometimes wrong for one that is
//! reliably EMPTY: this adapter's room gate correctly abstains for any persona
//! whose turn is in a different room than the focused one, and personas are
//! first-class MULTI-room subscribers ([[personas-are-first-class-multi-room-subscribers]]).
//! Most citizens would go blind rather than mis-sighted. That is not a repair.
//!
//! **The prerequisite is per-room instancing (`(room_id, kind)`).** With it, this
//! adapter serves every room correctly and the bespoke sources retire. Without it,
//! this module is a proven seam waiting on its substrate.
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
    fn units(&self) -> Vec<String>;

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
    /// The same substrate instance the WS server serves — a shared snapshot cache,
    /// so reading it is cheap and cannot diverge from what a browser sees.
    substrate: Substrate,
    _kind: std::marker::PhantomData<V>,
}

impl<V: RagRenderable> ViewStateRagSource<V> {
    pub fn new(substrate: Substrate) -> Self {
        Self {
            substrate,
            _kind: std::marker::PhantomData,
        }
    }

    /// Read + deserialize the current view, or `None` when the kind has never been
    /// stored (a cold boot before the first projection) or the payload does not
    /// match this build's shape. Both are honest absences: no block is rendered.
    fn current(&self) -> Option<V> {
        let envelope = self.substrate.cache().get(V::KIND)?;
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
        let view = self.current();
        // The ONE room gate, not a second copy of it: a room-scoped kind whose
        // view describes a DIFFERENT room than this turn abstains, with the same
        // probe every other room-scoped source emits.
        let units = match view {
            Some(v) if room_scope_allows(v.room(), _ctx, V::KIND) => v.units(),
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
        let view = self.current()?;
        if !room_scope_allows(view.room(), _ctx, V::KIND) {
            return None;
        }
        let units = view.units();
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

    fn units(&self) -> Vec<String> {
        self.roster
            .iter()
            .map(|slot| {
                // Kind and role are what make a name actionable ("who can I ask?"),
                // so they ride the SAME unit as the name rather than a second block
                // a tight budget would sever from it.
                let mut line = format!("{} ({:?})", slot.display_name, slot.kind);
                if let Some(role) = slot.role_label.as_deref().filter(|r| !r.is_empty()) {
                    line.push_str(&format!(" — {role}"));
                }
                if let Some(avail) = slot.availability.as_deref() {
                    line.push_str(&format!(" [{avail}]"));
                }
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

    fn units(&self) -> Vec<String> {
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

    fn roster_substrate(names: &[&str]) -> Substrate {
        let substrate = Substrate::new();
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
        substrate.store(builder.session(RosterViewState {
            room_id: Uuid::from_u128(0xaa),
            roster,
        }));
        substrate
    }

    /// A turn context stamped with the SAME room the fixtures build, so the
    /// shared room gate allows delivery (an unstamped ctx would pass too, but
    /// stamping is what a live turn does).
    fn ctx() -> RagContext {
        RagContext::for_persona_in_room(Uuid::from_u128(0x9), 0, Uuid::from_u128(0xaa))
    }

    /// what this catches: THE defect. A peer present in the room must reach the
    /// citizen's grounding — measured live at ZERO occurrences while the browser
    /// rendered the same peer fine, because the two read different code. If this
    /// regresses, citizens go blind to each other again and nothing else fails.
    #[tokio::test]
    async fn a_present_peer_reaches_the_citizens_grounding() {
        let source: ViewStateRagSource<RosterViewState> =
            ViewStateRagSource::new(roster_substrate(&["Anwen", "Asha"]));
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
            ViewStateRagSource::new(roster_substrate(&["Anwen", "Asha", "Anon"]));
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
            ViewStateRagSource::new(roster_substrate(&["Anwen"]));
        let delivery = source.deliver(&ctx(), 500, ResolutionPreference::Raw).await;
        assert_eq!(delivery.items.len(), 1);
        assert!(delivery.continuation.is_none());
    }

    /// what this catches: a fabricated block. An empty room must render NOTHING —
    /// a header with no members under it reads as "presence unknown" and is the
    /// same lie as the framing that promised presence and delivered none.
    #[tokio::test]
    async fn an_empty_view_renders_no_block_rather_than_an_empty_header() {
        let source: ViewStateRagSource<RosterViewState> =
            ViewStateRagSource::new(Substrate::new());
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
