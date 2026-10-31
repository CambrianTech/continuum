use super::*;

/// Why a turn is happening — the thing it is a RESPONSE to, and the root of its
/// causal thread (CAUSAL-MEMORY-GRAPH.md §3a).
///
/// Joel's law, 2026-08-18: *any stimulus has a response — that is what comes into a
/// system, like any input to a channel — therefore needing causal linkage.* A mind
/// whose acts chain only to each other has instants, not experience; it can say "I ran
/// a command" but never "I ran a command BECAUSE she asked". Without a head on the
/// thread there is no path in the graph from a work card to the acts done for it, and
/// no query can show a link that was never recorded.
///
/// This is an ENUM rather than an `Option<Uuid>` on purpose. The optional form made
/// "nothing caused this" and "nobody wired this up" the same value, so a burst site
/// that simply forgot was indistinguishable from one being honest — and the compiler
/// helped it forget. Naming the reasons separately means an uncaused turn is a
/// measurable fact ([`Cause::Ambient`]) instead of a silence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    /// A discrete input to a channel — the message, kickoff or event this turn
    /// answers — already admitted as an engram. The thread has a head, so the first
    /// act carries a real `CausedBy` edge back to what provoked it.
    Stimulus(uuid::Uuid),

    /// A self-directed wake: her own cadence noticed the world had changed, with no
    /// single admitted input to point at. The idle tick perceives AMBIENT state (a
    /// re-read of the room, the board, her work) rather than an arrival, so there is
    /// nothing to root in and inventing one would be a lie.
    ///
    /// Honest, but not the end state: the ambient sources are projections that drop
    /// their items' identity, so an idle turn *cannot yet* name the change that woke
    /// it. Giving deliveries a handle back to their source item is what would let this
    /// become a `Stimulus` — see `docs/architecture/CONTENT-TRAVELS-BY-HANDLE.md`.
    Ambient,

    /// A burst assembled with no live antecedent at all: eval exams, replay fixtures,
    /// faculty tests. Distinct from [`Ambient`](Self::Ambient) so measurements of how
    /// many LIVE turns run uncaused are not diluted by fixtures.
    Synthetic,
}

impl Cause {
    /// The engram a chain rooted here begins from — `Some` only for a real stimulus.
    /// Ambient and synthetic bursts have no antecedent, and reporting one they do not
    /// have would fabricate an edge.
    pub fn root(&self) -> Option<uuid::Uuid> {
        match self {
            Cause::Stimulus(id) => Some(*id),
            Cause::Ambient | Cause::Synthetic => None,
        }
    }

    /// Short tag for probes and receipts — the vocabulary a measurement groups by.
    pub fn as_str(&self) -> &'static str {
        match self {
            Cause::Stimulus(_) => "stimulus",
            Cause::Ambient => "ambient",
            Cause::Synthetic => "synthetic",
        }
    }
}

impl Burst {
    /// Assemble an attributed burst: a `[room {room}]` header followed by each
    /// turn rendered to its historical line format. The header is rendered INTO
    /// `rendered` (so `world_state` is byte-identical to the old
    /// `build_workspace_burst`) but deliberately kept OUT of `turns` — room
    /// identity is standing context for the system prompt, not a conversation turn.
    /// **Not for a live turn.** This constructor states [`Cause::Synthetic`] on your
    /// behalf, which is true for an exam, a replay fixture or a faculty test and a lie
    /// for anything a citizen actually lived. A live burst site must call
    /// [`from_turns_at`](Self::from_turns_at) and say what caused it.
    pub fn from_turns(room: crate::identity::ActivityRoom, turns: Vec<BurstTurn>) -> Self {
        Self::from_turns_at(room, turns, None, Cause::Synthetic)
    }

    /// Like [`from_turns`](Self::from_turns) but stamps the persona's NOW into the
    /// header — the clock in her perception (task #125 prospective memory: a being
    /// who commits to "tomorrow at 2 PM" must know when now IS; glass-boxed live,
    /// her prompts carried no time referent at all, so appointments were words she
    /// could never act on). The clock is a PARAMETER, never an ambient global read:
    /// the live path passes wall-clock, the eval passes its pinned epoch (exams stay
    /// byte-reproducible), tests pass fixtures. Rendered at MINUTE granularity so
    /// the prompt prefix — and the serving KV cache — only changes once a minute.
    ///
    /// `cause` is REQUIRED, not a builder step, because it is the one field a live
    /// assembly site can silently forget. See [`Cause`].
    pub fn from_turns_at(
        room: crate::identity::ActivityRoom,
        turns: Vec<BurstTurn>,
        now_ms: Option<u64>,
        cause: Cause,
    ) -> Self {
        use std::fmt::Write as _;
        let mut rendered = String::new();
        let _ = writeln!(rendered, "[room {room}]");
        // NO [now …] line here anymore (2026-08-26 glass box): this header is the
        // FIRST bytes of the conversation, so the minute tick invalidated every
        // message behind it — measured live as `cached=7945` (the stable system
        // head) with the ENTIRE 10k-27k tail re-prefilled every act, ~647k fresh
        // tokens in one boot. Temporal grounding lives in the deliberation
        // prompt's VOLATILE tier (delivered as the newest turn, zero prefix
        // cost) fed from `now_ms`, which stays on the Burst for exactly that.
        for turn in &turns {
            turn.write_line(&mut rendered);
        }
        Self {
            room,
            turns,
            rendered,
            now_ms,
            cause,
            room_updates: Arc::default(),
            hands: None,
            effects: None,
        }
    }

    /// This turn acts with the hands of `context` (the card claim it works).
    pub fn in_hands(
        mut self,
        context: Option<crate::identity::hands_context::HandsContext>,
    ) -> Self {
        self.hands = context;
        self
    }

    /// This turn's pre-effect mark (stop-drain S2): set by the loop when the turn has a
    /// confirmed obligation, so each tool batch and her reply are recorded before they happen.
    pub fn with_effects(mut self, gate: Option<crate::cognition::act_observe::EffectGate>) -> Self {
        self.effects = gate;
        self
    }

    /// A raw-string burst IN a named activity — the production path for a
    /// stimulus that arrives as text (a gate check, a replay line) but still
    /// belongs to a real room. One opaque turn, rendered verbatim.
    pub fn raw_in(room: crate::identity::ActivityRoom, s: String) -> Self {
        Self {
            room,
            turns: vec![BurstTurn::opaque(s.clone())],
            rendered: s,
            now_ms: None,
            cause: Cause::Synthetic,
            room_updates: Arc::default(),
            hands: None,
            effects: None,
        }
    }
}

/// Raw-string → Burst conversions are TEST-ONLY: a production burst must name
/// its activity ([`Burst::raw_in`] / [`Burst::from_turns_at`]) — a string
/// cannot, so letting it convert was the door #425 walked through. Fixtures
/// get a deterministic [`ActivityRoom::test_room`](crate::identity::ActivityRoom::test_room).
#[cfg(any(test, feature = "test-fixtures"))]
impl From<String> for Burst {
    fn from(s: String) -> Self {
        Self {
            room: crate::identity::ActivityRoom::test_room(),
            turns: vec![BurstTurn::opaque(s.clone())],
            rendered: s,
            now_ms: None,
            // A raw string arrives with no channel and no antecedent — a fixture, by
            // construction. Honest, never invented.
            cause: Cause::Synthetic,
            room_updates: Arc::default(),
            hands: None,
            effects: None,
        }
    }
}

#[cfg(any(test, feature = "test-fixtures"))]
impl From<&str> for Burst {
    fn from(s: &str) -> Self {
        Burst::from(s.to_string())
    }
}

#[cfg(any(test, feature = "test-fixtures"))]
impl From<&String> for Burst {
    fn from(s: &String) -> Self {
        Burst::from(s.clone())
    }
}

/// Why a turn receives attention. Human participation retains foreground
/// scheduling without claiming the speaker literally addressed this persona.
/// Addressing comes from the raw message's identity-aware mention check (or an
/// explicitly targeted command/eval), never from the speaker's kind alone.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TurnAttention {
    #[default]
    Ambient,
    HumanOpportunity,
    Addressed,
    /// A priority room input became visible during an existing turn. This grants
    /// its next perception a foreground lane without changing what originally
    /// triggered the turn or asserting that original trigger named the persona.
    PriorityInput,
}

impl TurnAttention {
    pub fn with_input(self, input: Self) -> Self {
        if !self.requires_priority() && input.requires_priority() {
            Self::PriorityInput
        } else {
            self
        }
    }
    pub fn for_message(mentioned: bool, sender_is_human: bool) -> Self {
        if mentioned {
            Self::Addressed
        } else if sender_is_human {
            Self::HumanOpportunity
        } else {
            Self::Ambient
        }
    }

    /// A literal addressing fact, used only to frame the prompt.
    pub fn is_addressed(self) -> bool {
        matches!(self, Self::Addressed)
    }

    /// Admission, fresh perception and foreground lanes preserve the existing
    /// opportunity for a human to speak without having to name each citizen.
    pub fn requires_priority(self) -> bool {
        !matches!(self, Self::Ambient)
    }
}

/// How THIS turn is framed for the persona — the structural facts the system
/// prompt reflects: is it DIRECTED at her (suppress the silence escape) and is it
/// SELF-INITIATED (the heartbeat pursuing her own thread, vs a message/eval
/// driving it). Replaces the lone `directed: bool` that used to thread through
/// the run/settle seam, so the self-initiated framing lives in the system prompt
/// instead of being concatenated onto the burst text. Both facts are STRUCTURAL
/// (addressing / scheduling origin), never a read of her output
/// ([[no-hardcoded-heuristics-to-steer-cognition]]).
#[derive(Debug, Clone, Copy, Default)]
pub struct TurnFraming {
    /// Addressing and scheduling opportunity travel together without conflation.
    pub attention: TurnAttention,
    /// The never-stop heartbeat pursuing her own thread (no inbound message) —
    /// see [`Workspace::self_initiated`].
    pub self_initiated: bool,
    /// This turn's DELIVERABLE is a change in the workspace, not an utterance —
    /// declared by the caller (a SWE-style harness grades the diff; nobody reads
    /// the speech). Structural exactly like the two above: it describes the turn's
    /// contract, never a read of her output. `false` (the default) is every
    /// ordinary turn, where speech IS the deliverable.
    ///
    /// The ONE thing it changes: [`super::act_observe::drive_to_settle`] gives a
    /// zero-deliverable Speak one re-perception instead of settling on it. See
    /// that seam for why.
    pub workspace_deliverable: bool,
    /// An INITIATIVE turn: she holds nothing and her own heartbeat offers her a turn with
    /// her hands (`persona::initiative_turn`). It takes the act allowance a
    /// work turn takes, and NONE of its deliverable contract: nothing is due, so no
    /// workspace change is expected of it. See [`Workspace::initiative`].
    pub initiative: bool,
}

impl TurnFraming {
    /// Ordinary room chatter, message-driven — silence stays first-class.
    pub fn ambient() -> Self {
        Self::default()
    }

    /// A message put TO her (the message path's addressed turn, or an eval
    /// examiner question) — directed, not self-initiated.
    pub fn directed() -> Self {
        Self {
            attention: TurnAttention::Addressed,
            self_initiated: false,
            ..Self::default()
        }
    }

    /// The message path: a turn driven by an inbound message (not self-initiated).
    /// Only literal addressing withholds the silence affordance; an unmentioned
    /// human message keeps its scheduling opportunity and ambient prompt.
    pub fn message(attention: TurnAttention) -> Self {
        Self {
            attention,
            self_initiated: false,
            ..Self::default()
        }
    }

    /// The self-initiated heartbeat. Directed iff she was named in what she
    /// perceived (so a question reaching her on the digest path is not ghosted).
    pub fn self_thread(directed: bool) -> Self {
        Self {
            attention: TurnAttention::for_message(directed, false),
            self_initiated: true,
            ..Self::default()
        }
    }

    /// Her own heartbeat's turn with her hands when she holds nothing: self-initiated,
    /// never directed, and an initiative (see [`Self::initiative`]).
    pub fn initiative() -> Self {
        Self {
            initiative: true,
            ..Self::self_thread(false)
        }
    }

    /// Declare that this turn's deliverable is a WORKSPACE CHANGE, not an
    /// utterance — the SWE/agentic-harness contract, where the grader reads the
    /// diff and nobody reads the speech. Caller-declared and structural; see
    /// [`Self::workspace_deliverable`].
    pub fn on_workspace(mut self) -> Self {
        self.workspace_deliverable = true;
        self
    }
}

/// The bounded global workspace: the consolidated world-state being reasoned
/// over (channel/recipe-shaped — a text thread, a game space + player
/// positions, an AR scene, a code diff) plus what won attention and is
/// broadcast back to all faculties.
#[derive(Debug, Clone)]
pub struct Workspace {
    /// Additional room inputs; the active turn's room and task remain separate.
    pub room_updates: Arc<Vec<Arc<crate::cognition::conversation::IncomingMessage>>>,
    /// The consolidated burst / world-state at service time, as flat text — the
    /// rendered projection of [`turns`](Self::turns). Opaque to the core; the
    /// channel/recipe adapter shapes it. Every TEXT reader (recall, dashboards,
    /// focus, capture) consumes this unchanged; the deliberation faculty reads
    /// [`turns`](Self::turns) instead to recover role attribution.
    pub world_state: String,
    /// The CANONICAL structured form of the burst — the attributed conversation
    /// the deliberation faculty turns into role-separated `Vec<ChatMessage>`
    /// (own posts → `assistant`, peers → `user`). `world_state` is its text
    /// projection; this is the source of truth for WHO said what. A raw-string
    /// burst collapses to a single opaque turn (rendered identically to today).
    pub turns: Vec<BurstTurn>,
    /// The wake/speech CONTEXT — the room/conversation the turn is
    /// for (the third ID tier, contextId; see
    /// docs/architecture/IDENTITY-SCOPE-PEER-LIVENESS-MODEL.md Part A). Faculties
    /// retain this provenance; task grounding and actions use `activity_room_id`.
    /// A wake in another room does not move the card's work there. `Uuid::nil()` only in faculty-isolation tests that
    /// don't run in a room. NEVER a session id — context is durable, session is
    /// ephemeral and never load-bearing for where an action lands.
    /// Guaranteed non-nil in production: the only live constructor is
    /// [`from_burst`](Self::from_burst) and a [`Burst`] carries a witnessed
    /// [`ActivityRoom`](crate::identity::ActivityRoom) (#425).
    pub room_id: Uuid,
    /// Subject activity resolved from this step's admitted hands. Grounding and acts
    /// share this scope; room_id remains wake/speech provenance.
    pub activity_room_id: Uuid,
    /// Why this turn is happening — carried from [`Burst::cause`] so downstream
    /// receipts (prompt capture, dataset fork-guard #427) can tell a LIVED turn
    /// ("stimulus"/"ambient") from an eval-fork/fixture tick ("synthetic")
    /// without inferring it from a nil room (which no longer exists).
    pub cause: Cause,
    /// Which service tick this workspace IS — the frame index every finding
    /// computed against it gets stamped with. [`CycleId::UNSTAMPED`] for
    /// hand-built / replay-reconstructed workspaces; the cycle loop sets the
    /// live value via [`with_cycle`](Self::with_cycle).
    pub cycle: CycleId,
    /// What entered the bounded workspace and is broadcast (the persona's "now").
    pub broadcast: Vec<Contribution>,
    /// The turn's addressing fact and admission opportunity. Prompt framing reads
    /// [`directed_at_self`](Self::directed_at_self); resource scheduling reads
    /// [`TurnAttention::requires_priority`]. A human speaker alone does not mean
    /// the message names this persona.
    pub attention: TurnAttention,
    /// Every generation this turn dispatched, faults included (card 0d51573a).
    ///
    /// **ORDER IS PHASE-THEN-FACULTY, NOT CHRONOLOGY.** `join_all` resolves in INPUT
    /// order, so this is: phase 1 bids in faculty order, then phase 2, with each
    /// faculty's own receipts in the order it produced them. A faculty that awaited a
    /// slow prerequisite before generating still appears at its input position, so
    /// this must NOT be read as wall-clock dispatch order. If true chronology is ever
    /// needed, it needs a real submission ordinal rather than a re-reading of this
    /// sequence (Astra, 2026-09-08).
    ///
    /// **PROVENANCE IS NOT SUBJECT TO ATTENTION.** Deliberately NOT derived from
    /// [`broadcast`](Self::broadcast): broadcast is the FOCUSED set after
    /// `SalienceArbiter` truncates to capacity, so a faculty that GENERATED but lost
    /// attention would lose its receipt — and an invisible fault is exactly what this
    /// card exists to prevent. What a turn generated is a fact about the turn; what
    /// earned a place in the prompt is a judgement about relevance. Populated from
    /// the same `all_bids` full-competition set the capture sink records.
    pub receipts: Vec<crate::ai::provenance::GenerationReceipt>,
    /// Submitted render facts from all bids, including bids omitted by attention.
    /// The resident owner must persist consumption before retiring a task wake.
    pub render_receipts: Vec<crate::cognition::render_receipt::RenderReceipt>,
    /// Is this a SELF-INITIATED turn — the never-stop heartbeat pursuing the
    /// persona's own thread with no inbound message — versus a turn driven by an
    /// arriving message or an examiner's question? Drives the `[Your own time]`
    /// framing block in the system prompt (relocated here from a text preamble
    /// concatenated onto the burst, so the framing lives in the system prompt and
    /// the conversation turns stay clean). `false` (the default) = message/eval
    /// driven.
    pub self_initiated: bool,
    /// This turn's deliverable is a change in the workspace (a held card's work
    /// turn). It sets the act allowance; the tool surface is the same on every turn
    /// (`cognition::tool_surface`).
    pub workspace_deliverable: bool,
    /// An initiative turn ([`TurnFraming::initiative`]): her hands and the act allowance,
    /// with no deliverable. Read by the deliberation faculty's act-turn test only.
    pub initiative: bool,
    /// The verbs this turn's ROOM authorizes, by command name, as its recipe declares
    /// them (`affordances[].command`). Stamped by the cycle from the node's experience
    /// source on every tick (S1). Empty = the recipe declares none, and the
    /// deliberation faculty offers the pre-S1 surface unchanged; non-empty = the
    /// surface is selected from these (plus the discovery pair, so a withheld verb
    /// stays one `commands/list` away). This is how "recipe = content-type + RULES"
    /// stops being half true: the rules reach her hands, not only the renderer.
    pub room_affordances: Vec<String>,
    /// The persona's NOW at burst assembly (see [`Burst::now_ms`]) — rendered as a
    /// [now …] line in the system prompt so time is a fact she can perceive (#125).
    pub now_ms: Option<u64>,
    /// Per-turn OUTPUT sink for STREAMING the deliberation's answer token-by-token
    /// (#169). `None` (the default) = the accumulate path — every current caller and
    /// test is byte-identical. When `Some`, the deliberation faculty generates through
    /// `generate_stream` instead of `generate_text` (the adapter returns the SAME full
    /// response either way — `generate_text` is literally `generate_stream` +
    /// accumulate), and forwards each [`GenerationChunk::Token`] here as it decodes so
    /// the caller can emit a progressive `persona.turn.delta` to the room / TTS /
    /// avatar. `GenerationChunk::Reasoning` is NOT forwarded — the model thinks
    /// deeply, only the ANSWER streams out ([[thinking-is-primary-never-suppress]]).
    /// Rides the Workspace because the cycle is room-agnostic (one persona, many
    /// rooms) so the output target is a per-TURN fact, not a per-cycle one. Cloneable
    /// (the sender is cheap/refcounted) so a cloned Workspace streams to the same
    /// channel; skipped by every capture/replay reader (it is live I/O, not state).
    pub token_sink: Option<tokio::sync::mpsc::UnboundedSender<crate::ai::adapter::GenerationChunk>>,
}

impl Workspace {
    /// TEST-ONLY faculty-isolation shorthand. Production has no roomless
    /// workspaces (#425): the burst carries its [`ActivityRoom`](crate::identity::ActivityRoom)
    /// and [`from_burst`](Self::from_burst) is the one live constructor.
    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn new(burst: impl Into<Burst>) -> Self {
        Self::from_burst(burst.into())
    }

    /// TEST-ONLY compat: construct with an explicit room override. Lets faculty
    /// tests exercise arbitrary rooms without hand-building attributed bursts.
    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn in_room(burst: impl Into<Burst>, room_id: Uuid) -> Self {
        let mut ws = Self::from_burst(burst.into());
        ws.room_id = room_id;
        ws
    }

    /// Construct from an attributed burst — the ONE production constructor.
    /// The room comes from the burst itself (witnessed non-nil at burst
    /// construction), so a workspace whose header renders room A can no longer
    /// be driven in room B or in nil: there is only one room, stated once.
    pub fn from_burst(burst: Burst) -> Self {
        let burst_now = burst.now_ms;
        Self {
            world_state: burst.rendered,
            turns: burst.turns,
            room_updates: burst.room_updates,
            room_id: burst.room.as_uuid(),
            activity_room_id: burst.room.as_uuid(),
            cause: burst.cause,
            cycle: CycleId::UNSTAMPED,
            broadcast: Vec::new(),
            attention: TurnAttention::Ambient,
            receipts: Vec::new(),
            render_receipts: Vec::new(),
            self_initiated: false,
            workspace_deliverable: false,
            initiative: false,
            room_affordances: Vec::new(),
            now_ms: burst_now,
            token_sink: None,
        }
    }

    /// The activity witness for this turn. Infallible by construction: the only
    /// production constructor is [`from_burst`](Self::from_burst) and a `Burst`
    /// cannot exist without a non-nil [`ActivityRoom`](crate::identity::ActivityRoom)
    /// — so the expect below can only fire if a `#[cfg(test)]` constructor leaked
    /// a nil room into production, which is exactly the bug it should crash on.
    pub fn room(&self) -> crate::identity::ActivityRoom {
        crate::identity::ActivityRoom::from_uuid(self.room_id)
            .expect("Workspace.room_id is witnessed non-nil at construction (#425)")
    }

    /// Attach a per-turn token sink for STREAMING the answer (#169). Builder form,
    /// mirroring [`directed`](Self::directed)/[`self_initiated`](Self::self_initiated).
    /// `None` stays the default everywhere; the live caller sets `Some` only for a
    /// turn it wants to stream to the room/TTS/avatar. See [`token_sink`](Self::token_sink).
    pub fn with_token_sink(
        mut self,
        sink: Option<tokio::sync::mpsc::UnboundedSender<crate::ai::adapter::GenerationChunk>>,
    ) -> Self {
        self.token_sink = sink;
        self
    }

    /// Mark whether this turn is directed AT the persona (builder form). See
    /// [`directed_at_self`](Self::directed_at_self) — `true` suppresses the silence
    /// escape so a question put to her is not ghosted; `false` keeps silence
    /// first-class for ambient participation.
    pub fn directed(mut self, directed: bool) -> Self {
        self.attention = TurnAttention::for_message(directed, false);
        self
    }

    pub fn with_attention(mut self, attention: TurnAttention) -> Self {
        self.attention = attention;
        self
    }

    /// Whether this message was actually addressed to the persona. Human
    /// opportunity affects scheduling, not this prompt-grounding fact.
    pub fn directed_at_self(&self) -> bool {
        self.attention.is_addressed()
    }

    /// Mark whether this turn is the self-initiated heartbeat (builder form). See
    /// [`self_initiated`](Self::self_initiated) — `true` emits the `[Your own time]`
    /// framing in the system prompt.
    pub fn self_initiated(mut self, self_initiated: bool) -> Self {
        self.self_initiated = self_initiated;
        self
    }

    pub fn workspace_deliverable(mut self, workspace_deliverable: bool) -> Self {
        self.workspace_deliverable = workspace_deliverable;
        self
    }

    pub fn initiative(mut self, initiative: bool) -> Self {
        self.initiative = initiative;
        self
    }

    /// Stamp the room's authorized verbs (builder form; see
    /// [`room_affordances`](Self::room_affordances)). The cycle calls this once
    /// per tick from the node's experience source.
    pub fn room_affordances(mut self, commands: Vec<String>) -> Self {
        self.room_affordances = commands;
        self
    }

    /// Stamp this workspace with the cycle it represents (builder form). The
    /// cycle loop calls this so every finding collected against it inherits the
    /// frame index — the seam a deferred faculty later reads to know how stale
    /// its own finding is.
    pub fn with_cycle(mut self, cycle: CycleId) -> Self {
        self.cycle = cycle;
        self
    }

    /// The assembled perception a phase-2 (deliberation) faculty conditions on:
    /// every context contribution that won attention this tick, newline-joined.
    /// This is where the persona reads what its perception faculties surfaced —
    /// roster/doctrine grounding, recall hits, and (the proprioception channel)
    /// her view's render of its own recent acts. A deliberation
    /// faculty scans THIS, not the raw `world_state` burst — the burst is the
    /// stimulus, the broadcast is what the mind actually knows going into the
    /// decision. (Replaces the deleted eval-only `[you just acted]` world-state
    /// fold: act-results now reach the mind through working memory, identically
    /// in the live heartbeat and the eval fork.)
    pub fn perceived(&self) -> String {
        self.broadcast
            .iter()
            .map(|c| c.content.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The participation decision that won attention this tick, if any. It is
    /// the highest-salience contribution that carries a [`Decision`] — i.e. the
    /// deliberation faculty's verdict, if it made it into the bounded workspace.
    pub fn decision(&self) -> Option<&Decision> {
        self.winning_verdict().and_then(|c| c.decision.as_ref())
    }

    /// The generation that produced [`decision`](Self::decision): the winning verdict's own
    /// last served receipt (a retry leaves a fault receipt before it). Read from the same
    /// selection as the decision, never from the turn-wide receipts, whose order is
    /// phase-then-faculty. Its tool calls carry it, so a command can bind what she submits to
    /// the turn that made it (card fe8554d2).
    pub fn decision_generation(&self) -> Option<String> {
        self.winning_verdict().and_then(|c| {
            c.receipts
                .iter()
                .rev()
                .find(|r| matches!(r.outcome, crate::ai::provenance::GenerationOutcome::Served { .. }))
                .map(|r| r.submitted_request_id.clone())
        })
    }

    /// The cost of the verdict that won attention this tick — latency + tokens of
    /// the deliberation model call behind the decision. `None` when no verdict
    /// carried metrics (a context-only tick, or a faculty stand-in that did no
    /// generation). Read by the settle loop to accumulate per-task speed/latency.
    pub fn metrics(&self) -> Option<TurnMetrics> {
        self.winning_verdict().and_then(|c| c.metrics)
    }

    /// The single decision-carrying contribution that won attention — the shared
    /// selection both [`decision`](Self::decision) and [`metrics`](Self::metrics)
    /// read from, so they can never disagree about WHICH verdict won.
    pub(super) fn winning_verdict(&self) -> Option<&Contribution> {
        self.broadcast
            .iter()
            .filter(|c| c.decision.is_some())
            .max_by(|a, b| {
                a.salience
                    .partial_cmp(&b.salience)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }

    /// The deliberation FAULT this tick, if the model call FAILED (as opposed to
    /// producing a verdict or a chosen `Pass`). Scans the broadcast for the
    /// highest-salience fault contribution ([`Contribution::deliberation_fault`]).
    /// `Some` means the settle step MUST surface `InferenceFailed` — a failed model
    /// is never a silence. The settle step checks this BEFORE [`decision`], so a
    /// fault can never be read as a `Pass` ([[fallbacks-are-illegal-fail-loud]]).
    /// Every generation THIS TURN dispatched, faults included, in phase-then-faculty
    /// order — see [`Workspace::receipts`], which this returns (card 0d51573a).
    ///
    /// This is the collection credit is traced through. There is no canonical
    /// singular request for a cycle — a turn may dispatch several calls and some may
    /// fail — so a single request id could not represent it, and
    /// [`Workspace::metrics`] aggregates across the cycle and cannot say which call
    /// produced what.
    ///
    /// Faults are INCLUDED deliberately. Filtering to served-only would make a turn
    /// that failed twice and succeeded once indistinguishable from a turn that
    /// succeeded once, and the staged record would then claim cleaner provenance
    /// than the turn actually had.
    ///
    /// Distinct from [`deliberation_fault`](Self::deliberation_fault), which is the
    /// contribution-level verdict failure the settle step turns into
    /// `InferenceFailed`. A turn can have fault RECEIPTS here and no deliberation
    /// fault at all — a retry that failed and then succeeded is exactly that.
    pub fn generation_receipts(
        &self,
    ) -> impl Iterator<Item = &crate::ai::provenance::GenerationReceipt> {
        self.receipts.iter()
    }

    pub fn prompt_capacity_refusal(&self) -> Option<CapacityRefusal> {
        self.broadcast
            .iter()
            .find_map(|c| c.capacity_refusal.clone())
    }

    pub fn deliberation_fault(&self) -> Option<&str> {
        self.broadcast
            .iter()
            .filter(|c| c.fault.is_some())
            .max_by(|a, b| {
                a.salience
                    .partial_cmp(&b.salience)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .and_then(|c| c.fault.as_deref())
    }
}

/// A cognitive faculty — a swappable ML adapter. The brain never knows whether
/// the backend is an LLM, a custom/sentinel-ai-forged specialist, or a
/// composite. Async because backends do real inference/IPC.
#[async_trait]
pub trait Faculty: Send + Sync {
    fn id(&self) -> FacultyId;
    /// Bid into the workspace given the current state. `None` = abstain.
    ///
    /// Called once in the faculty's phase (see [`Faculty::reacts_to_broadcast`]).
    /// Perception faculties see an empty `ws.broadcast` (they react to the raw
    /// world-state); deliberation faculties see the *assembled context* that won
    /// attention in phase 1. A faculty's intelligence is entirely here — the
    /// arbiter only integrates the salience it returns.
    async fn contribute(&self, ws: &Workspace) -> Option<Contribution>;

    /// The faculty's **dependency / phase** in the staged cycle — the reactive
    /// "what do I fire on" declaration (a faculty is a React hook; this is its
    /// dependency array). This is the direct analog of cbar's `needsRealTime()`
    /// bool that split real-time motion from delayed scene understanding: a
    /// *structural* scheduling declaration, **not** a cognition gate.
    ///
    /// - `false` (default) — **perception tier**: reacts to the raw world-state,
    ///   bids in phase 1 (recall, world-model, affect, salience, roster…).
    /// - `true` — **deliberation tier**: reacts to the *assembled broadcast* (the
    ///   context that won attention), bids in phase 2 so it can condition its
    ///   [`Decision`] on what recall/world-model/affect actually surfaced.
    ///
    /// This is what makes "pull relevant memory, *then* decide" expressible: the
    /// decider runs after, over the assembled context — cbar's lines→planes, GWT's
    /// broadcast-then-rebid. It does NOT enumerate faculties or privilege a
    /// decider; any faculty may be either tier.
    fn reacts_to_broadcast(&self) -> bool {
        false
    }
}

/// What the FOCUS LAYER knows about THIS tick beyond the raw bids — the ask, and
/// the situation it's in — so it can streamline the assembled context *for the
/// given ask* instead of dumping everything. This is the "REALLY good hints"
/// seam ([[persona-brain-reactive-cognition]], `docs/cognition/REALLY-GOOD-HINTS.md`):
/// the focuser curates the INPUT context the model reasons over (legitimate
/// attention, exactly what every brain does), it NEVER reads or filters the
/// model's OUTPUT ([[no-hardcoded-heuristics-to-steer-cognition]] — the forbidden
/// move). Borrows the tick's world-state so the hot path copies a pointer, not a
/// string.
pub struct FocusContext<'a> {
    /// The consolidated burst / ask this tick reasoned over (the world-state).
    pub world_state: &'a str,
    /// The situation this tick is in — see [`Situation`].
    pub situation: Situation,
}

/// The situation a tick is in, as a TYPED signal — never inferred by reading the
/// burst text back (that brittle string-matching is the heuristic we forbid). It
/// tells a situation-aware focuser how much context the ask actually needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Situation {
    /// A fresh ask / new burst: the persona may need fuller standing grounding
    /// (who's here, what the room is for) to decide well. The safe default — when
    /// in doubt, ground more, never less.
    #[default]
    FreshContext,
    /// The last externalized act was a tool run and this tick re-perceives its
    /// result. The persona doesn't need the standing grounding re-dumped — it
    /// needs the result + the affordances for "what next" — so a focuser can drop
    /// re-grounding and tighten the context. Carried from the act→observe loop.
    PostAction,
}
