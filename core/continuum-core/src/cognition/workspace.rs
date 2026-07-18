//! The Global Workspace — the brain's integration core.
//!
//! See `docs/architecture/PERSONA-BRAIN-ARCHITECTURE.md`. Cognition is a
//! federation of **faculties** (swappable ML adapters). Each service tick (over
//! a *consolidated burst*, never per-event) the faculties bid in parallel into
//! a bounded **Workspace**; a pluggable **Arbiter** integrates their bids
//! (attention), and the winners are broadcast. The persona's participation
//! **Decision** is the *output of the deliberation faculty's thinking* over that
//! workspace — never a heuristic gate, never an `@`-trigger, never a sender caste.
//!
//! This is built ALONGSIDE the live loop; it does not yet replace
//! `calculate_priority`/`fast_path`. Cut-over lands once it's tested + the
//! recipe-executor (the servicing substrate) calls into it.
//!
//! ## Interface contract (for the recipe-executor `ai/should-respond` step)
//! - `Faculty::contribute` is **async** (backends do inference/IPC).
//! - It consumes `&Workspace` (the consolidated world-state + current broadcast).
//! - It returns `Option<Contribution>` (`None` = abstain this tick).
//! - Faculties are **per-persona instances** (each mind owns its faculties);
//!   model *backends* may be shared. Look one up by `FacultyId`.
//! - The participation result is a typed [`Decision`] carried by the
//!   deliberation faculty's contribution.

use std::sync::Arc;

use async_trait::async_trait;
use futures_util::future::join_all;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::llm_deliberation_faculty::{
    empty_genome, relaxed_decoding, DecodingHandle, GenomeHandle, ModelBinding, ModelBindingHandle,
};
use crate::ai::types::{ActiveAdapterRequest, ToolCall};

/// Identifier for a cognitive faculty — a *structural name* (like a brain
/// region), NOT a cognition decision. `Custom` keeps the set open so new
/// faculties (incl. sentinel-ai-forged ones) need no enum edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FacultyId {
    /// Sophisticated learned recall over engrams (hippocampal relevance).
    Recall,
    /// Generative model of the (multimodal, channel/recipe-shaped) world.
    WorldModel,
    /// Affect / arousal — neuromodulatory gain.
    Affect,
    /// Self-generated goals + curiosity (active inference policy proposals).
    Volition,
    /// The reasoner: produces the participation [`Decision`]. LLM-grade floor.
    Deliberation,
    /// Optional fast pre-attention salience (scheduling, never the decider).
    Salience,
    /// Open extension point — sentinel-ai faculties, future regions.
    Custom(String),
}

impl FacultyId {
    pub fn as_str(&self) -> &str {
        match self {
            FacultyId::Recall => "recall",
            FacultyId::WorldModel => "world-model",
            FacultyId::Affect => "affect",
            FacultyId::Volition => "volition",
            FacultyId::Deliberation => "deliberation",
            FacultyId::Salience => "salience",
            FacultyId::Custom(s) => s,
        }
    }

    /// Inverse of [`as_str`]: parse a kebab tag back into a `FacultyId`. Total —
    /// an unknown tag becomes `Custom(tag)`, so a sentinel-forged faculty (or a
    /// `cognition/replay` caller naming a faculty by string) round-trips without
    /// an enum edit. This is the ONE place the tag→variant mapping lives; keep it
    /// the mirror of `as_str` so the two never drift.
    pub fn from_kebab(tag: &str) -> FacultyId {
        match tag {
            "recall" => FacultyId::Recall,
            "world-model" => FacultyId::WorldModel,
            "affect" => FacultyId::Affect,
            "volition" => FacultyId::Volition,
            "deliberation" => FacultyId::Deliberation,
            "salience" => FacultyId::Salience,
            other => FacultyId::Custom(other.to_string()),
        }
    }
}

/// The persona's participation decision — the OUTPUT of the deliberation
/// faculty thinking over the consolidated burst. This is what a recipe
/// `ai/should-respond` step returns. It is a *thought's result*, not a gate:
/// silence (`Pass`) and unprompted initiative (`RaiseUnprompted`) are
/// first-class, equal to `Speak`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Decision {
    /// Respond to the thread with this content.
    Speak { text: String },
    /// Raise something no one asked for — initiative, not reaction.
    RaiseUnprompted { text: String },
    /// **Act on the world.** The mind reached for its hands — run code, search
    /// the web, read a file, drive its own avatar. This is a first-class verdict,
    /// peer to `Speak`: the deliberation faculty emits it when the model emitted
    /// tool calls. The driver executes the `calls` through the persona's
    /// identity-bearing `ToolExecutor` (the ACL gate decides what's allowed) and
    /// — crucially — the RESULT re-enters as an Episodic engram next tick, so the
    /// mind *perceives* what its hands did. It is NOT a synchronous call whose
    /// return value a faculty consumes inside one tick (that was the textbook
    /// inner loop we deleted); it is an action whose effect becomes memory.
    ///
    /// `intent` is the mind's own words for WHY it acted ("run the failing test
    /// to see the traceback"). It is captured into the observation engram so next
    /// tick she remembers the *reason*, not just the *result*. See
    /// `docs/cognition/ACTING-ORGANISM.md`.
    Act {
        calls: Vec<ToolCall>,
        intent: String,
    },
    /// Nothing worth adding this turn (the persona's own judgment, not a gate).
    /// Together with `Speak`/`RaiseUnprompted`, this is how the organism SETTLES:
    /// the absence of an `Act` bid is the mind's judgment that the work is done.
    Pass,
}

/// The cost of producing ONE deliberation verdict: how long the model took and
/// how many tokens it moved. Stamped by the deliberation faculty onto the verdict
/// [`Contribution`] from the adapter's `TextGenerationResponse` (`response_time_ms`
/// + `usage`), so latency and throughput ride out of the brain on the SAME path as
/// the decision — never inferred after the fact. Accumulates across the act→observe
/// settle loop (sum tokens, sum latency) so a multi-act task reports its total cost.
/// `Copy` + `Default` so the live heartbeat can ignore it for free and the eval can
/// fold it without ceremony. This is the speed/latency half of the four-axis
/// scoreboard (the other half — accuracy + lift — is the gym grade).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TurnMetrics {
    /// Prompt tokens the model conditioned on (the perception bid's size).
    pub input_tokens: u32,
    /// Tokens the model generated (the verdict's length).
    pub output_tokens: u32,
    /// Wall-clock the generation took, end to end (the adapter's measured
    /// `response_time_ms` — request send to full response).
    pub latency_ms: u64,
    /// KV-prefix tokens served from cache (lane `cache_n`), summed across acts.
    /// High vs `prefill_tokens` = the static identity+catalog prefix stayed
    /// resident across the settle loop. 0 when the lane reports no timings.
    pub cached_tokens: u32,
    /// NEW prompt tokens the lane actually prefilled (lane `prompt_n`), summed.
    /// THIS — not total prompt size — is the prefill cost the KV cache governs;
    /// it is the re-rasterization tax to drive toward zero.
    pub prefill_tokens: u32,
    /// Lane wall-ms spent PREFILLING (lane `prompt_ms`), summed. On Apple-Silicon
    /// Metal this dominates wall-clock; separated from decode so the harness can
    /// attack it directly instead of guessing from one conflated tok/s.
    pub prefill_ms: u64,
    /// Lane wall-ms spent DECODING (lane `predicted_ms`), summed.
    pub decode_ms: u64,
}

impl TurnMetrics {
    /// Decode throughput in tokens/sec — `output_tokens / latency`. The headline
    /// speed number. Zero when no time elapsed (avoids a divide-by-zero NaN that
    /// would poison the aggregate). NOTE: this is WALL-CLOCK tok/s — diluted by
    /// prefill + cognition overhead. For the lane's undiluted generation rate use
    /// [`decode_tokens_per_second`](Self::decode_tokens_per_second).
    pub fn tokens_per_second(&self) -> f64 {
        if self.latency_ms == 0 {
            return 0.0;
        }
        self.output_tokens as f64 / (self.latency_ms as f64 / 1000.0)
    }

    /// REAL decode throughput from the lane's own clock — `output_tokens /
    /// decode_ms` — undiluted by prefill or cognition overhead. This is the honest
    /// generation speed the wall-clock `tokens_per_second()` can't isolate. 0 when
    /// the lane reported no decode time (provider omitted timings).
    pub fn decode_tokens_per_second(&self) -> f64 {
        if self.decode_ms == 0 {
            return 0.0;
        }
        self.output_tokens as f64 / (self.decode_ms as f64 / 1000.0)
    }

    /// Fraction of prompt tokens served from KV cache — `cached / (cached +
    /// prefilled)`. 1.0 = fully warm prefix (cheap); low = re-encoding the prompt
    /// every act (the inefficiency to attack). 0 when the lane reported no prompt
    /// tokens (provider omitted timings).
    pub fn cache_hit_rate(&self) -> f64 {
        let total = self.cached_tokens.saturating_add(self.prefill_tokens);
        if total == 0 {
            return 0.0;
        }
        self.cached_tokens as f64 / total as f64
    }

    /// Fold another turn's cost in (the settle loop accumulates each act→observe
    /// generation into the task's total). Tokens and latency are additive; tok/s is
    /// always re-derived from the totals, never averaged (averaging rates lies).
    pub fn accumulate(&mut self, other: TurnMetrics) {
        self.input_tokens = self.input_tokens.saturating_add(other.input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(other.output_tokens);
        self.latency_ms = self.latency_ms.saturating_add(other.latency_ms);
        self.cached_tokens = self.cached_tokens.saturating_add(other.cached_tokens);
        self.prefill_tokens = self.prefill_tokens.saturating_add(other.prefill_tokens);
        self.prefill_ms = self.prefill_ms.saturating_add(other.prefill_ms);
        self.decode_ms = self.decode_ms.saturating_add(other.decode_ms);
    }
}

/// Which service tick (cycle) a finding was computed against — the cognition
/// analog of cbar's per-finding `frameIndex`. A faculty reasons against a
/// [`Workspace`] that IS a particular cycle; its [`Contribution`] is stamped
/// with that cycle so a *late* finding (a slow/deferred faculty that lands a
/// tick or three after it reasoned) knows its own time and can be reconciled
/// forward against the moved-on world instead of silently pretending to be
/// current — the decoupling primitive: a finding can only land late safely once
/// it knows its own time. `UNSTAMPED` (0) = built but not yet attached to a live
/// cycle (the constructor default; the cycle loop stamps the real value).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct CycleId(pub u64);

impl CycleId {
    /// A contribution built but not yet attached to a live cycle.
    pub const UNSTAMPED: CycleId = CycleId(0);
}

/// What a faculty surfaces into the workspace this service tick.
#[derive(Debug, Clone)]
pub struct Contribution {
    pub faculty: FacultyId,
    /// The cycle this finding was computed against (cbar `frameIndex` analog).
    /// Stamped by the cycle loop at collection; [`CycleId::UNSTAMPED`] until then.
    pub cycle: CycleId,
    /// Human/LLM-readable content the faculty surfaces (recalled memory, a
    /// predicted world-state, an affect signal, a proposed utterance).
    pub content: String,
    /// **ML-derived** salience the faculty assigns its OWN contribution
    /// (`0.0..=1.0`): how much its model thinks this matters now. The arbiter
    /// integrates these ML scores; it never invents salience itself.
    pub salience: f32,
    /// Why — for audit/replay; the brain is observable.
    pub reasoning: String,
    /// Set by the deliberation faculty: the participation decision. Other
    /// faculties leave this `None` (they contribute context, not the verdict).
    pub decision: Option<Decision>,
    /// The cost of producing this contribution, when it came from a model call
    /// (the deliberation verdict). `None` for context contributions (recall,
    /// affect) that did no generation. Surfaced via [`Workspace::metrics`].
    pub metrics: Option<TurnMetrics>,
    /// **Session-stable** content — standing framing that rarely changes within a
    /// session (the room roster, the room's operating doctrine, the workspace map),
    /// as opposed to volatile per-turn grounding (recall, working memory). The
    /// serializer emits stable contributions FIRST so they sit in the cacheable
    /// KV-prefix region adjacent to the static system prompt (which is what they
    /// are — standing framing is "like the system prompt"), while volatile content
    /// lands LAST, nearest the generation point (best instruction-following AND
    /// minimal re-prefill). Defaults `false` (volatile); set via [`session_stable`].
    /// This is a SERIALIZATION-order property, not an attention one — salience still
    /// governs which contributions are included and truncated.
    pub stable: bool,
    /// Set ONLY by the deliberation faculty when the model call itself FAILED — a
    /// timeout, a 5xx, or the serving lane refusing a model it isn't hosting. This
    /// is NOT a [`Decision`]: a failed inference is neither a chosen silence nor a
    /// verdict, and it must never collapse into a `Pass`
    /// ([[fallbacks-are-illegal-fail-loud]]). Carried into the broadcast so the
    /// fault is auditable/replayable like any finding, and read by
    /// [`Workspace::deliberation_fault`] — which the settle step turns into a
    /// distinct `InferenceFailed` outcome instead of a lying `Passed`. `None` on
    /// every healthy contribution.
    pub fault: Option<String>,
}

impl Contribution {
    /// A context contribution (no decision) — recall, world-model, affect, etc.
    pub fn context(
        faculty: FacultyId,
        content: impl Into<String>,
        salience: f32,
        reasoning: impl Into<String>,
    ) -> Self {
        Self {
            faculty,
            cycle: CycleId::UNSTAMPED,
            content: content.into(),
            salience: salience.clamp(0.0, 1.0),
            reasoning: reasoning.into(),
            decision: None,
            metrics: None,
            stable: false,
            fault: None,
        }
    }

    /// The deliberation faculty's verdict contribution.
    pub fn verdict(decision: Decision, salience: f32, reasoning: impl Into<String>) -> Self {
        let content = match &decision {
            Decision::Speak { text } | Decision::RaiseUnprompted { text } => text.clone(),
            // The mind's narration of WHY it's acting — surfaced/audited like any
            // contribution content; the calls themselves live on the decision.
            Decision::Act { intent, .. } => intent.clone(),
            Decision::Pass => String::new(),
        };
        Self {
            faculty: FacultyId::Deliberation,
            cycle: CycleId::UNSTAMPED,
            content,
            salience: salience.clamp(0.0, 1.0),
            reasoning: reasoning.into(),
            decision: Some(decision),
            metrics: None,
            // A verdict is the volatile output of THIS turn; never standing framing.
            stable: false,
            fault: None,
        }
    }

    /// A **deliberation fault** — the model call FAILED and produced no verdict.
    /// Emitted by the deliberation faculty in place of a silent `None` so the
    /// failure rides the broadcast (auditable, replayable) and the settle step
    /// surfaces it LOUD as `InferenceFailed`, never masked as a `Pass`
    /// ([[fallbacks-are-illegal-fail-loud]]). Not a [`Decision`]: an inference
    /// failure is neither speech, act, nor a chosen silence. Max salience so it
    /// always wins [`Workspace::deliberation_fault`]'s scan; `decision` stays
    /// `None` (no verdict was produced), `fault` carries the named cause.
    pub fn deliberation_fault(error: impl Into<String>) -> Self {
        let error = error.into();
        Self {
            faculty: FacultyId::Deliberation,
            cycle: CycleId::UNSTAMPED,
            content: error.clone(),
            salience: 1.0,
            reasoning: "deliberation inference failed".to_string(),
            decision: None,
            metrics: None,
            stable: false,
            fault: Some(error),
        }
    }

    /// Stamp the model-call cost onto this contribution (builder form, so the
    /// deliberation faculty can do `self.verdict(...).with_metrics(m)`).
    pub fn with_metrics(mut self, metrics: TurnMetrics) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Mark this contribution as **session-stable** standing framing (roster,
    /// doctrine, workspace map) — builder form so a faculty can do
    /// `Contribution::context(...).session_stable()`. The serializer hoists stable
    /// contributions into the cacheable KV-prefix region (see [`Contribution::stable`]).
    pub fn session_stable(mut self) -> Self {
        self.stable = true;
        self
    }
}

/// One attributed turn in the burst the persona reasons over — a single
/// message in the conversation, with WHO said it preserved as structure rather
/// than flattened into a `Name:`-prefixed line. This is the unit that lets the
/// deliberation faculty assemble role-attributed `Vec<ChatMessage>` (the
/// persona's OWN posts → `assistant`, peers' → `user`; PERSONA-COGNITION-PIPELINE
/// §7.5) instead of collapsing the whole conversation into one `user` message —
/// the defect that caused identity bleed, transcript replay, and the echo loop.
#[derive(Debug, Clone)]
pub struct BurstTurn {
    /// Did THIS persona author the turn? `true` → it renders as an `assistant`
    /// message (her own voice); `false` → a peer's `user` message. The ONE fact
    /// role attribution turns on — carried as structure so the deliberation
    /// faculty never has to guess from a `Name:` prefix.
    pub is_self: bool,
    /// Display name of the author (the persona's `agent_name` for self, the
    /// roster `display_name` for peers, the raw `peer_id` when unresolved —
    /// honest, never fabricated). Empty for an opaque turn (raw-string burst with
    /// no authorship — eval/test stimuli).
    pub author: String,
    /// The message body.
    pub content: String,
    /// When it occurred (airc `occurred_at_ms`), if known. Drives the `[t=…]`
    /// timestamp prefix in the rendered projection so a lived turn and a measured
    /// one render byte-identically.
    pub occurred_at_ms: Option<u64>,
}

impl BurstTurn {
    /// An attributed turn — the live/eval path that knows WHO spoke and WHEN.
    pub fn attributed(
        is_self: bool,
        author: impl Into<String>,
        content: impl Into<String>,
        occurred_at_ms: Option<u64>,
    ) -> Self {
        Self {
            is_self,
            author: author.into(),
            content: content.into(),
            occurred_at_ms,
        }
    }

    /// An OPAQUE turn — a raw-string burst with no authorship (a hand-built eval
    /// stimulus, a faculty-isolation test input, a replay-reconstructed
    /// world-state). NOT a fallback ([[fallbacks-are-illegal-fail-loud]]): these
    /// inputs genuinely have no author, so the honest structure is one unattributed
    /// turn — the deliberation faculty renders it as a single `user` message,
    /// byte-identical to today.
    pub fn opaque(content: impl Into<String>) -> Self {
        Self {
            is_self: false,
            author: String::new(),
            content: content.into(),
            occurred_at_ms: None,
        }
    }

    /// Render this turn as ONE line of the world-state projection, matching the
    /// historical `build_workspace_burst` format exactly: `[t={ms}] {author}:
    /// {content}` when timed, `{author}: {content}` when authored-but-untimed,
    /// the bare content (no prefix, no added newline) when opaque.
    fn write_line(&self, out: &mut String) {
        use std::fmt::Write as _;
        if self.author.is_empty() {
            // Opaque: preserve the raw burst verbatim (it may already contain its
            // own newlines / structure — adding a prefix or trailing newline would
            // change the bytes every text reader of `world_state` sees).
            let _ = write!(out, "{}", self.content);
        } else if let Some(t) = self.occurred_at_ms {
            let _ = writeln!(out, "[t={t}] {}: {}", self.author, self.content);
        } else {
            let _ = writeln!(out, "{}: {}", self.author, self.content);
        }
    }
}

/// The assembled burst a persona reasons over THIS tick, carried as structure
/// (`turns`) with its text projection (`rendered`) materialized once. `turns` is
/// the canonical form the deliberation faculty reads to build role-attributed
/// messages; `rendered` is the flat string every other (text) reader of
/// `world_state` keeps consuming unchanged — one rendering decision, one place
/// (the compression principle). Construct via [`Burst::from_turns`] (the live /
/// eval path, with authorship) or via `From<String>`/`From<&str>` (a raw-string
/// stimulus → a single opaque turn rendered verbatim).
#[derive(Debug, Clone)]
pub struct Burst {
    /// The persona's NOW at assembly (wall-clock ms; eval passes its pinned epoch;
    /// None for raw-string/test bursts). Threaded to the prompt as a [now …] line
    /// (task #125 — the rendered header never reaches the structured-turns prompt).
    pub now_ms: Option<u64>,
    /// The structured conversation — the unit the deliberation faculty attributes
    /// to `assistant`/`user` roles. Excludes the room header (standing context
    /// that belongs in the system prompt, not the conversation).
    pub turns: Vec<BurstTurn>,
    /// The text projection of `turns` (+ room header) — what `world_state` IS.
    /// Materialized once at construction so the hot path never re-renders.
    pub rendered: String,
}

impl Burst {
    /// Assemble an attributed burst: a `[room {room}]` header followed by each
    /// turn rendered to its historical line format. The header is rendered INTO
    /// `rendered` (so `world_state` is byte-identical to the old
    /// `build_workspace_burst`) but deliberately kept OUT of `turns` — room
    /// identity is standing context for the system prompt, not a conversation turn.
    pub fn from_turns(room: Uuid, turns: Vec<BurstTurn>) -> Self {
        Self::from_turns_at(room, turns, None)
    }

    /// Like [`from_turns`](Self::from_turns) but stamps the persona's NOW into the
    /// header — the clock in her perception (task #125 prospective memory: a being
    /// who commits to "tomorrow at 2 PM" must know when now IS; glass-boxed live,
    /// her prompts carried no time referent at all, so appointments were words she
    /// could never act on). The clock is a PARAMETER, never an ambient global read:
    /// the live path passes wall-clock, the eval passes its pinned epoch (exams stay
    /// byte-reproducible), tests pass fixtures. Rendered at MINUTE granularity so
    /// the prompt prefix — and the serving KV cache — only changes once a minute.
    pub fn from_turns_at(room: Uuid, turns: Vec<BurstTurn>, now_ms: Option<u64>) -> Self {
        use std::fmt::Write as _;
        let mut rendered = String::new();
        let _ = writeln!(rendered, "[room {room}]");
        if let Some(ms) = now_ms {
            if let Some(dt) = chrono::DateTime::from_timestamp_millis(ms as i64) {
                let local = dt.with_timezone(&chrono::Local);
                let _ = writeln!(rendered, "[now {}]", local.format("%Y-%m-%d %H:%M %A"));
            }
        }
        for turn in &turns {
            turn.write_line(&mut rendered);
        }
        Self { turns, rendered, now_ms }
    }
}

impl From<String> for Burst {
    /// A raw-string burst → ONE opaque turn, rendered verbatim. Keeps every
    /// string-passing call site (faculty tests, eval shorthand, replay) compiling
    /// and byte-identical.
    fn from(s: String) -> Self {
        Self {
            turns: vec![BurstTurn::opaque(s.clone())],
            rendered: s,
            now_ms: None,
        }
    }
}

impl From<&str> for Burst {
    fn from(s: &str) -> Self {
        Burst::from(s.to_string())
    }
}

impl From<&String> for Burst {
    fn from(s: &String) -> Self {
        Burst::from(s.clone())
    }
}

/// How THIS turn is framed for the persona — the two structural facts the system
/// prompt reflects: is it DIRECTED at her (suppress the silence escape) and is it
/// SELF-INITIATED (the heartbeat pursuing her own thread, vs a message/eval
/// driving it). Replaces the lone `directed: bool` that used to thread through
/// the run/settle seam, so the self-initiated framing lives in the system prompt
/// instead of being concatenated onto the burst text. Both facts are STRUCTURAL
/// (addressing / scheduling origin), never a read of her output
/// ([[no-hardcoded-heuristics-to-steer-cognition]]).
#[derive(Debug, Clone, Copy, Default)]
pub struct TurnFraming {
    /// Directed AT the persona (@mention / DM / examiner question) — see
    /// [`Workspace::directed_at_self`].
    pub directed: bool,
    /// The never-stop heartbeat pursuing her own thread (no inbound message) —
    /// see [`Workspace::self_initiated`].
    pub self_initiated: bool,
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
            directed: true,
            self_initiated: false,
        }
    }

    /// The message path: a turn driven by an inbound message (not self-initiated),
    /// `directed` iff she was actually named. `false` collapses to [`ambient`] —
    /// silence stays first-class for room chatter she wasn't addressed in.
    pub fn message(directed: bool) -> Self {
        Self {
            directed,
            self_initiated: false,
        }
    }

    /// The self-initiated heartbeat. Directed iff she was named in what she
    /// perceived (so a question reaching her on the digest path is not ghosted).
    pub fn self_thread(directed: bool) -> Self {
        Self {
            directed,
            self_initiated: true,
        }
    }
}

/// The bounded global workspace: the consolidated world-state being reasoned
/// over (channel/recipe-shaped — a text thread, a game space + player
/// positions, an AR scene, a code diff) plus what won attention and is
/// broadcast back to all faculties.
#[derive(Debug, Clone)]
pub struct Workspace {
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
    /// The CONTEXT this tick reasons within — the room/conversation the turn is
    /// for (the third ID tier, contextId; see
    /// docs/architecture/IDENTITY-SCOPE-PEER-LIVENESS-MODEL.md Part A). Faculties
    /// scope their actions to it: the deliberation faculty stamps tool calls with
    /// this room so a persona's hands act in the SAME room the turn is about, not
    /// a phantom `nil` room. `Uuid::nil()` only in faculty-isolation tests that
    /// don't run in a room. NEVER a session id — context is durable, session is
    /// ephemeral and never load-bearing for where an action lands.
    pub room_id: Uuid,
    /// Which service tick this workspace IS — the frame index every finding
    /// computed against it gets stamped with. [`CycleId::UNSTAMPED`] for
    /// hand-built / replay-reconstructed workspaces; the cycle loop sets the
    /// live value via [`with_cycle`](Self::with_cycle).
    pub cycle: CycleId,
    /// What entered the bounded workspace and is broadcast (the persona's "now").
    pub broadcast: Vec<Contribution>,
    /// Is this turn DIRECTED at this persona — a direct @mention, a DM, or (in the
    /// eval fork) an examiner's question put TO her? When `true`, declining the turn
    /// by emitting the bare PASS token is NOT a legitimate option: the [Silence
    /// Option] affordance is for *ambient* participation (room chatter she is free to
    /// let pass), never for ghosting a question asked of her. The deliberation
    /// faculty reads this to decide whether to OFFER the silence escape — a framing
    /// decision over a structural addressing fact (like ACL/routing), NOT a filter on
    /// her output. `false` (the default) = ambient: silence stays first-class.
    pub directed_at_self: bool,
    /// Is this a SELF-INITIATED turn — the never-stop heartbeat pursuing the
    /// persona's own thread with no inbound message — versus a turn driven by an
    /// arriving message or an examiner's question? Drives the `[Your own time]`
    /// framing block in the system prompt (relocated here from a text preamble
    /// concatenated onto the burst, so the framing lives in the system prompt and
    /// the conversation turns stay clean). `false` (the default) = message/eval
    /// driven.
    pub self_initiated: bool,
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
    pub fn new(burst: impl Into<Burst>) -> Self {
        Self::in_room(burst, Uuid::nil())
    }

    /// Construct scoped to a specific room/context (the contextId the turn acts
    /// within). The live persona path always uses this; `new` is the nil-room
    /// shorthand for faculty-isolation tests. Takes `impl Into<Burst>`: an
    /// attributed `Burst` (live/eval, carries authorship) or a raw `String`/`&str`
    /// (collapses to one opaque turn — faculty tests, replay).
    pub fn in_room(burst: impl Into<Burst>, room_id: Uuid) -> Self {
        let burst = burst.into();
        let burst_now = burst.now_ms;
        Self {
            world_state: burst.rendered,
            turns: burst.turns,
            room_id,
            cycle: CycleId::UNSTAMPED,
            broadcast: Vec::new(),
            directed_at_self: false,
            self_initiated: false,
            now_ms: burst_now,
            token_sink: None,
        }
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
        self.directed_at_self = directed;
        self
    }

    /// Mark whether this turn is the self-initiated heartbeat (builder form). See
    /// [`self_initiated`](Self::self_initiated) — `true` emits the `[Your own time]`
    /// framing in the system prompt.
    pub fn self_initiated(mut self, self_initiated: bool) -> Self {
        self.self_initiated = self_initiated;
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
    /// the `WorkingMemoryFaculty`'s render of its own recent acts. A deliberation
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
    fn winning_verdict(&self) -> Option<&Contribution> {
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

/// Attention / FOCUS: selects AND consolidates which contributions enter the
/// bounded workspace, given the situation. Pluggable so a *learned* focuser
/// (itself a faculty) can replace the bootstrap. The bootstrap
/// ([`SalienceArbiter`]) is a blind top-k over the faculties' OWN ML-derived
/// salience — mechanical integration of ML scores (like attention's softmax
/// top-k), NOT a hand-coded cognition rule. A situation-aware [`FocusArbiter`]
/// consolidates *for the given ask* (post-action → minimal; fresh → fuller),
/// dedups overlap, and respects a token budget rather than a raw count. The
/// intelligence lives in the faculties (and, later, a learned focuser); the
/// arbiter only integrates and focuses it.
pub trait Arbiter: Send + Sync {
    /// Focus `candidates` down to what should enter the bounded workspace for
    /// THIS tick's `ctx`. INPUT-side attention only — curates the context the
    /// model attends to, never the model's output.
    fn focus(
        &self,
        candidates: Vec<Contribution>,
        capacity: usize,
        ctx: &FocusContext<'_>,
    ) -> Vec<Contribution>;
}

/// Top-k by ML salience within the workspace's bounded capacity.
///
/// This is the **bootstrap** policy: pure exploitation, attention at temperature
/// 0 — the highest-salience bids always win. It is *greedy*, so on its own it
/// collapses to safe convergence (the obvious bid wins every tick; divergent /
/// creative bids get truncated). Encouraging creativity is an
/// exploration-preserving arbiter policy that slots in here — reserve part of
/// capacity for high-epistemic-value / divergent bids so they aren't crowded out
/// (the active-inference exploration term; see PERSONA-BRAIN-ARCHITECTURE.md
/// §3.5). It is a documented seam, NOT built yet — it waits on a Volition faculty
/// that emits an epistemic-value signal, so no novelty metric is invented
/// prematurely.
pub struct SalienceArbiter;

impl Arbiter for SalienceArbiter {
    /// Blind top-k by ML salience — ignores the situation (`_ctx`). It is the
    /// floor (outlier A): every richer, situation-aware focuser must beat THIS on
    /// the scoreboard to earn its latency.
    fn focus(
        &self,
        mut candidates: Vec<Contribution>,
        capacity: usize,
        _ctx: &FocusContext<'_>,
    ) -> Vec<Contribution> {
        candidates.sort_by(|a, b| {
            b.salience
                .partial_cmp(&a.salience)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        candidates.truncate(capacity);
        candidates
    }
}

/// Situation-aware focuser (outlier B to [`SalienceArbiter`]'s outlier A) — the
/// first policy that actually READS `ctx.situation` instead of ignoring it, and
/// the wire that makes [`Situation::PostAction`]'s documented promise real:
/// *"the persona doesn't need the standing grounding re-dumped — it needs the
/// result + the affordances for 'what next' — so a focuser can drop re-grounding
/// and tighten the context."*
///
/// This is the mechanism behind "straightforward for whatever their current state
/// requires": when a persona is heads-down driving an act→observe loop (write →
/// compile → run → re-perceive the result), every re-perception tick is
/// `PostAction`. On those ticks the standing SOCIAL framing — room roster,
/// operating doctrine, workspace map — was already perceived on the fresh tick and
/// only crowds the window; the code, the tool result, and working memory are what
/// the "what next" decision turns on. So on `PostAction` we drop the
/// [`Contribution::stable`] standing-framing contributions BEFORE the salience
/// top-k, leaving the volatile task context (recall, working memory, the just-
/// landed action result) to fill the bounded workspace. `FreshContext` is
/// untouched — a fresh ask still gets the fuller grounding (the safe default,
/// "when in doubt, ground more, never less").
///
/// Still INPUT-side only ([[no-hardcoded-heuristics-to-steer-cognition]]): it
/// curates which context the model attends to, never the output. And it uses the
/// existing, maintained `stable` semantic — not a brittle faculty-name list — so a
/// new standing-grounding source inherits the right behavior for free the moment
/// it marks itself stable.
pub struct SituationFocusArbiter {
    inner: SalienceArbiter,
}

impl SituationFocusArbiter {
    pub const fn new() -> Self {
        Self {
            inner: SalienceArbiter,
        }
    }
}

impl Default for SituationFocusArbiter {
    fn default() -> Self {
        Self::new()
    }
}

impl Arbiter for SituationFocusArbiter {
    fn focus(
        &self,
        candidates: Vec<Contribution>,
        capacity: usize,
        ctx: &FocusContext<'_>,
    ) -> Vec<Contribution> {
        let candidates = match ctx.situation {
            // Re-perceiving a tool result: drop the standing SOCIAL re-grounding so
            // the result + affordances + working memory own the window. The salience
            // top-k below then packs the lean, code-first context.
            Situation::PostAction => candidates
                .into_iter()
                .filter(|c| !c.stable)
                .collect::<Vec<_>>(),
            // Fresh ask: fuller grounding, ground more never less. Identical to the
            // bootstrap floor.
            Situation::FreshContext => candidates,
        };
        self.inner.focus(candidates, capacity, ctx)
    }
}

/// A full record of one workspace tick — the **mechanic's view of the mind.**
/// Captures every faculty bid *including the ones that LOST attention*, what won,
/// and the decision. This is why working on cognition is debuggable and fun:
/// replay a tick, see exactly why the mind did what it did ("why didn't recall
/// win?" — the loser bid + its salience + reasoning are right here), and run test
/// benches against recorded world-states. Per OBSERVABILITY-AS-SUBSTRATE.md,
/// capture is half the brain; per VDD ([[persona-record-replay-is-a-product-
/// requirement]]) knowing the exact inputs + competition beats any log.
#[derive(Debug, Clone)]
pub struct WorkspaceTrace {
    pub world_state: String,
    /// The room/context this tick reasoned within (contextId) — so a replayed
    /// trace correlates to the room it happened in, not a floating burst.
    pub room_id: Uuid,
    /// ALL bids this tick, winners and losers, across BOTH phases — the full
    /// competition (perception phase-1 context bids + deliberation phase-2 bids).
    pub bids: Vec<Contribution>,
    /// The **assembled context** the deliberation faculty saw — what won attention
    /// in phase 1 and was broadcast into phase 2. This is the glass-box answer to
    /// "what context did the decider actually have?" (the RAG it reasoned over).
    pub context_broadcast: Vec<Contribution>,
    /// The final broadcast: the assembled context PLUS the deliberation output.
    pub broadcast: Vec<Contribution>,
    /// The participation decision that emerged, if any.
    pub decision: Option<Decision>,
    /// Per-faculty wall-clock for THIS tick — every faculty across both phases,
    /// winners/losers/abstainers alike. The speed axis of the four-axis scoreboard
    /// and the dashboard's primary feed: it answers "where did this turn's latency
    /// actually go?" After deferral, the perception tier should read ~0µs and the
    /// deliberation LLM call should visibly dominate. See [`FacultyTiming`].
    pub timings: Vec<FacultyTiming>,
}

/// Sink for workspace traces — the replay/logging seam. Default is `Noop`
/// (zero hot-path cost); operators/test-benches swap in a recording sink.
/// Same pattern as `RagCaptureSink`.
pub trait WorkspaceCaptureSink: Send + Sync {
    fn record(&self, trace: &WorkspaceTrace);
}

/// Zero-cost default — drops traces on the floor.
pub struct NoopWorkspaceCaptureSink;
impl WorkspaceCaptureSink for NoopWorkspaceCaptureSink {
    fn record(&self, _trace: &WorkspaceTrace) {}
}

/// The persona's body for the act→observe motion: the HANDS that execute an
/// [`Decision::Act`], the HIPPOCAMPUS that remembers the result, and the
/// IDENTITY the action is performed as. Held on the [`WorkspaceCycle`] because
/// the cycle IS the persona's one mind — and per Joel, a persona (like a Claude
/// tab) is in MANY rooms at once, so the body is deliberately **room-agnostic**:
/// `room_id` flows per-act (the room *that* tick is about), never baked in here.
/// `None` on the cycle → a pure-cognition mind with no hands (harnesses, or any
/// persona whose spawn path built no executor): an `Act` verdict simply can't be
/// driven, and tools were never offered, so it never arises.
///
/// The act→observe driver ([`super::act_observe`]) reads this; `run_in_room`
/// never touches it (that stays a pure single tick).
pub struct ActingBody {
    pub persona_id: Uuid,
    pub persona_name: String,
    /// Runs the tool calls an `Act` verdict carries (identity-bearing, so the
    /// ACL gates what the persona may actually do).
    pub executor: Arc<dyn crate::cognition::tool_executor::ToolExecutor>,
    /// The hippocampus the action's RESULT is admitted into as an Episodic
    /// engram — so the outcome becomes a thing the mind remembers and can be
    /// reminded of next tick, the same way it carries every other fact.
    pub admission: Arc<crate::persona::admission_state::AdmissionState>,
    /// The volatile working-memory scratchpad the organism records each act into
    /// (proprioception). Distinct from `admission` (long-term, content-deduped): an
    /// act must be perceptible NEXT tick even when its result is a dedup no-op in
    /// long-term memory, so the mind sees its own hands and doesn't re-issue the
    /// identical act blind. Shared `Arc` with the perception-tier
    /// [`WorkingMemoryFaculty`] (one buffer, written here, read there).
    pub working_memory: Arc<crate::cognition::working_memory::WorkingMemory>,
}

/// One service-tick of cognition over a CONSOLIDATED burst (never per-event):
/// every faculty bids in parallel over the same world-state, the arbiter
/// integrates the bids into the bounded workspace, the workspace broadcasts.
/// The participation [`Decision`] is then read from the broadcast.
pub struct WorkspaceCycle {
    faculties: Vec<Arc<dyn Faculty>>,
    arbiter: Arc<dyn Arbiter>,
    /// Bound on how many contributions can hold the workspace at once — the
    /// finite "spotlight" of attention.
    capacity: usize,
    /// Replay/logging seam — every tick is recorded here. Default `Noop`
    /// (zero hot-path cost); swap in a recording sink to make the mind a
    /// glass box for debugging, tuning, and test benches.
    capture: Arc<dyn WorkspaceCaptureSink>,
    /// The persona's hands + hippocampus + identity for the act→observe driver.
    /// `None` → no hands (pure cognition). See [`ActingBody`].
    acting: Option<Arc<ActingBody>>,
    /// The persona's paged-in genome — the LoRA layers active for generation. The
    /// deliberation faculty shares this exact handle and reads it wait-free on
    /// every generation; [`page_in`](Self::page_in)/[`page_out`](Self::page_out)
    /// swap which gene is active (virtual memory for skill). Empty → base model.
    /// This is the seam `cognition/eval` A/Bs base vs a candidate gene over.
    genome: GenomeHandle,
    /// Shared decoding-temperature override ([`DecodingHandle`]) — the same
    /// `ArcSwap` the deliberation faculty reads wait-free on every generation.
    /// `None` (live cognition) → her configured warmth; the eval window
    /// ([`isolate_for_eval`]) flips it to `Some(0.0)` (greedy) so the reward metric
    /// is reproducible, and restores it on the guard's drop. Mirrors `genome`: one
    /// handle, two holders (cycle + faculty).
    decoding: DecodingHandle,
    /// The persona's live model binding — the served adapter + requested model +
    /// context window the deliberation faculty reads wait-free each generation. The
    /// faculty shares this EXACT handle; [`rebind_model`](Self::rebind_model) swaps
    /// it atomically when the served model changes (`serving/pin` or grid failover),
    /// carrying the genome + memory across untouched (no cycle rebuild). `None` →
    /// a pure-cognition / test cycle with no deliberation faculty to re-home (the
    /// re-home is then a benign no-op). Mirrors `genome`/`decoding`: one handle, two
    /// holders. See [`ModelBinding`] + [[seamless-persona-failover-model-and-genome]].
    model_binding: Option<ModelBindingHandle>,
    /// Monotonic service-tick counter — the source of each [`Workspace::cycle`]
    /// frame index. Interior-mutable because `run` takes `&self` (the living
    /// mind is shared, not owned per tick); bumped once per `run_in_room`.
    /// Starts at 0 so the first live cycle is `CycleId(1)` and `CycleId(0)`
    /// stays the UNSTAMPED sentinel.
    cycle_counter: std::sync::atomic::AtomicU64,
    /// #169 per-turn STREAMING sink — the output channel for the CURRENT turn's
    /// answer tokens, or `None` when this turn is not streamed (the default; every
    /// non-live caller and test). Interior-mutable (like `cycle_counter`) because
    /// `run_in_room_inner` takes `&self`: the live caller (service_loop) sets it
    /// `Some` just before a streamed turn and clears it after, and
    /// `run_in_room_inner` reads it into the per-turn [`Workspace::token_sink`]. A
    /// std `Mutex` — set/read is a cheap clone of a refcounted `Sender`, never held
    /// across an await. Safe: a persona's live turns are sequential, and eval runs
    /// on a SEPARATE forked cycle, so no cross-turn contention.
    token_sink: std::sync::Mutex<
        Option<tokio::sync::mpsc::UnboundedSender<crate::ai::adapter::GenerationChunk>>,
    >,
    /// #186 glass-box: the decaying per-axis "which faculty is firing" accumulator the
    /// vitals radiator samples (Focus/Reason/Recall/Act → the tile's live compass). The
    /// tick seam bumps it from the faculties this cycle already runs + times; the acting
    /// seam bumps Act. PURE OBSERVABILITY — no decision path ever reads it. `Arc` so the
    /// radiator can hold a cheap clone without the cycle lock. See [`FacultyPulse`].
    faculty_pulse: Arc<super::faculty_pulse::FacultyPulse>,
}

/// RAII guard for a memory-isolated measurement window over a cycle's
/// hippocampus — see [`WorkspaceCycle::isolate_for_eval`]. Holds the persona's
/// `AdmissionState`, the frame to rewind to, and the real persistence sink to
/// restore. All `Option` so a pure-cognition cycle (no hands) yields a benign
/// no-op guard. Not `Clone` — the restore must happen exactly once, on drop.
pub struct EvalIsolation {
    admission: Option<Arc<crate::persona::admission_state::AdmissionState>>,
    checkpoint: Option<crate::persona::admission_state::AdmissionCheckpoint>,
    real_sink:
        Option<Arc<dyn crate::persona::admission_persistence::AdmissionPersistenceSink>>,
    /// The shared decoding handle the guard forced to greedy on creation — restored
    /// to relaxed (`None`) on drop. Carried even for a no-hands (pure-cognition)
    /// cycle, because a reproducible metric needs deterministic generation whether
    /// or not she acts.
    decoding: Option<DecodingHandle>,
}

impl EvalIsolation {
    /// Rewind the persona's in-memory admission frame to the checkpoint taken
    /// when the guard was created — call BETWEEN A/B arms so the base and
    /// candidate arms start from identical memory (the only difference the lift
    /// measures is the genome, never accumulated engrams). No-op for a
    /// pure-cognition cycle.
    pub fn rewind(&self) {
        if let (Some(admission), Some(checkpoint)) = (&self.admission, &self.checkpoint) {
            admission.restore(checkpoint);
        }
    }
}

impl Drop for EvalIsolation {
    fn drop(&mut self) {
        // Restore relaxed decoding (her lived warmth) first — independent of hands,
        // so even a pure-cognition eval leaves her sampling normally afterward.
        if let Some(decoding) = &self.decoding {
            decoding.store(Arc::new(None));
        }
        let Some(admission) = &self.admission else { return };
        // Rewind the memory frame, THEN restore the real sink — order matters:
        // restoring the sink first could let a racing observe land a write the
        // rewind was meant to erase. With the sink still muted, the rewind is
        // the last word on what disk will ever see from this window.
        if let Some(checkpoint) = &self.checkpoint {
            admission.restore(checkpoint);
        }
        if let Some(sink) = self.real_sink.take() {
            admission.swap_persistence(sink);
        }
    }
}

/// One faculty's replayed bid: its output against a GIVEN workspace plus the
/// wall-clock it took. The factory's per-station reading — deterministic for the
/// same workspace + backend, so a one-variable workspace mutation isolates
/// exactly what moved the faculty's mind. `contribution == None` ⇒ the faculty
/// abstained this run.
#[derive(Debug, Clone)]
pub struct ReplayBid {
    pub faculty: FacultyId,
    pub contribution: Option<Contribution>,
    pub elapsed_us: u128,
}

/// Per-faculty wall-clock for ONE **live** tick — the live analog of
/// [`ReplayBid`]'s timing, captured for EVERY faculty (winners, losers, AND
/// abstainers) so the glass box shows where a turn's latency actually went.
/// This is the speed scoreboard at faculty granularity and the dashboard's
/// primary feed: after deferring recall + the grounding sources, every
/// perception faculty should read ~0µs (a warm last-good cache read) while the
/// deliberation LLM call visibly dominates — the proof, per tick, that the only
/// thing left on the critical path is inference. An abstaining faculty is still
/// timed: a slow abstainer is exactly the latency you need to see.
#[derive(Debug, Clone)]
pub struct FacultyTiming {
    pub faculty: FacultyId,
    /// Wall-clock this faculty's `contribute()` took on the live concurrent
    /// barrier — includes scheduling waits, which is honest: it's the latency the
    /// barrier actually saw, not an isolated micro-benchmark.
    pub elapsed_us: u128,
    /// `false` = perception tier (phase 1); `true` = deliberation tier (phase 2).
    pub deliberation: bool,
    /// Whether the faculty produced a [`Contribution`] (vs abstained).
    pub bid: bool,
}

impl WorkspaceCycle {
    pub fn new(
        faculties: Vec<Arc<dyn Faculty>>,
        arbiter: Arc<dyn Arbiter>,
        capacity: usize,
    ) -> Self {
        Self {
            faculties,
            arbiter,
            capacity: capacity.max(1),
            capture: Arc::new(NoopWorkspaceCaptureSink),
            acting: None,
            genome: empty_genome(),
            decoding: relaxed_decoding(),
            model_binding: None,
            cycle_counter: std::sync::atomic::AtomicU64::new(0),
            token_sink: std::sync::Mutex::new(None),
            faculty_pulse: Arc::new(super::faculty_pulse::FacultyPulse::new()),
        }
    }

    /// The live cognition-compass accumulator (#186). The tick seam + acting seam bump
    /// it; the vitals radiator samples [`FacultyPulse::levels`]. `Arc` clone is cheap.
    pub fn faculty_pulse(&self) -> Arc<super::faculty_pulse::FacultyPulse> {
        self.faculty_pulse.clone()
    }

    /// #186 glass-box tap: record that a faculty CONTRIBUTED this tick on the live
    /// compass, brightness scaled by its salience (a strong recall lights brighter than
    /// a weak one) but floored so any real firing is visibly lit. Maps the internal
    /// faculty → its display axis; a faculty with no compass home (Affect/Volition/
    /// Salience) is a silent no-op. PURE OBSERVABILITY — called from the tick after a
    /// bid, never reads back into a decision.
    fn note_faculty_firing(&self, faculty: &FacultyId, salience: f32) {
        if let Some(axis) = super::faculty_pulse::CognitionAxis::of(faculty) {
            // 35..=100: a bid always lights (≥35), salience carries the rest.
            let level = (35.0 + salience.clamp(0.0, 1.0) * 65.0).round() as u8;
            self.faculty_pulse.note(axis, level);
        }
    }

    /// #186 glass-box: fire the Act axis on the live compass — the hands moving, a tool
    /// actually executing. Called from the acting seam ([`super::act_observe::apply_act`]);
    /// Act has no `FacultyId` (the hands run AFTER deliberation, not as a workspace
    /// faculty), so it is bumped explicitly rather than through the tick map.
    pub fn note_acting(&self) {
        self.faculty_pulse.fire(super::faculty_pulse::CognitionAxis::Act);
    }

    /// Share the persona's decoding handle — call with the SAME [`DecodingHandle`]
    /// passed to [`LlmDeliberationFaculty::with_decoding`] so the eval window's
    /// greedy flip takes effect on the faculty's next generation.
    pub fn with_decoding(mut self, decoding: DecodingHandle) -> Self {
        self.decoding = decoding;
        self
    }

    /// Share the genome handle the deliberation faculty reads — call with the SAME
    /// [`GenomeHandle`] passed to [`LlmDeliberationFaculty::with_genome`] so a
    /// page-in on the cycle takes effect on the faculty's next generation.
    pub fn with_genome(mut self, genome: GenomeHandle) -> Self {
        self.genome = genome;
        self
    }

    /// Share the model binding the deliberation faculty reads — call with the SAME
    /// [`ModelBindingHandle`] passed to
    /// [`LlmDeliberationFaculty::with_model_binding`](super::llm_deliberation_faculty::LlmDeliberationFaculty::with_model_binding)
    /// so a [`rebind_model`](Self::rebind_model) on the cycle takes effect on the
    /// faculty's next generation. Without this call the cycle has no binding to
    /// re-home (a pure-cognition / test cycle), and `rebind_model` is a no-op.
    pub fn with_model_binding(mut self, binding: ModelBindingHandle) -> Self {
        self.model_binding = Some(binding);
        self
    }

    /// Re-home the persona's deliberation onto a newly served model — swap the
    /// {adapter, model, context window} triple ATOMICALLY into the shared binding
    /// the faculty reads. The genome, working memory, admission, and hippocampus are
    /// UNTOUCHED (no cycle rebuild): the SAME continuous mind now deliberates through
    /// the new model, so this is model-load-as-paging (frequent, on grid demand —
    /// the base-model sibling of [`page_in`](Self::page_in)'s LoRA paging), not a
    /// teardown. Wait-free swap; the faculty's next generation sees it. No-op for a
    /// cycle with no deliberation faculty (no binding was shared in). See
    /// [`ModelBinding`] + [[seamless-persona-failover-model-and-genome]].
    pub fn rebind_model(&self, binding: ModelBinding) {
        if let Some(handle) = &self.model_binding {
            handle.store(Arc::new(binding));
        }
    }

    /// #169: set the per-turn STREAMING sink for the NEXT turn(s). The live caller
    /// (service_loop) sets `Some(tx)` just before a streamed turn and `None` after,
    /// so `run_in_room_inner` reads it into that turn's [`Workspace::token_sink`].
    /// `&self` (interior-mutable, like [`rebind_model`](Self::rebind_model)) — the
    /// living mind is shared, not owned per tick.
    pub fn set_token_sink(
        &self,
        sink: Option<tokio::sync::mpsc::UnboundedSender<crate::ai::adapter::GenerationChunk>>,
    ) {
        *self.token_sink.lock().unwrap() = sink;
    }

    /// The per-turn streaming sink to hand this tick's Workspace, or `None`.
    fn current_token_sink(
        &self,
    ) -> Option<tokio::sync::mpsc::UnboundedSender<crate::ai::adapter::GenerationChunk>> {
        self.token_sink.lock().unwrap().clone()
    }

    /// The inference adapter this mind CURRENTLY deliberates through — read from
    /// the same shared binding [`rebind_model`](Self::rebind_model) writes, so it
    /// tracks re-homes. `None` for a cycle with no deliberation faculty. The seam
    /// background regions (dream consolidation, wanderers) resolve their adapter
    /// from, instead of a spawn-time snapshot that would go stale on failover.
    pub fn current_adapter(&self) -> Option<Arc<dyn crate::ai::adapter::AIProviderAdapter>> {
        self.model_binding
            .as_ref()
            .map(|handle| handle.load().adapter.clone())
    }

    /// The adapter AND the served-model id it must be asked for, as one read of
    /// the shared binding. Background regions must send BOTH: the model id
    /// selects the route/template on the serving side, and omitting it produced
    /// degenerate role-token runaway on the dream's first live pass
    /// (2026-07-12) while normal turns — which send `binding.model` — were
    /// clean through the same adapter.
    pub fn current_model_route(
        &self,
    ) -> Option<(Arc<dyn crate::ai::adapter::AIProviderAdapter>, Option<String>)> {
        self.model_binding.as_ref().map(|handle| {
            let b = handle.load();
            (b.adapter.clone(), b.model.clone())
        })
    }

    /// The served-model id + EFFECTIVE context window, as ONE atomic read of the
    /// shared binding — the display inputs a roster LOADOUT projects (`model ·
    /// ctx`). `None` for a cycle with no deliberation faculty (no binding to
    /// report). The parameter COUNT is deliberately NOT here: it isn't on the
    /// binding, and the caller resolves it from the model registry by this id
    /// (honest-absent when the row is unhydrated) — the cognition layer must not
    /// take a `model_registry` dependency to hand a display fact to the projector.
    /// Reading both in one `load()` avoids a torn `(model, window)` across a
    /// concurrent [`rebind_model`](Self::rebind_model).
    pub fn model_loadout(&self) -> Option<(Option<String>, u32)> {
        self.model_binding.as_ref().map(|handle| {
            let b = handle.load();
            (b.model.clone(), b.context_window)
        })
    }

    /// Page a gene (set of LoRA layers) into the persona's genome — the next
    /// generation runs the base model adapted by these layers. Wait-free swap.
    /// This is the measured page-in: the genome loop pages in a freshly forged
    /// gene here and `cognition/eval` measures the lift it produced.
    pub fn page_in(&self, adapters: Vec<ActiveAdapterRequest>) {
        self.genome.store(Arc::new(adapters));
    }

    /// Page out all genes — the persona reverts to the base model (no LoRA). The
    /// baseline arm of an A/B, and the clean state to leave a persona in.
    pub fn page_out(&self) {
        self.genome.store(Arc::new(Vec::new()));
    }

    /// The persona's currently paged-in genome (a snapshot).
    pub fn genome(&self) -> Vec<ActiveAdapterRequest> {
        self.genome.load().as_ref().clone()
    }

    /// This mind's monotonic service-tick count — how many `Workspace::cycle`
    /// ticks it has serviced since spawn. The DELTA of this over an interval is
    /// the persona's live "thinking tempo" (the roster **ACT** vital): a rising
    /// count = actively servicing concerns, a flat count = idle. Read wait-free.
    pub fn cycle_count(&self) -> u64 {
        self.cycle_counter.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Clear the volatile working-memory scratch (the act/reasoning proprioception
    /// buffer), if this cycle has hands. Called at the boundary between disjoint
    /// concerns — e.g. each independent task in a `cognition/eval` pass — so one
    /// concern's proprioception does not bleed into the next. No-op for a
    /// pure-cognition cycle. See [`super::working_memory::WorkingMemory::clear`].
    pub fn reset_working_memory(&self) {
        if let Some(acting) = &self.acting {
            acting.working_memory.clear();
        }
    }

    /// Begin a memory-isolated measurement window over this cycle's hippocampus.
    ///
    /// `cognition/eval` drives the persona's REAL admission as it grades her, so
    /// the act-observations a run admits would otherwise (1) drift her absolute
    /// score run-to-run, (2) order-bias a paired A/B (the second arm inherits the
    /// first arm's writes), and (3) pollute her durable sqlite. While the returned
    /// guard is alive, admission STILL fires — the measured memory motion is
    /// identical to a real turn, which is what keeps the measurement valid — but
    /// the persistence sink is muted (nothing reaches disk) and the in-memory
    /// admission frame is checkpointed. Call [`EvalIsolation::rewind`] between A/B
    /// arms so both start from identical memory; dropping the guard restores the
    /// memory and the real sink. A pure-cognition cycle (no hands → nothing is
    /// admitted) yields a no-op guard. See
    /// [[eval-mutates-persona-lift-needs-isolation]].
    pub fn isolate_for_eval(&self) -> EvalIsolation {
        // Force greedy decoding for the WHOLE eval window — before the no-hands
        // early return, because a reproducible metric needs deterministic
        // generation even for a pure-speak (no-tools) eval. Restored on drop.
        self.decoding.store(Arc::new(Some(0.0)));
        let decoding = Some(Arc::clone(&self.decoding));

        let Some(acting) = &self.acting else {
            return EvalIsolation {
                admission: None,
                checkpoint: None,
                real_sink: None,
                decoding,
            };
        };
        let admission = acting.admission.clone();
        let checkpoint = admission.checkpoint();
        let real_sink = admission
            .swap_persistence(crate::persona::admission_persistence::NoopSink::arc());
        EvalIsolation {
            admission: Some(admission),
            checkpoint: Some(checkpoint),
            real_sink: Some(real_sink),
            decoding,
        }
    }

    /// Install a capture sink (recording / on-disk replay / in-flight inspection).
    pub fn with_capture(mut self, capture: Arc<dyn WorkspaceCaptureSink>) -> Self {
        self.capture = capture;
        self
    }

    /// Give this mind a body — the hands + hippocampus + identity the act→observe
    /// driver uses to execute an [`Decision::Act`], remember its result, and
    /// re-perceive. Without it the cycle is pure cognition (no acting). See
    /// [`ActingBody`].
    pub fn with_acting(mut self, body: Arc<ActingBody>) -> Self {
        self.acting = Some(body);
        self
    }

    /// The persona's body, if it has hands. The act→observe driver reads this;
    /// `None` → pure-cognition mind (no tools were offered, so no `Act` arises).
    pub fn acting(&self) -> Option<&Arc<ActingBody>> {
        self.acting.as_ref()
    }

    /// Run one cognition tick over the consolidated `world_state`, recording the
    /// full trace (every bid incl. losers, what won, the decision) for replay.
    ///
    /// **Staged assembly** (cbar lines→planes / GWT broadcast-then-rebid):
    /// 1. **Perception phase** — faculties with `reacts_to_broadcast() == false`
    ///    bid in parallel over the raw world-state (broadcast still empty). The
    ///    arbiter routes the salient subset into the broadcast: this is the
    ///    *assembled context* (the "RAG" the decider will read).
    /// 2. **Deliberation phase** — faculties with `reacts_to_broadcast() == true`
    ///    bid in parallel over the workspace whose broadcast now holds the
    ///    assembled context, so the [`Decision`] is conditioned on what recall /
    ///    world-model / affect actually surfaced. Their bids append to the
    ///    broadcast.
    ///
    /// This is what makes "pull relevant memory, *then* decide" real: the decider
    /// is never blind to recall. Still one tick over the consolidated burst, still
    /// `O(capacity)` for the bounded context — no per-event slowdown.
    pub async fn run(&self, burst: impl Into<Burst>) -> Workspace {
        self.run_in_room(burst, Uuid::nil()).await
    }

    /// Same as [`run_in_room`](Self::run_in_room) but with explicit
    /// [`TurnFraming`] — the live persona path passes `directed`/`self_initiated`
    /// here so the deliberation faculty's system prompt reflects whether a question
    /// was put TO her (suppress the silence escape) and whether this is her own
    /// heartbeat. `run_in_room` is the ambient shorthand.
    pub async fn run_framed(
        &self,
        burst: impl Into<Burst>,
        room_id: Uuid,
        framing: TurnFraming,
    ) -> Workspace {
        // Fresh-context default: a bare framed tick is a fresh ask (fuller
        // grounding). The act→observe driver calls [`run_situated`] with
        // `PostAction` on re-perception ticks.
        self.run_in_room_inner(burst, room_id, framing, Situation::FreshContext)
            .await
    }

    /// Same as [`run_framed`](Self::run_framed) but with the tick's [`Situation`]
    /// threaded explicitly — the seam the act→observe loop uses to tell the focuser
    /// "this tick re-perceives a tool result" (`PostAction`) vs "fresh ask"
    /// (`FreshContext`). The situation is a TYPED signal carried from the loop,
    /// never inferred from the burst text ([[no-hardcoded-heuristics-to-steer-
    /// cognition]]).
    pub async fn run_situated(
        &self,
        burst: impl Into<Burst>,
        room_id: Uuid,
        framing: TurnFraming,
        situation: Situation,
    ) -> Workspace {
        self.run_in_room_inner(burst, room_id, framing, situation)
            .await
    }

    /// Same as [`run`](Self::run) but scoped to a room/context (the contextId the
    /// turn acts within). The live persona path uses THIS so the deliberation
    /// faculty stamps tool calls with the real room — `run` is the nil-room
    /// shorthand for tests that aren't room-scoped.
    pub async fn run_in_room(&self, burst: impl Into<Burst>, room_id: Uuid) -> Workspace {
        // Ambient default: silence stays first-class, message-driven. A turn put TO
        // the persona, or her own heartbeat, uses [`run_framed`](Self::run_framed)
        // with the appropriate [`TurnFraming`].
        self.run_in_room_inner(burst, room_id, TurnFraming::ambient(), Situation::FreshContext)
            .await
    }

    /// The full cognitive tick. [`TurnFraming`] is set on the [`Workspace`] so the
    /// deliberation faculty knows whether to offer the silence escape (see
    /// [`Workspace::directed_at_self`]) and whether to frame the turn as the
    /// persona's own time (see [`Workspace::self_initiated`]); everything else is
    /// identical across framings — framing only reshapes the system prompt.
    async fn run_in_room_inner(
        &self,
        burst: impl Into<Burst>,
        room_id: Uuid,
        framing: TurnFraming,
        situation: Situation,
    ) -> Workspace {
        // Bump the service tick: this workspace IS cycle N, and every finding
        // collected against it is stamped N (the cbar `frameIndex`). 1-based so
        // `CycleId(0)` stays the UNSTAMPED sentinel — the seam a deferred faculty
        // later reads to know how stale its own finding is.
        let cycle = CycleId(
            self.cycle_counter
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                .wrapping_add(1),
        );
        // Carry the structured burst straight through: `in_room` splits it into the
        // canonical `turns` (the deliberation faculty's role-attribution source) and
        // the `world_state` text projection (every other reader). Framing reshapes
        // only the system prompt — it never touches the conversation turns.
        let mut ws = Workspace::in_room(burst, room_id)
            .with_cycle(cycle)
            .directed(framing.directed)
            .self_initiated(framing.self_initiated)
            // #169: hand this turn the live streaming sink if the caller set one
            // (service_loop, just before a streamed Speak); `None` otherwise.
            .with_token_sink(self.current_token_sink());

        // --- Phase 1: perception. Context faculties react to the raw world-state. ---
        let perception: Vec<&Arc<dyn Faculty>> = self
            .faculties
            .iter()
            .filter(|f| !f.reacts_to_broadcast())
            .collect();
        // Time every faculty individually on the live concurrent barrier — winners,
        // losers, AND abstainers — so the glass box / dashboard sees exactly where a
        // tick's latency went. `let ws = &ws` rebinds to a shared reference so each
        // `async move` future copies the POINTER (concurrent immutable borrow), never
        // moving the Workspace.
        let mut timings: Vec<FacultyTiming> = Vec::with_capacity(self.faculties.len());
        let perception_timed: Vec<(FacultyId, u128, Option<Contribution>)> =
            join_all(perception.iter().map(|f| {
                let f = *f;
                let ws = &ws;
                async move {
                    let id = f.id();
                    let t0 = std::time::Instant::now();
                    let bid = f.contribute(ws).await;
                    (id, t0.elapsed().as_micros(), bid)
                }
            }))
            .await;
        let mut context_bids: Vec<Contribution> = Vec::with_capacity(perception_timed.len());
        for (id, us, bid) in perception_timed {
            // #186 compass: a perception faculty that surfaced something FIRES its axis
            // (Recall→recall, WorldModel→focus), scaled by salience.
            if let Some(c) = &bid {
                self.note_faculty_firing(&id, c.salience);
            }
            timings.push(FacultyTiming {
                faculty: id,
                elapsed_us: us,
                deliberation: false,
                bid: bid.is_some(),
            });
            if let Some(c) = bid {
                context_bids.push(c);
            }
        }
        // Stamp each finding with the cycle it reasoned against. Immediate faculties
        // all saw THIS workspace, so the stamp is `ws.cycle`; a future deferred lane
        // will stamp its own (older) cycle and reconcile forward.
        for bid in &mut context_bids {
            bid.cycle = cycle;
        }
        // Route the salient subset into the bounded workspace — the arbiter is the
        // attention/FOCUS layer over information flow, not a gate. The winners are
        // the assembled context the deliberation faculty reasons over. The focuser
        // is handed the REAL situation the tick is in — threaded from the act→observe
        // loop (`PostAction` on every re-perception of a tool result, `FreshContext`
        // on a fresh ask), NEVER inferred by reading the burst text back. A
        // situation-aware arbiter ([`SituationFocusArbiter`]) uses it to streamline
        // context FOR the ask (drop standing re-grounding post-action); the bootstrap
        // [`SalienceArbiter`] ignores it.
        let focus_ctx = FocusContext {
            world_state: &ws.world_state,
            situation,
        };
        let focused = self
            .arbiter
            .focus(context_bids.clone(), self.capacity, &focus_ctx);
        // Bids-vs-focused receipt at the attention seam. Together with
        // `delib.context.render` (received-vs-rendered) this closes the glass box
        // over a surfaced finding's whole path: faculty bid → attention →
        // prompt. The silver-harbor failure (recall surfaced at z=5.5σ, [recall]
        // absent from the prompt) was unattributable because BOTH seams were
        // dark (#130).
        crate::probe!(
            class = "workspace.attention.focus",
            capacity = self.capacity,
            // The situation that drove this focus — the stat that makes context
            // control learnable: a focuser policy (this bootstrap, or a later
            // learned one) is a function of situation → dropped grounding, and the
            // bids-vs-focused delta below is the reward signal for it.
            situation = ?situation,
            bids = context_bids.len(),
            focused = focused.len(),
            kept = %focused
                .iter()
                .map(|c| format!("{}(sal={:.2})", c.faculty.as_str(), c.salience))
                .collect::<Vec<_>>()
                .join(","),
            evicted = %context_bids
                .iter()
                .filter(|b| !focused.iter().any(|f| f.faculty == b.faculty))
                .map(|b| format!("{}(sal={:.2})", b.faculty.as_str(), b.salience))
                .collect::<Vec<_>>()
                .join(","),
            "attention: {}/{} bids focused",
            focused.len(),
            context_bids.len()
        );
        ws.broadcast = focused;
        let context_broadcast = ws.broadcast.clone();

        // --- Phase 2: deliberation. Reacts to the assembled broadcast (it can now
        // see what recall/world-model/affect surfaced) and emits the verdict. ---
        let deliberation: Vec<&Arc<dyn Faculty>> = self
            .faculties
            .iter()
            .filter(|f| f.reacts_to_broadcast())
            .collect();
        let deliberation_timed: Vec<(FacultyId, u128, Option<Contribution>)> =
            join_all(deliberation.iter().map(|f| {
                let f = *f;
                let ws = &ws;
                async move {
                    let id = f.id();
                    let t0 = std::time::Instant::now();
                    let bid = f.contribute(ws).await;
                    (id, t0.elapsed().as_micros(), bid)
                }
            }))
            .await;
        let mut decision_bids: Vec<Contribution> = Vec::with_capacity(deliberation_timed.len());
        for (id, us, bid) in deliberation_timed {
            // #186 compass: the reasoner (Deliberation→reason) FIRES when it emits a
            // verdict — the bright Reason phase you watch while she thinks.
            if let Some(c) = &bid {
                self.note_faculty_firing(&id, c.salience);
            }
            timings.push(FacultyTiming {
                faculty: id,
                elapsed_us: us,
                deliberation: true,
                bid: bid.is_some(),
            });
            if let Some(c) = bid {
                decision_bids.push(c);
            }
        }
        for bid in &mut decision_bids {
            bid.cycle = cycle;
        }
        // The deliberation output is the RESULT of attending to the context, not a
        // competitor for the bounded context spotlight — append it to the broadcast.
        ws.broadcast.extend(decision_bids.iter().cloned());

        // Capture the FULL competition: every bid across both phases (incl. the
        // losers you need to debug "why didn't X win?"), the assembled context the
        // decider saw, the final broadcast, and the decision.
        let mut all_bids = context_bids;
        all_bids.extend(decision_bids);
        self.capture.record(&WorkspaceTrace {
            world_state: ws.world_state.clone(),
            room_id: ws.room_id,
            bids: all_bids,
            context_broadcast,
            broadcast: ws.broadcast.clone(),
            decision: ws.decision().cloned(),
            timings,
        });
        ws
    }

    /// **Factory seam — isolate and repeat one cognition phase.** Re-run faculties
    /// against a GIVEN [`Workspace`] (reconstructed from a capture, or hand-built)
    /// instead of assembling one live. `only = Some(id)` isolates a single faculty
    /// ("what did *recall* surface for this burst?"); `None` re-runs every faculty
    /// over the same `ws`. Each bid is timed individually — faculties run
    /// sequentially here precisely so per-faculty wall-clock is attributable; this
    /// is the measurement path, not the live concurrent cycle. Deterministic for
    /// the same `ws` + the same faculty backends, so mutating ONE field of `ws`
    /// between two calls isolates its causal effect on a faculty's output. This is
    /// the glass box that makes a turn repeatable instead of a guess.
    ///
    /// Phase honesty is the caller's contract: perception faculties read
    /// `ws.world_state` with an empty `ws.broadcast`; the deliberation faculty
    /// reads `ws.broadcast` (the assembled context). Build `ws` to match the
    /// station you are probing — `replay` runs the faculty exactly as the live
    /// cycle would, it does not re-stage the phases for you.
    pub async fn replay(&self, ws: &Workspace, only: Option<&FacultyId>) -> Vec<ReplayBid> {
        let mut out = Vec::new();
        for f in self
            .faculties
            .iter()
            .filter(|f| only.map_or(true, |id| &f.id() == id))
        {
            let t0 = std::time::Instant::now();
            let contribution = f.contribute(ws).await;
            out.push(ReplayBid {
                faculty: f.id(),
                contribution,
                elapsed_us: t0.elapsed().as_micros(),
            });
        }
        out
    }

    /// Whether the faculty with this id reads the assembled `broadcast` (a
    /// deliberation-tier faculty) rather than raw `world_state`. `None` if no
    /// such faculty is in this cycle. `cognition/replay` uses this to REFUSE
    /// replaying a broadcast-reading faculty against an un-reconstructed (empty)
    /// broadcast — a blind run would produce a confident-but-wrong verdict, the
    /// silent lie the no-fallback doctrine forbids.
    pub fn reacts_to_broadcast(&self, id: &FacultyId) -> Option<bool> {
        self.faculties
            .iter()
            .find(|f| &f.id() == id)
            .map(|f| f.reacts_to_broadcast())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A canned faculty for tests — fixed contribution + salience.
    struct FixedFaculty(Contribution);
    #[async_trait]
    impl Faculty for FixedFaculty {
        fn id(&self) -> FacultyId {
            self.0.faculty.clone()
        }
        async fn contribute(&self, _ws: &Workspace) -> Option<Contribution> {
            Some(self.0.clone())
        }
    }

    /// A faculty that abstains this tick.
    struct AbstainFaculty(FacultyId);
    #[async_trait]
    impl Faculty for AbstainFaculty {
        fn id(&self) -> FacultyId {
            self.0.clone()
        }
        async fn contribute(&self, _ws: &Workspace) -> Option<Contribution> {
            None
        }
    }

    /// A deliberation faculty that CONDITIONS its decision on the assembled
    /// broadcast: it speaks (referencing what it saw) only if recall surfaced
    /// something; otherwise it passes. This is the probe for staged assembly —
    /// under the old single-pass join_all it would always see an empty broadcast
    /// and could never be informed by recall.
    struct ConditionalDeliberation;
    #[async_trait]
    impl Faculty for ConditionalDeliberation {
        fn id(&self) -> FacultyId {
            FacultyId::Deliberation
        }
        fn reacts_to_broadcast(&self) -> bool {
            true
        }
        async fn contribute(&self, ws: &Workspace) -> Option<Contribution> {
            // Look at the assembled context that won attention in phase 1.
            let recalled = ws.broadcast.iter().find(|c| c.faculty == FacultyId::Recall);
            match recalled {
                Some(mem) => Some(Contribution::verdict(
                    Decision::Speak {
                        text: format!("informed by recall: {}", mem.content),
                    },
                    0.9,
                    "conditioned the reply on the recalled context",
                )),
                None => Some(Contribution::verdict(
                    Decision::Pass,
                    0.5,
                    "blind — no context in the broadcast",
                )),
            }
        }
    }

    fn cycle(faculties: Vec<Arc<dyn Faculty>>, capacity: usize) -> WorkspaceCycle {
        WorkspaceCycle::new(faculties, Arc::new(SalienceArbiter), capacity)
    }

    // what this catches: the cycle's genome page-in/page-out round-trips through the
    // shared handle — page_in publishes a gene the next generation reads, page_out
    // reverts to the clean base. This is the A/B's lever (eval pages a gene in/out
    // around two passes); if it didn't round-trip, the candidate and base arms
    // would measure the same genome and every lift would read as zero.
    #[test]
    fn genome_page_in_and_out_round_trips() {
        let c = cycle(vec![], 4);
        assert!(c.genome().is_empty(), "a fresh cycle starts on the base model");

        c.page_in(vec![ActiveAdapterRequest {
            name: "coder-0p5b".to_string(),
            path: "/genes/coder.gguf".to_string(),
            domain: String::new(),
            scale: 0.8,
        }]);
        let paged = c.genome();
        assert_eq!(paged.len(), 1, "the gene is now the active genome");
        assert_eq!(paged[0].name, "coder-0p5b");
        assert_eq!(paged[0].scale, 0.8);

        c.page_out();
        assert!(c.genome().is_empty(), "page_out reverts to the clean base");
    }

    // what this catches: the #186 acting tap lights the Act axis on the live cognition
    // compass THROUGH the cycle's public API — the exact path `act_observe::apply_act`
    // drives when a real tool batch executes. A resting cycle's compass is dark; a
    // `note_acting()` lights ONLY Act (idx 3 in the Focus/Reason/Recall/Act order the
    // radiator samples). If the tap regressed (wrong axis, or the pulse field dropped)
    // the tile's Act corner would never light while she runs tools.
    #[test]
    fn acting_lights_only_the_act_axis_on_the_compass() {
        let c = cycle(vec![], 4);
        assert_eq!(
            c.faculty_pulse().levels(),
            [0, 0, 0, 0],
            "a resting cycle's compass is dark"
        );
        c.note_acting();
        let levels = c.faculty_pulse().levels();
        assert!(levels[3] > 0, "acting lights the Act axis");
        assert_eq!(levels[0] + levels[1] + levels[2], 0, "and nothing else");
    }

    // what this catches: `rebind_model` writes THROUGH to the SAME shared binding
    // handle the deliberation faculty reads — the exact seam the served-model
    // re-home reconciler drives (ipc/mod.rs `re_home_all`). A live persona's model
    // page must swap the {adapter, model, context_window} triple the faculty will
    // budget its NEXT turn against; if the cycle held a separate handle from the
    // faculty, the swap would land on nothing and every re-home would be a silent
    // no-op (the multi-model sweep would keep answering as the boot-bound brain).
    // Also asserts the no-faculty cycle's `rebind_model` is a benign no-op (a
    // pure-cognition / test cycle shared no binding in).
    #[test]
    fn rebind_model_writes_through_the_shared_binding() {
        use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
        use crate::cognition::llm_deliberation_faculty::{model_binding, ModelBinding};

        let adapter: Arc<dyn crate::ai::adapter::AIProviderAdapter> =
            Arc::new(HeuristicInferenceAdapter::new());
        // The handle the faculty would hold; the cycle shares the SAME Arc.
        let handle = model_binding(Arc::clone(&adapter), None, 4096);
        let c = cycle(vec![], 4).with_model_binding(Arc::clone(&handle));

        // Baseline: what boot bound.
        assert_eq!(handle.load().context_window, 4096);
        assert!(handle.load().model.is_none());

        // A served-model change re-homes onto a new model + served window.
        c.rebind_model(ModelBinding {
            adapter: Arc::clone(&adapter),
            model: Some("qwen-coder-14b".to_string()),
            context_window: 8192,
        });

        // The faculty (holding the SAME handle) now sees the new binding.
        assert_eq!(
            handle.load().context_window,
            8192,
            "rebind must swap the served window through the shared handle"
        );
        assert_eq!(
            handle.load().model.as_deref(),
            Some("qwen-coder-14b"),
            "rebind must swap the model id through the shared handle"
        );

        // A cycle that shared no binding in (no deliberation faculty) must not
        // panic on re-home — it has nothing to rebind.
        let bare = cycle(vec![], 4);
        bare.rebind_model(ModelBinding {
            adapter,
            model: Some("whatever".to_string()),
            context_window: 2048,
        });
    }

    // what this catches: every finding is stamped with the cycle it was computed
    // against (the cbar frameIndex) — the decoupling precondition. Without the
    // stamp a late/deferred finding can't know how stale it is, so the arbiter
    // can't merge or reproject it correctly. Asserts a fresh construction is
    // UNSTAMPED, both phases inherit `ws.cycle`, and the counter advances per tick
    // (so two findings from different ticks are never confused for one moment).
    #[tokio::test]
    async fn contributions_carry_the_cycle_they_were_born_in() {
        // A fresh contribution is UNSTAMPED until a live cycle stamps it.
        let raw = Contribution::context(FacultyId::Recall, "engram", 0.8, "r");
        assert_eq!(raw.cycle, CycleId::UNSTAMPED);

        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::Recall,
                "engram: prior context",
                0.8,
                "perception finding",
            ))),
            Arc::new(ConditionalDeliberation),
        ];
        let c = cycle(faculties, 4);

        // First tick → CycleId(1); both the perception (Recall) and deliberation
        // (verdict) findings must carry it.
        let ws1 = c.run("burst one").await;
        assert_eq!(ws1.cycle, CycleId(1), "first live cycle is 1, not the 0 sentinel");
        assert!(ws1.broadcast.len() >= 2, "both faculties contributed this tick");
        for bid in &ws1.broadcast {
            assert_eq!(
                bid.cycle,
                CycleId(1),
                "finding from {:?} must be stamped with the cycle it reasoned against",
                bid.faculty
            );
        }

        // Second tick → the counter advances to CycleId(2): findings from
        // different ticks can never be mistaken for the same moment.
        let ws2 = c.run("burst two").await;
        assert_eq!(ws2.cycle, CycleId(2), "the service tick advances each run");
        for bid in &ws2.broadcast {
            assert_eq!(bid.cycle, CycleId(2));
        }
    }

    // what this catches: the factory's faculty-isolation seam (#14 ReplayFaculty).
    // `replay(ws, Some(id))` must re-run EXACTLY that faculty against the given
    // workspace, return its contribution, and stamp a timing — never leak other
    // faculties; `replay(ws, None)` runs them all. If isolation leaks or timing is
    // dropped, the glass box can't answer "what did recall surface for this burst?"
    // in isolation, and the harness degrades back to replaying only the final call.
    #[tokio::test]
    async fn replay_isolates_one_faculty_and_times_it() {
        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::Recall,
                "engram: the auth migration broke the deploy",
                0.8,
                "relevant past engram",
            ))),
            Arc::new(ConditionalDeliberation),
        ];
        let c = cycle(faculties, 4);
        let ws = Workspace::new("teammate: the deploy is red, what happened?");

        // isolate recall: exactly one bid, its engram content, a real timing.
        let recall_only = c.replay(&ws, Some(&FacultyId::Recall)).await;
        assert_eq!(recall_only.len(), 1, "only the requested faculty runs");
        assert_eq!(recall_only[0].faculty, FacultyId::Recall);
        assert!(recall_only[0]
            .contribution
            .as_ref()
            .expect("recall bid present")
            .content
            .contains("auth migration"));

        // no filter: every faculty re-runs over the same workspace.
        let all = c.replay(&ws, None).await;
        assert_eq!(all.len(), 2, "None replays every faculty");
    }

    // what this catches: `from_kebab` is the SINGLE inverse of `as_str` — if a
    // new FacultyId variant is added to `as_str` but not `from_kebab`, its tag
    // would silently degrade to `Custom`, splitting the one-place mapping. This
    // round-trip over every non-Custom variant fails loud on that drift.
    #[test]
    fn faculty_id_kebab_round_trips_every_variant() {
        for id in [
            FacultyId::Recall,
            FacultyId::WorldModel,
            FacultyId::Affect,
            FacultyId::Volition,
            FacultyId::Deliberation,
            FacultyId::Salience,
        ] {
            let tag = id.as_str();
            assert_eq!(
                FacultyId::from_kebab(tag),
                id,
                "tag '{tag}' must round-trip; from_kebab drifted from as_str"
            );
        }
        // Custom tags pass through untouched (sentinel-forged faculties).
        assert_eq!(
            FacultyId::from_kebab("sentinel-x"),
            FacultyId::Custom("sentinel-x".to_string())
        );
    }

    // what this catches: attention is a competition over ML-derived salience —
    // the highest-salience faculty bids win the bounded workspace. This is the
    // ML-integration replacement for calculate_priority's hand-weights.
    #[tokio::test]
    async fn arbiter_selects_top_k_by_ml_salience() {
        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::Recall,
                "low",
                0.2,
                "weak recall",
            ))),
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::WorldModel,
                "high",
                0.9,
                "strong signal",
            ))),
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::Affect,
                "mid",
                0.5,
                "some arousal",
            ))),
        ];
        let ws = cycle(faculties, 2).run("the consolidated thread").await;
        assert_eq!(ws.broadcast.len(), 2, "bounded capacity");
        assert_eq!(ws.broadcast[0].content, "high", "highest ML salience wins");
        assert_eq!(ws.broadcast[1].content, "mid");
    }

    // what this catches: ACTING-ORGANISM step 1 — the mind can express "I want to
    // act on the world" as a first-class verdict. A deliberation faculty emits
    // Decision::Act{calls,intent}; the arbiter routes it like any decision, and
    // decision() returns it carrying the calls + the mind's narrated intent. This
    // is the vocabulary the genome learns to use (the disposition is trained, never
    // hardcoded); the executor that runs the calls and re-enters the result as a
    // memory is steps 3–4. Act is peer to Speak, not a special case.
    #[tokio::test]
    async fn deliberation_can_emit_an_act_decision() {
        let call = ToolCall {
            id: "toolu_run_1".to_string(),
            name: "code/run".to_string(),
            input: serde_json::json!({ "lang": "rust", "code": "fn main() { println!(\"{}\", (0..5).sum::<i32>()); }" }),
        };
        let act = Decision::Act {
            calls: vec![call.clone()],
            intent: "run my solution to see what it actually prints".to_string(),
        };
        let faculties: Vec<Arc<dyn Faculty>> = vec![Arc::new(FixedFaculty(Contribution::verdict(
            act,
            0.95,
            "the model emitted a tool call",
        )))];
        let ws = cycle(faculties, 4).run("peer: does your sum work?").await;
        match ws.decision() {
            Some(Decision::Act { calls, intent }) => {
                assert_eq!(calls.as_slice(), std::slice::from_ref(&call));
                assert!(
                    intent.contains("run my solution"),
                    "the mind's narrated intent rides on the Act decision"
                );
            }
            other => panic!("expected an Act verdict to route through the arbiter, got {other:?}"),
        }
        // verdict() surfaces the intent as the contribution content (audited like
        // any bid) while the calls live on the decision.
        assert_eq!(
            ws.broadcast[0].content,
            "run my solution to see what it actually prints"
        );
    }

    // what this catches: EQUAL CITIZENS — a persona-sent message with high
    // relevance beats a human-sent one with low relevance. Salience (ML) decides,
    // never the sender's rank. This is the death of the Human=1.0/Persona=0.3
    // caste: there is nowhere in this core to encode it.
    #[tokio::test]
    async fn salience_decides_not_sender_caste() {
        // Two world-model bids; the "from a persona" one is more relevant.
        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::WorldModel,
                "human said hi (low value)",
                0.3,
                "pleasantry",
            ))),
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::WorldModel,
                "peer persona flagged a real blocker (high value)",
                0.85,
                "actionable",
            ))),
        ];
        let ws = cycle(faculties, 1).run("burst").await;
        assert_eq!(ws.broadcast.len(), 1);
        assert!(
            ws.broadcast[0].content.contains("peer persona"),
            "the relevant peer-persona content wins on salience, not the human's rank"
        );
    }

    // what this catches: the participation Decision is the OUTPUT of the
    // deliberation faculty's thought, surfaced from the workspace — not a gate.
    // Speak / RaiseUnprompted / Pass are all first-class.
    #[tokio::test]
    async fn decision_is_the_deliberation_faculty_output() {
        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::Recall,
                "context",
                0.4,
                "recalled",
            ))),
            Arc::new(FixedFaculty(Contribution::verdict(
                Decision::RaiseUnprompted {
                    text: "blocker on the deploy".into(),
                },
                0.95,
                "high epistemic value — no one raised it",
            ))),
        ];
        let ws = cycle(faculties, 5).run("coordination thread").await;
        match ws.decision() {
            Some(Decision::RaiseUnprompted { text }) => {
                assert_eq!(text, "blocker on the deploy")
            }
            other => panic!("expected unprompted initiative, got {other:?}"),
        }
    }

    // what this catches: silence (Pass) is a first-class judgment, and abstaining
    // faculties simply don't contribute — no panic, no gate.
    #[tokio::test]
    async fn pass_and_abstain_are_first_class() {
        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(AbstainFaculty(FacultyId::Recall)),
            Arc::new(FixedFaculty(Contribution::verdict(
                Decision::Pass,
                0.6,
                "nothing worth adding",
            ))),
        ];
        let ws = cycle(faculties, 5).run("idle chatter").await;
        assert_eq!(
            ws.broadcast.len(),
            1,
            "the abstaining faculty added nothing"
        );
        assert_eq!(ws.decision(), Some(&Decision::Pass));
    }

    // what this catches: a FAILED model call (a deliberation FAULT) is surfaced by
    // `deliberation_fault()` and is NEVER read as a verdict — `decision()` stays
    // `None`, so the settle step routes it to `InferenceFailed`, never a serene
    // `Passed`. This is the swept-model regression: reassign changed the served
    // model, the faculty still requested the old one, the lane refused, and the Err
    // used to masquerade as chosen silence ([[fallbacks-are-illegal-fail-loud]]).
    #[tokio::test]
    async fn deliberation_fault_is_surfaced_and_is_not_a_decision() {
        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::Recall,
                "context",
                0.4,
                "recalled",
            ))),
            Arc::new(FixedFaculty(Contribution::deliberation_fault(
                "model 'X' is not the active served model (serving: 'Y')",
            ))),
        ];
        let ws = cycle(faculties, 5).run("swept to an unhosted model").await;
        // The fault is readable and names the cause verbatim…
        assert_eq!(
            ws.deliberation_fault(),
            Some("model 'X' is not the active served model (serving: 'Y')"),
        );
        // …and it is NOT a decision, so it can never collapse into Pass/Speak/Act.
        assert_eq!(
            ws.decision(),
            None,
            "a fault is not a verdict — settle_step must see None here and surface InferenceFailed"
        );
    }

    // what this catches: one cycle runs over a CONSOLIDATED burst (the whole
    // world-state at once), not per-event — the efficiency spine.
    #[tokio::test]
    async fn runs_once_over_a_consolidated_burst() {
        let consolidated = "msg1\nmsg2\nmsg3\nmsg4 (many events, one unit)";
        let faculties: Vec<Arc<dyn Faculty>> = vec![Arc::new(FixedFaculty(Contribution::verdict(
            Decision::Speak {
                text: "one reply to the whole thread".into(),
            },
            0.8,
            "caught up on the backlog",
        )))];
        let ws = cycle(faculties, 5).run(consolidated).await;
        assert!(ws.world_state.contains("many events, one unit"));
        assert!(matches!(ws.decision(), Some(Decision::Speak { .. })));
    }

    /// In-memory capture sink — the test-bench / replay primitive.
    #[derive(Default)]
    struct RecordingSink(std::sync::Mutex<Vec<WorkspaceTrace>>);
    impl WorkspaceCaptureSink for RecordingSink {
        fn record(&self, trace: &WorkspaceTrace) {
            self.0.lock().unwrap().push(trace.clone());
        }
    }

    // what this catches: every tick is replayable — the trace captures the FULL
    // competition incl. the LOSER bid (the bit you need to debug "why didn't it
    // win?"), what won, and the decision. This is the glass box that makes
    // working on the mind debuggable + test-benchable, not guesswork.
    #[tokio::test]
    async fn capture_records_the_full_tick_including_losers() {
        let sink = Arc::new(RecordingSink::default());
        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::Recall,
                "loser bid",
                0.1,
                "weak — should lose attention",
            ))),
            Arc::new(FixedFaculty(Contribution::verdict(
                Decision::Speak {
                    text: "winner".into(),
                },
                0.9,
                "high value",
            ))),
        ];
        // capacity 1 → only the winner is broadcast, but the trace keeps both.
        let _ws = WorkspaceCycle::new(faculties, Arc::new(SalienceArbiter), 1)
            .with_capture(sink.clone())
            .run("thread")
            .await;

        let traces = sink.0.lock().unwrap();
        assert_eq!(traces.len(), 1, "one tick recorded");
        let t = &traces[0];
        assert_eq!(t.bids.len(), 2, "trace keeps ALL bids — winner AND loser");
        assert_eq!(t.broadcast.len(), 1, "only the winner held attention");
        assert!(
            t.bids.iter().any(|b| b.content == "loser bid"),
            "the loser bid + its salience + reasoning are replayable for debugging"
        );
        assert_eq!(
            t.decision,
            Some(Decision::Speak {
                text: "winner".into()
            })
        );
    }

    // what this catches: STAGED ASSEMBLY — the deliberation faculty conditions its
    // Decision on what recall surfaced in phase 1. This is the coherence fix: "pull
    // relevant memory, THEN decide." Under the old single-pass join_all the decider
    // bid over an EMPTY broadcast and could never be informed by recall.
    #[tokio::test]
    async fn deliberation_sees_the_recall_that_won_phase_one() {
        let faculties: Vec<Arc<dyn Faculty>> = vec![
            // Phase 1 (perception): recall surfaces a memory with strong salience.
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::Recall,
                "deploy pipeline is red",
                0.8,
                "recalled the open blocker",
            ))),
            // Phase 2 (deliberation): reacts to the assembled broadcast.
            Arc::new(ConditionalDeliberation),
        ];
        let ws = cycle(faculties, 5).run("what's the status?").await;
        match ws.decision() {
            Some(Decision::Speak { text }) => assert!(
                text.contains("deploy pipeline is red"),
                "the decider must condition on recall it saw, got: {text}"
            ),
            other => {
                panic!("expected an informed Speak, got {other:?} — decider was blind to recall")
            }
        }
    }

    // what this catches: the trace exposes the ASSEMBLED CONTEXT the decider saw
    // (context_broadcast) separately from the final broadcast — the glass-box
    // answer to "what RAG did the mind reason over?" — and the deliberation output
    // is appended, not competing for the bounded context spotlight.
    #[tokio::test]
    async fn trace_separates_assembled_context_from_deliberation_output() {
        let sink = Arc::new(RecordingSink::default());
        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::Recall,
                "deploy pipeline is red",
                0.8,
                "recalled",
            ))),
            Arc::new(ConditionalDeliberation),
        ];
        let _ws = WorkspaceCycle::new(faculties, Arc::new(SalienceArbiter), 5)
            .with_capture(sink.clone())
            .run("status?")
            .await;

        let traces = sink.0.lock().unwrap();
        let t = &traces[0];
        // The assembled context is exactly the phase-1 recall winner.
        assert_eq!(t.context_broadcast.len(), 1);
        assert_eq!(t.context_broadcast[0].faculty, FacultyId::Recall);
        // The final broadcast holds the context PLUS the deliberation verdict.
        assert_eq!(
            t.broadcast.len(),
            2,
            "assembled context + deliberation output"
        );
        assert!(
            t.broadcast.iter().any(|c| c.decision.is_some()),
            "verdict appended"
        );
        // Both phases' bids are in the full competition record.
        assert_eq!(t.bids.len(), 2);
    }

    // what this catches: a perception faculty does NOT bid in the deliberation
    // phase (reacts_to_broadcast == false), and a deliberation faculty does NOT
    // bid in the perception phase — each faculty fires once, in its tier. This is
    // the cbar needsRealTime() split: no double inference.
    #[tokio::test]
    async fn faculties_fire_only_in_their_declared_phase() {
        // A faculty that records how many times it was asked to contribute, and in
        // what broadcast state, so we can prove single-phase firing.
        struct CountingDeliberation(std::sync::Arc<std::sync::atomic::AtomicUsize>);
        #[async_trait]
        impl Faculty for CountingDeliberation {
            fn id(&self) -> FacultyId {
                FacultyId::Deliberation
            }
            fn reacts_to_broadcast(&self) -> bool {
                true
            }
            async fn contribute(&self, ws: &Workspace) -> Option<Contribution> {
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                // If we are ever called, the broadcast must be populated (phase 2).
                assert!(
                    !ws.broadcast.is_empty(),
                    "deliberation must only fire after phase 1 assembled context"
                );
                Some(Contribution::verdict(Decision::Pass, 0.5, "noted"))
            }
        }
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::WorldModel,
                "ctx",
                0.7,
                "perception",
            ))),
            Arc::new(CountingDeliberation(calls.clone())),
        ];
        let _ws = cycle(faculties, 5).run("burst").await;
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "deliberation faculty fired exactly once, in its phase"
        );
    }

    // what this catches: the situation-aware focuser must make Situation::PostAction
    // REAL — on a re-perception tick it drops the standing SOCIAL re-grounding
    // (stable contributions: roster/doctrine/workspace-map) so the tool result +
    // working memory own the window, while FreshContext keeps the fuller grounding.
    // Regression guard for the "cognition soup crowds out the code" failure: if a
    // future edit makes PostAction stop dropping stable grounding, a heads-down
    // coding turn goes back to re-dumping social framing on every act→observe tick.
    #[test]
    fn situation_focuser_drops_stable_grounding_only_post_action() {
        let arbiter = SituationFocusArbiter::new();
        // A realistic mixed tick: high-salience standing grounding (stable) plus the
        // volatile task context (the just-landed tool result + a recalled fact).
        let roster =
            Contribution::context(FacultyId::Custom("roster".into()), "room roster", 0.9, "grounding")
                .session_stable();
        let doctrine = Contribution::context(
            FacultyId::Custom("doctrine".into()),
            "operating doctrine",
            0.85,
            "grounding",
        )
        .session_stable();
        let result =
            Contribution::context(FacultyId::WorldModel, "[action #1] code/read → fn main()...", 0.6, "result");
        let recall = Contribution::context(FacultyId::Recall, "recalled: the ticket asks for X", 0.5, "recall");
        let candidates = vec![roster, doctrine, result.clone(), recall.clone()];

        // Fresh ask: fuller grounding — everything within capacity survives, exactly
        // like the blind salience floor.
        let fresh = arbiter.focus(
            candidates.clone(),
            10,
            &FocusContext {
                world_state: "ask",
                situation: Situation::FreshContext,
            },
        );
        assert_eq!(fresh.len(), 4, "FreshContext keeps the fuller grounding");

        // Post-action re-perception: the two stable grounding contributions are
        // dropped BEFORE the top-k, leaving only the volatile task context — even
        // though the stable ones had the HIGHEST salience (the whole point: they were
        // already perceived; re-dumping them just crowds the result out).
        let post = arbiter.focus(
            candidates,
            10,
            &FocusContext {
                world_state: "result",
                situation: Situation::PostAction,
            },
        );
        assert_eq!(post.len(), 2, "PostAction drops the stable standing grounding");
        assert!(
            post.iter().all(|c| !c.stable),
            "no stable grounding survives a re-perception tick: {:?}",
            post.iter().map(|c| c.content.as_str()).collect::<Vec<_>>()
        );
        assert!(
            post.iter().any(|c| c.content.contains("[action #1]")),
            "the tool result MUST survive — it's what 'what next' turns on"
        );
    }
}
