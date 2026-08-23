//! RagSourceFaculty — lifts ANY [`RagSource`] into a perception-tier [`Faculty`].
//!
//! ## Why this bridge exists (the elegance call)
//!
//! Before the Workspace brain, all grounding flowed through ONE pipeline: the
//! [`RagSource`] trait (`deliver → RagDelivery`, budget-aware, paginated) — the
//! roster (#1650), the room doctrine (#1651), the airc transcript, the engram
//! store. The Workspace introduced a SECOND pipeline: the [`Faculty`] trait
//! (`contribute → Contribution`, salience-scored, staged into attention). The
//! gating cutover routes the participation [`Decision`] through the Workspace —
//! so any grounding that is a `RagSource` but NOT a `Faculty` silently falls out
//! of the live decision path. Roster + doctrine grounding, just landed, would go
//! dark the moment the switch flips.
//!
//! Re-implementing each source as a bespoke faculty would be the slop move: a
//! second place that reads airc presence, a second place that reads the doctrine
//! — drift waiting to happen (the compression principle forbids it). Instead this
//! is ONE adapter that lifts the WHOLE `RagSource` ecosystem into the Workspace:
//! every reviewed source becomes a faculty for free, with no second source of
//! truth. It is also the concrete first step of "broadcast == RAG context" (kill
//! parallel allocators) — the two pipelines meet here at the seam.
//!
//! ## Migration tool, not permanent architecture
//!
//! The grid converged (M5/IntelMac/BigMama, 2026-06-17) on: `RagSource` answers
//! **how to fetch grounding** (IO, budget, pagination, provenance); `Faculty`
//! answers **how grounding competes for attention** (salience, phase, the bounded
//! workspace). They are distinct concerns and coexist today — but the *end state*
//! is full convergence: every source becomes a native `Faculty` whose
//! `contribute()` returns its own salience, and this bridge is the **migration
//! scaffolding** that ships grounding into the Workspace NOW (unblocking the
//! gating cutover) without a big-bang rewrite. Deletion of `RagSource` is GATED
//! on (a) every source honoring its salience floor as a native faculty and (b)
//! `WorkspaceCaptureSink`/replay reaching parity with the mature
//! `RagCaptureSink` + `Recording/ReplayRagSource` observability — never before.
//! Recall is already converged: `RecallFaculty` is a *native* faculty (not a
//! bridged `EngramSource`) because it closes the bidirectional rehearsal loop the
//! one-way `RagSource` path never did.
//!
//! ## Salience POLICY — standing-framing vs retrieved (the load-bearing guard)
//!
//! A flat lift is a latent bug (BigMama): roster + doctrine are **standing
//! framing** — always-present structural context, like the system prompt — and
//! must NOT lose a budget fight to a high-cosine memory, or the persona forgets
//! the room's own rules mid-turn. Engram + conversation are **retrieved** — they
//! SHOULD compete on relevance. So the bridge carries a [`SaliencePolicy`]:
//! standing-framing bids at a high floor the top-k arbiter never truncates;
//! retrieved bids moderate and competes. The classification lives at the ASSEMBLY
//! layer (who builds the cycle), NOT in `RagSource` — the source stays
//! salience-free, no new coupling. In the converged end-state this policy is just
//! what each native faculty's `contribute()` returns.

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use super::workspace::{Contribution, Faculty, FacultyId, Workspace};
use crate::persona::rag_budget::{RagContext, RagSource, ResolutionPreference};

/// Per-source grounding ceiling as a FRACTION of the LIVE served window — the
/// per-source budget must SCALE with the window, never a baked constant (task
/// #124, [[no-hardcoded-context-numbers-derive-from-the-live-window]]). A source
/// fills up to this ceiling then self-truncates; the window-sized prompt packer
/// downstream is the real bound, and standing framing carries a high salience floor
/// so it is never the first thing dropped. So the ceiling should be GENEROUS: let
/// the workspace map show its full layout, the work board show every card, the wall
/// show the whole plan — "be more verbose as the budget allows" (Joel 2026-07-13).
///
/// The old fixed 4096 was the exact anti-pattern: it forced the board + map + roster
/// + doctrine + wall to each squeeze into ~4k tokens whether the served window was
/// 16k or 128k — starving a big model of the very grounding it could hold, and
/// (worse) over-spending on a tiny 2k window. Sizing at `window / 4` gives ~4096 at
/// the common 16k served window (preserving the tuned value that let each source
/// breathe), grows so a 128k model holds its full board/map/roster, and shrinks
/// honestly on a tight window — the packer still keeps the TOTAL ≤ window.
// context-budget-exempt: a DENOMINATOR — this is already the window-relative pattern this test exists to enforce
const GROUNDING_WINDOW_FRACTION: u32 = 4;

/// The per-source grounding ceiling for a given LIVE served window. Floored at the
/// substrate serving floor's share ([`MIN_SERVE_CTX`]/[`GROUNDING_WINDOW_FRACTION`])
/// so a faculty built without a window (tests) still gets a sane ceiling derived
/// from a substrate constant, never a fresh magic number.
pub fn grounding_budget_for(served_window: u32) -> u32 {
    use crate::cognition::serving_plan::MIN_SERVE_CTX;
    (served_window / GROUNDING_WINDOW_FRACTION).max(MIN_SERVE_CTX / GROUNDING_WINDOW_FRACTION)
}

/// Salience floor for **standing framing** (roster, doctrine) — always-present
/// structural context, like the system prompt. High enough that the top-k arbiter
/// never truncates it under attention pressure: a persona must not forget the
/// room's own rules because a high-cosine memory out-bid the doctrine this tick.
/// NOT a caste/`@`-gate — it is "this framing always applies."
const STANDING_FRAMING_SALIENCE: f32 = 0.9;

/// Salience for **retrieved** grounding (engram, conversation) — turn-specific,
/// SHOULD compete on relevance. Moderate so a strongly-relevant hit can rise and
/// a weak one can be crowded out. (Native retrieval faculties like `RecallFaculty`
/// already self-score by relevance; this is the bridge's bootstrap for retrieved
/// sources that don't yet.)
const RETRIEVED_SALIENCE: f32 = 0.5;

/// How a bridged source's grounding competes for attention. Classified at the
/// ASSEMBLY layer (whoever builds the cycle), never inside `RagSource` — the
/// source stays salience-free (BigMama's separation-of-concerns). In the
/// converged end-state this becomes what each native faculty's `contribute()`
/// returns; here it is the bridge's bootstrap.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SaliencePolicy {
    /// Always-present structural context (roster, doctrine) — bids at the
    /// [`STANDING_FRAMING_SALIENCE`] floor so attention pressure can't evict it.
    StandingFraming,
    /// Turn-specific grounding (engram, conversation) — bids at
    /// [`RETRIEVED_SALIENCE`] and competes on relevance.
    Retrieved,
    /// Explicit fixed salience (tests / tuning / a learned signal).
    Fixed(f32),
}

impl SaliencePolicy {
    /// The salience a source under this policy bids.
    fn salience(self) -> f32 {
        match self {
            SaliencePolicy::StandingFraming => STANDING_FRAMING_SALIENCE,
            SaliencePolicy::Retrieved => RETRIEVED_SALIENCE,
            SaliencePolicy::Fixed(s) => s.clamp(0.0, 1.0),
        }
    }
}

/// Wall-clock seam — injectable so tests are deterministic. ms since unix epoch,
/// matching the `now_ms()` convention across cognition (and `RecallFaculty`).
pub type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

fn wall_clock() -> Clock {
    Arc::new(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    })
}

/// Adapts an [`Arc<dyn RagSource>`] to the [`Faculty`] trait. Perception tier
/// (`reacts_to_broadcast() == false`): it bids the source's delivery as context
/// in phase 1, so the deliberation faculty conditions on it in phase 2. The
/// faculty's identity IS the source's identity (`FacultyId::Custom(source_id)`) —
/// one name, one source of truth, traceable straight back to the source in every
/// workspace trace.
pub struct RagSourceFaculty {
    persona_id: Uuid,
    source: Arc<dyn RagSource>,
    faculty_id: FacultyId,
    salience: f32,
    /// `true` for [`SaliencePolicy::StandingFraming`] sources (roster, doctrine,
    /// map) — propagated onto the [`Contribution`] so the deliberation serializer
    /// hoists this grounding into the cacheable KV-prefix region (standing framing
    /// is "like the system prompt"; it belongs adjacent to it). Volatile retrieved
    /// sources stay `false` and serialize last, nearest the generation point.
    stable: bool,
    budget: u32,
    clock: Clock,
}

impl RagSourceFaculty {
    /// Wrap `source` as a perception faculty for `persona_id` under a
    /// [`SaliencePolicy`] (standing-framing vs retrieved). The `FacultyId` is
    /// derived from `source.source_id()` so the faculty and the source can never
    /// disagree about their identity.
    pub fn new(persona_id: Uuid, source: Arc<dyn RagSource>, policy: SaliencePolicy) -> Self {
        let faculty_id = FacultyId::Custom(source.source_id().to_string());
        Self {
            persona_id,
            source,
            faculty_id,
            salience: policy.salience(),
            // Standing framing is session-stable by default; retrieved grounding
            // is volatile. `with_volatile_content` overrides for framing whose
            // BYTES mutate per turn (active-work, room-board — convicted by
            // debug/prompt-reuse 2026-08-22): importance keeps the floor,
            // placement follows content stability.
            stable: matches!(policy, SaliencePolicy::StandingFraming),
            // Floor default (derived from the substrate serving floor, not a magic
            // number). Production overrides via `with_budget(grounding_budget_for(
            // cfg.context_window))` so the ceiling tracks the LIVE served window.
            budget: grounding_budget_for(crate::cognition::serving_plan::MIN_SERVE_CTX),
            clock: wall_clock(),
        }
    }

    /// Override the resolved salience directly (e.g. a learned signal) — escape
    /// hatch past the policy default.
    pub fn with_salience(mut self, salience: f32) -> Self {
        self.salience = salience.clamp(0.0, 1.0);
        self
    }

    /// Override the per-tick token budget handed to the source.
    pub fn with_budget(mut self, budget: u32) -> Self {
        self.budget = budget;
        self
    }

    /// Framing whose content mutates per turn: demote OUT of the stable tier
    /// while keeping the StandingFraming salience floor (see `stable` field doc).
    pub fn with_volatile_content(mut self, volatile: bool) -> Self {
        if volatile {
            self.stable = false;
        }
        self
    }

    /// Inject a deterministic clock (tests / replay).
    pub fn with_clock(mut self, clock: Clock) -> Self {
        self.clock = clock;
        self
    }
}

#[async_trait]
impl Faculty for RagSourceFaculty {
    fn id(&self) -> FacultyId {
        self.faculty_id.clone()
    }

    // Perception tier (default): grounding bids in phase 1 over the raw
    // world-state, so the deliberation faculty reads it from the broadcast in
    // phase 2. The grounding sources (roster, doctrine) read airc/substrate
    // state, not the burst, so the bridge does not pass the world_state as a
    // query; a future query-conditioned source would extend RagContext, not this
    // seam.
    async fn contribute(&self, ws: &Workspace) -> Option<Contribution> {
        let now = (self.clock)();
        // Thread the turn's CONTEXT (the WHERE axis — `Workspace::room_id`, the
        // tick's contextId) into the delivery context, so room-scoped sources
        // ground THE TURN'S room, never wherever they happened to be bound at
        // build time. This is what keeps room A's kanban/roster/doctrine out of
        // a turn in room B — and out of a synthetic context like the eval fork's
        // nil room (the exam-bleed bug: stale board imperatives injected into a
        // coding exam derailed agentically-trained models; glass-boxed live,
        // Hermes-8B OURS 38% < RAW 52%). A nil room is deliberately threaded as
        // Some(nil): it IS a context — one that is no room — so every room-bound
        // source honestly mismatches and abstains. [[identity-context-session-three-axes]]
        let ctx = RagContext::for_persona_in_room(self.persona_id, now, ws.room_id);
        let delivery = self
            .source
            .deliver(&ctx, self.budget, ResolutionPreference::Raw)
            .await;

        // Empty delivery → abstain (the source had nothing, or degraded to empty
        // per the good-citizen doctrine). No empty bid clutters the workspace.
        if delivery.items.is_empty() {
            return None;
        }

        // One context block per source: concatenate the delivered atomic units.
        // The deliberation faculty renders this under a `[<source_id>]` header.
        //
        // The join is the RENDERING, not the loss of structure — the units ride
        // along on the contribution as `parts` (below), so a block too large for
        // the prompt budget can still contribute its leading units instead of
        // vanishing whole. Flattening and DISCARDING the list is what made the
        // work board structurally invisible: measured 2026-08-06, `room-kanban`
        // was kept 0 / dropped 495 times, a median 5,364-token all-or-nothing
        // offer against a median 55-token budget, while its first two units
        // (~200 tokens) carried every fact a citizen needed to find work.
        let units: Vec<String> = delivery.items.iter().map(|i| i.content.clone()).collect();
        let content = units.join("\n");
        let reasoning = format!(
            "grounding from '{}' — {} item(s), {} tokens",
            self.source.source_id(),
            delivery.items.len(),
            delivery.tokens_used
        );

        let c = Contribution::context(self.faculty_id.clone(), content, self.salience, reasoning)
            .with_parts(units)
            .with_expand_command(self.source.expand_command());
        // Volatile-content grounding rides the TRAILING-turn mechanism (#205),
        // never the system message. The volatile tier of the system context
        // block was a half-measure: demoted out of the cacheable stable head,
        // but still rendered BEFORE the entire conversation — so a kanban
        // claim-flap or a workspace-map change (her own write!) re-prefilled
        // every conversation token after it. Measured 2026-08-23 from her own
        // captures on the MirrorCode round: workspace-map content changed
        // act-over-act and live KV reuse pinned at 18-33% while acts paid
        // ~35-45k re-prefill (~2 min) each. As a trailing turn the same churn
        // costs exactly its own tokens.
        Some(if self.stable {
            c.session_stable()
        } else {
            c.trailing()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::rag_budget::{
        ContinuationCursor, RagDelivery, RagItem, ResolutionPreference,
    };
    use std::sync::Mutex;

    fn persona() -> Uuid {
        Uuid::parse_str("00000000-0000-0000-0000-000000000aaa").unwrap()
    }

    /// A stub RagSource that returns canned items and records the persona_id it
    /// was delivered for (to prove the bridge passes scope through).
    struct StubSource {
        id: &'static str,
        items: Vec<RagItem>,
        seen_persona: Mutex<Option<Uuid>>,
    }

    impl StubSource {
        fn new(id: &'static str, contents: &[&str]) -> Self {
            let items = contents
                .iter()
                .map(|c| RagItem {
                    content: c.to_string(),
                    tokens: ((c.len() / 4) as u32).saturating_add(1),
                    metadata: serde_json::json!({}),
                })
                .collect();
            Self {
                id,
                items,
                seen_persona: Mutex::new(None),
            }
        }
    }

    #[async_trait]
    impl RagSource for StubSource {
        fn source_id(&self) -> &'static str {
            self.id
        }

        fn expand_command(&self) -> Option<&'static str> {
            // Test/stub source — nothing further to fetch.
            None
        }

        /// Test/stub source — floorless, so it never encodes a production floor.
        fn floor_tokens(&self) -> u32 {
            0
        }
        async fn deliver(
            &self,
            ctx: &RagContext,
            _budget: u32,
            resolution: ResolutionPreference,
        ) -> RagDelivery {
            *self.seen_persona.lock().unwrap() = Some(ctx.persona_id);
            let tokens_used = self.items.iter().map(|i| i.tokens).sum();
            RagDelivery {
                source_id: self.id.to_string(),
                items: self.items.clone(),
                tokens_used,
                continuation: None,
                resolution_used: resolution,
            }
        }
        async fn deliver_continuation(
            &self,
            _ctx: &RagContext,
            _cursor: ContinuationCursor,
            _budget: u32,
        ) -> Option<RagDelivery> {
            None
        }
    }

    // what this catches: ANY RagSource is lifted into a perception-tier context
    // Contribution — the delivery's items become the bid content, the faculty id
    // is the source id, and it carries no Decision (grounding is context, never a
    // verdict). This is the regression fix: roster/doctrine reach the decider.
    #[tokio::test]
    async fn lifts_a_rag_source_into_a_context_bid() {
        let source = Arc::new(StubSource::new(
            "room-roster",
            &["Aria [persona]", "win-claude [claude] — Busy"],
        ));
        let faculty = RagSourceFaculty::new(persona(), source, SaliencePolicy::StandingFraming)
            .with_clock(Arc::new(|| 1_000));

        assert!(
            !faculty.reacts_to_broadcast(),
            "grounding is perception tier — it bids in phase 1, before deliberation"
        );
        assert_eq!(faculty.id(), FacultyId::Custom("room-roster".to_string()));

        let c = faculty
            .contribute(&Workspace::new("who's around?"))
            .await
            .expect("a non-empty source must bid");
        assert_eq!(c.faculty, FacultyId::Custom("room-roster".to_string()));
        assert!(
            c.decision.is_none(),
            "grounding is context, never a verdict"
        );
        assert!(c.content.contains("Aria [persona]"));
        assert!(c.content.contains("win-claude [claude] — Busy"));
        assert!(c.salience > 0.0);
    }

    // what this catches: the KV routing contract (2026-08-23). A session-stable
    // source's bid lands in the cacheable system prefix (stable, not trailing);
    // a volatile-content source's bid rides as a TRAILING conversation turn —
    // never the system message, where its churn (kanban claim-flaps, her own
    // writes mutating the workspace map) re-prefilled every conversation token
    // after it (live KV reuse pinned at 18-33% on the MirrorCode round).
    #[tokio::test]
    async fn volatile_content_grounding_is_trailing_and_stable_grounding_is_not() {
        let stable = RagSourceFaculty::new(
            persona(),
            Arc::new(StubSource::new("room-roster", &["Aria [persona]"])),
            SaliencePolicy::StandingFraming,
        )
        .with_clock(Arc::new(|| 1_000));
        let c = stable
            .contribute(&Workspace::new("hi"))
            .await
            .expect("non-empty source bids");
        assert!(c.stable, "standing framing stays in the cacheable prefix");
        assert!(!c.trailing, "stable grounding must not double as trailing");

        let volatile = RagSourceFaculty::new(
            persona(),
            Arc::new(StubSource::new("workspace-map", &["src/ lib/ tests/"])),
            SaliencePolicy::StandingFraming,
        )
        .with_volatile_content(true)
        .with_clock(Arc::new(|| 1_000));
        let c = volatile
            .contribute(&Workspace::new("hi"))
            .await
            .expect("non-empty source bids");
        assert!(!c.stable, "volatile content leaves the stable tier");
        assert!(
            c.trailing,
            "volatile grounding rides a trailing turn — churn costs its own tokens, \
             never the conversation's"
        );
    }

    // what this catches: an empty delivery → abstain (None), not an empty bid.
    // A degraded/absent source (good-citizen empty delivery) simply does not
    // clutter the workspace.
    #[tokio::test]
    async fn empty_delivery_abstains() {
        let source = Arc::new(StubSource::new("room-doctrine", &[]));
        let faculty = RagSourceFaculty::new(persona(), source, SaliencePolicy::StandingFraming);
        assert!(faculty
            .contribute(&Workspace::new("anything?"))
            .await
            .is_none());
    }

    // what this catches: the bridge passes the faculty's persona scope through to
    // the source (so the source's persona-scoping / defense-in-depth check sees
    // the right citizen).
    #[tokio::test]
    async fn passes_persona_scope_to_the_source() {
        let source = Arc::new(StubSource::new("room-doctrine", &["coordination room"]));
        let probe = source.clone();
        let faculty = RagSourceFaculty::new(persona(), source, SaliencePolicy::Retrieved);
        let _ = faculty.contribute(&Workspace::new("burst")).await;
        assert_eq!(
            *probe.seen_persona.lock().unwrap(),
            Some(persona()),
            "the source must be delivered for the faculty's persona"
        );
    }

    // what this catches: the whole point — bridged grounding reaches the
    // deliberation faculty through the staged cycle. A doctrine bid in phase 1 is
    // in the assembled broadcast the decider reads in phase 2.
    #[tokio::test]
    async fn bridged_grounding_reaches_the_broadcast() {
        use crate::cognition::workspace::{
            Contribution, Decision, SalienceArbiter, WorkspaceCycle,
        };

        // A deliberation faculty that proves it saw the bridged doctrine.
        struct SeesDoctrine;
        #[async_trait]
        impl crate::cognition::workspace::Faculty for SeesDoctrine {
            fn id(&self) -> FacultyId {
                FacultyId::Deliberation
            }
            fn reacts_to_broadcast(&self) -> bool {
                true
            }
            async fn contribute(&self, ws: &Workspace) -> Option<Contribution> {
                let doctrine = ws
                    .broadcast
                    .iter()
                    .find(|c| c.faculty == FacultyId::Custom("room-doctrine".to_string()));
                match doctrine {
                    Some(d) => Some(Contribution::verdict(
                        Decision::Speak {
                            text: format!("noted the room is: {}", d.content),
                        },
                        0.9,
                        "conditioned on the bridged doctrine grounding",
                    )),
                    None => Some(Contribution::verdict(
                        Decision::Pass,
                        0.4,
                        "no doctrine in the broadcast",
                    )),
                }
            }
        }

        let doctrine_source = Arc::new(StubSource::new(
            "room-doctrine",
            &["a coordination room — respond sparingly"],
        ));
        let faculties: Vec<Arc<dyn crate::cognition::workspace::Faculty>> = vec![
            Arc::new(
                RagSourceFaculty::new(persona(), doctrine_source, SaliencePolicy::StandingFraming)
                    .with_clock(Arc::new(|| 1)),
            ),
            Arc::new(SeesDoctrine),
        ];
        let ws = WorkspaceCycle::new(faculties, Arc::new(SalienceArbiter), 6)
            .run("is anyone going to merge this?")
            .await;

        match ws.decision() {
            Some(Decision::Speak { text }) => assert!(
                text.contains("respond sparingly"),
                "the decider must condition on the bridged doctrine grounding, got: {text}"
            ),
            other => panic!("expected a doctrine-grounded Speak, got {other:?}"),
        }
    }

    // what this catches: the salience POLICY — standing framing (roster/doctrine)
    // bids HIGHER than retrieved grounding, so the floor exists. This is the
    // load-bearing guard: it is what keeps a high-cosine memory from out-bidding
    // the room's own rules. Fixed escape-hatch is honored verbatim.
    #[tokio::test]
    async fn standing_framing_outbids_retrieved() {
        let framing = RagSourceFaculty::new(
            persona(),
            Arc::new(StubSource::new("room-doctrine", &["respond sparingly"])),
            SaliencePolicy::StandingFraming,
        )
        .with_clock(Arc::new(|| 1));
        let retrieved = RagSourceFaculty::new(
            persona(),
            Arc::new(StubSource::new("conversation", &["someone said hi"])),
            SaliencePolicy::Retrieved,
        )
        .with_clock(Arc::new(|| 1));

        let f = framing.contribute(&Workspace::new("q")).await.unwrap();
        let r = retrieved.contribute(&Workspace::new("q")).await.unwrap();
        assert!(
            f.salience > r.salience,
            "standing framing must out-bid retrieved (floor): framing={} retrieved={}",
            f.salience,
            r.salience
        );

        // The Fixed escape-hatch is passed through verbatim (after clamp).
        let fixed = RagSourceFaculty::new(
            persona(),
            Arc::new(StubSource::new("x", &["y"])),
            SaliencePolicy::Fixed(0.42),
        )
        .with_clock(Arc::new(|| 1));
        let c = fixed.contribute(&Workspace::new("q")).await.unwrap();
        assert!((c.salience - 0.42).abs() < 1e-6);
    }

    // what this catches: THE CANARY — standing framing keeps a non-zero presence
    // in the assembled context_broadcast even when a RETRIEVED bid out-saliences
    // it. At the default workspace capacity the floor is what guarantees the
    // persona never "forgets the room's rules mid-turn" because a relevant memory
    // crowded the doctrine out. (Hard-capacity-pressure exemption — making
    // standing framing truncation-proof regardless of capacity — is the
    // convergence follow-up; this asserts the realistic-capacity guarantee the
    // bridge ships with.)
    #[tokio::test]
    async fn standing_framing_present_even_when_outsalienced() {
        use crate::cognition::workspace::{
            Contribution, FacultyId as FId, SalienceArbiter, Workspace as Ws, WorkspaceCaptureSink,
            WorkspaceCycle, WorkspaceTrace,
        };

        // A retrieved-tier faculty that bids ABOVE the standing-framing floor
        // (a very relevant memory) — the adversarial case for the canary.
        struct HotRetrieval;
        #[async_trait]
        impl crate::cognition::workspace::Faculty for HotRetrieval {
            fn id(&self) -> FId {
                FId::Recall
            }
            async fn contribute(&self, _ws: &Ws) -> Option<Contribution> {
                Some(Contribution::context(
                    FId::Recall,
                    "a highly relevant recalled memory",
                    0.99,
                    "hot cosine hit",
                ))
            }
        }

        #[derive(Default)]
        struct Sink(std::sync::Mutex<Vec<WorkspaceTrace>>);
        impl WorkspaceCaptureSink for Sink {
            fn record(&self, t: &WorkspaceTrace) {
                self.0.lock().unwrap().push(t.clone());
            }
        }

        let doctrine = RagSourceFaculty::new(
            persona(),
            Arc::new(StubSource::new(
                "room-doctrine",
                &["coordination room — respond sparingly"],
            )),
            SaliencePolicy::StandingFraming,
        )
        .with_clock(Arc::new(|| 1));

        let sink = Arc::new(Sink::default());
        let faculties: Vec<Arc<dyn crate::cognition::workspace::Faculty>> =
            vec![Arc::new(doctrine), Arc::new(HotRetrieval)];
        let _ = WorkspaceCycle::new(
            faculties,
            Arc::new(SalienceArbiter),
            DEFAULT_CAPACITY_FOR_TEST,
        )
        .with_capture(sink.clone())
        .run("anything urgent?")
        .await;

        let traces = sink.0.lock().unwrap();
        let ctx = &traces[0].context_broadcast;
        assert!(
            ctx.iter()
                .any(|c| c.faculty == FId::Custom("room-doctrine".to_string())),
            "standing-framing doctrine must remain in context_broadcast even when a \
             retrieved bid out-saliences it — else the persona forgets the room's rules"
        );
    }

    /// The production default capacity (mirror of persona_workspace's constant) so
    /// the canary tests the realistic configuration, not a contrived one.
    const DEFAULT_CAPACITY_FOR_TEST: usize = 6;
}
