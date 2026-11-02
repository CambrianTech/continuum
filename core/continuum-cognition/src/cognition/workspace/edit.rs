use super::*;

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
    ///
    /// `reason` is the mind's OWN words for why it passed, captured verbatim at
    /// the single text→decision seam (`deliberation_parse::decision_from_response`)
    /// when she narrates her pass ("done — patch ready", "blocked: the fixture is
    /// missing", "nothing to add") — `None` for a bare `PASS` token. A pass is an
    /// accountable decision, not anonymous silence: the benchmark held-work edge
    /// reads this to tell a gradeable *done* from a *blocker* from a substrate-gap
    /// *nothing* ([[a-citizen-saying-i-have-nothing-to-contribute-is-a-substrate-gap-report]]),
    /// and any consumer can surface WHY a turn produced no utterance.
    Pass { reason: Option<String> },
}

impl Decision {
    /// A bare pass with no stated reason — the anonymous silence a plain `PASS`
    /// token or an empty generation resolves to. Convenience so the common
    /// construction stays terse and every reasoned pass is visibly the exception.
    pub fn pass() -> Self {
        Decision::Pass { reason: None }
    }

    /// A pass the SUBSTRATE decided, not her: a mechanical gate at the Speak seam
    /// refused what she said (a framing echo, a not-speech envelope, a parroted
    /// perception fact). The ONE constructor every gate uses, and
    /// [`Self::gate_refusal`] is the one reader — the mindless receipt (card aed15611)
    /// and the gate-as-lesson (card 657e74de) decide on this, never on a prose prefix
    /// written for the glass box (Cormac's block on #4081).
    pub fn gate_refused(
        gate: &'static str,
        marker: &'static str,
        why: impl std::fmt::Display,
    ) -> Self {
        Decision::Pass {
            reason: Some(format!("{GATE_REFUSAL_PREFIX}{gate}:{marker}: {why}")),
        }
    }

    /// `Some((gate, marker))` iff this pass was a gate refusal. Her own `PASS`, a
    /// narrated blocker, a yield — every pass that was HER decision — reads `None`.
    pub fn gate_refusal(&self) -> Option<(&str, &str)> {
        let Decision::Pass { reason: Some(r) } = self else {
            return None;
        };
        let rest = r.strip_prefix(GATE_REFUSAL_PREFIX)?;
        let (gate, tail) = rest.split_once(':')?;
        let (marker, _) = tail.split_once(':')?;
        Some((gate, marker))
    }
}

/// The typed head of a gate-refusal reason (see [`Decision::gate_refused`]).
pub const GATE_REFUSAL_PREFIX: &str = "gate-refused/";

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
    /// Standing activity context, independently of its KV placement. The prompt
    /// reserves a truthful minimum before optional history: whole atomic content
    /// or a complete source-declared `parts` prefix with an explicit omission
    /// notice. Additional list detail competes for spare room after history.
    pub standing_grounding: bool,
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
    /// Required prompt input did not fit; kept separate from inference failures.
    pub capacity_refusal: Option<CapacityRefusal>,
    /// Every generation THIS contribution dispatched, in dispatch order, faults
    /// included (card 0d51573a).
    ///
    /// A `Vec`, not an `Option`: a faculty may retry, and a one-to-one field for a
    /// one-to-many domain is a mistake I have already made once on this card. EMPTY
    /// means the faculty generated nothing — a different fact from a faculty whose
    /// generation FAILED, and the two must not collapse.
    ///
    /// **RELATIONSHIP TO [`fault`](Self::fault), which is NOT superseded.** `fault`
    /// is the CONTRIBUTION's verdict-level failure: one message, read by
    /// [`Workspace::deliberation_fault`], turned by the settle step into a distinct
    /// `InferenceFailed` outcome instead of a lying `Passed`. These receipts are
    /// PER-CALL provenance: which request id was submitted, what actually served it,
    /// and which individual calls faulted. A contribution can have several receipts
    /// and one `fault`, or receipts with no `fault` at all — a retry that failed then
    /// succeeded produced a real fault receipt and no contribution-level failure.
    /// Neither field can be derived from the other and neither should be removed for
    /// the other's sake.
    pub receipts: Vec<crate::ai::provenance::GenerationReceipt>,
    /// Submitted render facts return through the same owned faculty result.
    /// They are evidence for the resident checkpoint owner, never an acknowledgement.
    pub render_receipts: Vec<crate::cognition::render_receipt::RenderReceipt>,
    /// The model's **verbatim generation** for this verdict — the raw response text
    /// EXACTLY as the model emitted it, BEFORE the tool-call/PASS parser lifted a
    /// [`Decision`] from it. Set ONLY by the deliberation faculty on its verdict
    /// contribution (peer to [`metrics`](Self::metrics)); `None` on every context
    /// contribution and every non-model finding. Rides the broadcast so the ONE
    /// glass-box seam (the workspace capture, e.g. the eval's `--capture_dir`) holds
    /// raw AND parsed together — a fumbled artifact (a stray `<<!DOCTYPE`, a malformed
    /// tool envelope) is then attributable to the MODEL vs the HARNESS from a single
    /// record, never inferred by cross-referencing two files by timestamp (#210).
    pub raw_generation: Option<String>,
    /// **Render nearest generation, as a trailing conversation turn — NOT in the
    /// system message.** Standing framing and byte-stable grounding (roster, doctrine,
    /// map, recall) belong in the system prompt where they anchor the cacheable
    /// KV-prefix. But proprioception that GROWS every act within one settling loop —
    /// the working-memory action ledger, the full most-recent result — must not live
    /// in the system message: the system prompt precedes the whole conversation, so
    /// appending one `[action #n]` line there shifts every conversation token after
    /// it and the KV prefix breaks at the working-memory block, re-prefilling the
    /// entire tail (~4000 tokens / ~30s per act — the #205 eval-lane crawl). A
    /// `trailing` contribution is instead emitted AFTER the conversation turns, so a
    /// settle-act only appends to the very end of the token stream and only that
    /// small new tail re-prefills. The append-only formatting the working-memory
    /// faculty already uses (see its `contribute`) finally pays off, because the
    /// growing block is now genuinely last. Defaults `false` (system-message context);
    /// set via [`trailing`](Self::trailing).
    pub trailing: bool,
    /// The **atomic units** this content is made of, in the order the faculty
    /// emitted them — the granularity at which it is still truthful to cut.
    ///
    /// A faculty whose finding is ONE indivisible thing (a recalled engram, an
    /// affect signal) leaves this a single element equal to `content`: it fits
    /// whole or it is dropped whole, because half an engram is noise. But a
    /// grounding source that already delivers a LIST — the board's `[your work]`
    /// lead, then its `[available work]` lead, then one line per card — has
    /// declared each unit individually meaningful, and cutting between units
    /// costs nothing but the tail.
    ///
    /// The distinction is load-bearing, not cosmetic. Measured 2026-08-06 across
    /// 1,284 context assemblies: `room-kanban` was KEPT **0 times** and dropped
    /// 495 — a median 5,364-token block offered all-or-nothing into a median
    /// 55-token budget. Its first two units are ~200 tokens and carry every fact
    /// a citizen needs to find work; the other ~5,100 are a verbatim 61-card
    /// dump. Flattening the source's own list into one string is what made the
    /// board structurally invisible, so the citizens perceived an empty world and
    /// said "there are no open tasks available". The parts were always there —
    /// the bridge threw them away. [[compression-principle]]
    ///
    /// Never re-ordered and never re-ranked: salience still governs WHICH
    /// contributions are considered, and this only governs how much of one
    /// survives when the whole will not fit. Set via [`with_parts`](Self::with_parts).
    pub parts: Vec<String>,
    /// The exact verb that yields this content IN FULL, when only part of it fit.
    ///
    /// A truncation notice has to name a command a citizen can actually type. "The
    /// full list is available from the matching command" does not — she cannot run
    /// a description, and a name she has to guess is a name she gets wrong. Carried
    /// from the source's own `RagSource::expand_command`, which has no default impl
    /// precisely so every source decides. `None` = nothing further to fetch, and
    /// the notice then says only how much was omitted.
    pub expand_command: Option<&'static str>,
    /// The SOURCE stopped short of its own list at its budget (it minted a
    /// continuation): units exist beyond `parts` that no packing here can show. The
    /// notice then says so even when every delivered part fits, and gives no total,
    /// because the source did not report one ("whole or handle": a cut the reader
    /// cannot see is a silent clip, PERSONA-COGNITION-PIPELINE.md §3).
    pub source_truncated: bool,
    /// The source's own items, each with its stable key and levels (whole, summary,
    /// pointer), when it delivered some: her view prices and collapses them one by one
    /// (`context_streams::blocks`), so the one allocation chooses per card, per post, per
    /// member instead of whole-block-or-minimum. Shared, never copied per reader.
    pub units: std::sync::Arc<[crate::cognition::rag_budget::RagItem]>,
}

impl Contribution {
    /// A context contribution (no decision) — recall, world-model, affect, etc.
    pub fn context(
        faculty: FacultyId,
        content: impl Into<String>,
        salience: f32,
        reasoning: impl Into<String>,
    ) -> Self {
        let content = content.into();
        Self {
            faculty,
            cycle: CycleId::UNSTAMPED,
            // Indivisible by default: one part IS the content, so a faculty that
            // never calls `with_parts` behaves exactly as it always has (fits
            // whole or drops whole). Divisibility is opt-IN, declared by the
            // faculty that actually knows its content is a list.
            parts: vec![content.clone()],
            expand_command: None,
            source_truncated: false,
            units: std::sync::Arc::from(Vec::new()),
            content,
            salience: salience.clamp(0.0, 1.0),
            reasoning: reasoning.into(),
            decision: None,
            metrics: None,
            stable: false,
            standing_grounding: false,
            fault: None,
            capacity_refusal: None,
            receipts: Vec::new(),
            render_receipts: Vec::new(),
            raw_generation: None,
            trailing: false,
        }
    }

    /// Declare this contribution's **atomic units** — the granularity at which
    /// cutting it stays truthful (see [`parts`](Self::parts)). Used by the
    /// grounding bridge, which receives a source's `RagItem` list and previously
    /// flattened it to one string, making a large block all-or-nothing against
    /// the prompt budget.
    ///
    /// Empty input is ignored: a contribution always has at least one part, so
    /// the assembly path never has to special-case "divisible but with nothing
    /// to divide". `content` is left untouched — it remains the whole, and is
    /// what renders whenever the whole fits.
    /// The source's keyed, levelled items (see [`Contribution::units`]).
    pub fn with_units(mut self, units: Vec<crate::cognition::rag_budget::RagItem>) -> Self {
        self.units = units.into();
        self
    }

    pub fn with_parts(mut self, parts: Vec<String>) -> Self {
        if !parts.is_empty() {
            self.parts = parts;
        }
        self
    }

    /// Name the verb that yields this content in full — see [`expand_command`](Self::expand_command).
    /// Mark that the source withheld units beyond `parts` (see `source_truncated`).
    pub fn with_source_truncated(mut self, truncated: bool) -> Self {
        self.source_truncated = truncated;
        self
    }

    pub fn with_expand_command(mut self, cmd: Option<&'static str>) -> Self {
        self.expand_command = cmd;
        self
    }

    /// Carry the assembler's standing-framing contract through attention to the
    /// prompt budget. This neither changes salience nor chooses KV placement.
    pub fn standing_grounding(mut self) -> Self {
        self.standing_grounding = true;
        self
    }

    /// The deliberation faculty's verdict contribution.
    pub fn verdict(decision: Decision, salience: f32, reasoning: impl Into<String>) -> Self {
        let content = match &decision {
            Decision::Speak { text } | Decision::RaiseUnprompted { text } => text.clone(),
            // The mind's narration of WHY it's acting — surfaced/audited like any
            // contribution content; the calls themselves live on the decision.
            Decision::Act { intent, .. } => intent.clone(),
            Decision::Pass { reason } => reason.clone().unwrap_or_default(),
        };
        Self {
            faculty: FacultyId::Deliberation,
            cycle: CycleId::UNSTAMPED,
            // A verdict is one utterance — never a list, never divisible.
            parts: vec![content.clone()],
            expand_command: None,
            source_truncated: false,
            units: std::sync::Arc::from(Vec::new()),
            content,
            salience: salience.clamp(0.0, 1.0),
            reasoning: reasoning.into(),
            decision: Some(decision),
            metrics: None,
            // A verdict is the volatile output of THIS turn; never standing framing.
            stable: false,
            standing_grounding: false,
            fault: None,
            capacity_refusal: None,
            receipts: Vec::new(),
            render_receipts: Vec::new(),
            raw_generation: None,
            trailing: false,
        }
    }

    /// Complete input cannot fit this serving window; retain its typed cause.
    pub fn prompt_capacity_refused(error: PromptCapacityError) -> Self {
        let mut contribution = Self::deliberation_fault(error.to_string());
        contribution.fault = None;
        contribution.reasoning = "complete prompt input does not fit the serving window".into();
        contribution.capacity_refusal = Some(error.into());
        contribution
    }

    pub fn with_capacity_readiness(mut self, readiness: Arc<CapacityReadiness>) -> Self {
        if let Some(refusal) = &mut self.capacity_refusal {
            refusal.readiness = Some(readiness);
        }
        self
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
            // A fault is one named cause — never a list, never divisible.
            parts: vec![error.clone()],
            expand_command: None,
            source_truncated: false,
            units: std::sync::Arc::from(Vec::new()),
            content: error.clone(),
            salience: 1.0,
            reasoning: "deliberation inference failed".to_string(),
            decision: None,
            metrics: None,
            stable: false,
            standing_grounding: false,
            fault: Some(error),
            capacity_refusal: None,
            receipts: Vec::new(),
            render_receipts: Vec::new(),
            raw_generation: None,
            trailing: false,
        }
    }

    /// Stamp the model-call cost onto this contribution (builder form, so the
    /// deliberation faculty can do `self.verdict(...).with_metrics(m)`).
    pub fn with_metrics(mut self, metrics: TurnMetrics) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Stamp the model's **verbatim generation** onto this verdict (builder form,
    /// peer to [`with_metrics`](Self::with_metrics)) — the raw response text before
    /// the parser lifted a [`Decision`], so the glass box holds raw + parsed in one
    /// record (#210). Empty text stays `None` (nothing to attribute).
    pub fn with_raw_generation(mut self, raw: impl Into<String>) -> Self {
        let raw = raw.into();
        if !raw.is_empty() {
            self.raw_generation = Some(raw);
        }
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

    /// Mark this contribution as **trailing proprioception** — content that GROWS
    /// each act within one settling loop (the working-memory ledger, the full
    /// most-recent result) and so must render nearest generation, as a trailing
    /// conversation turn, NOT in the system message. Builder form so a faculty can do
    /// `Contribution::context(...).trailing()`. See [`Contribution::trailing`] for
    /// the KV-prefix rationale (#205).
    pub fn trailing(mut self) -> Self {
        self.trailing = true;
        self
    }
}

/// Is this text something somebody SAID, or something the system TOLD her?
///
/// Orthogonal to authorship on purpose. `is_self`/`author` answer "whose voice is this"
/// (which drives assistant-vs-user role attribution); this answers "is this speech at all".
/// They are different questions and conflating them cost a live defect: perception facts and
/// raw-string test/eval stimuli were BOTH built with [`BurstTurn::opaque`], so "no known
/// author" was the only signal available, and a guard keyed on it could not tell a system
/// brick from a peer's message with the author stripped (caught 2026-08-06 by
/// `deliberates_through_a_real_adapter`, which builds its stimulus with `Workspace::new`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TurnVoice {
    /// Somebody spoke it — the persona, a peer, or an unattributed raw stimulus. It is
    /// conversation, and reproducing it is a normal (if sometimes redundant) social act.
    #[default]
    Speech,
    /// The SYSTEM told her — a perception fact, written in second person, addressed to her.
    /// Never something she may say back into the room: see
    /// [`crate::cognition::parroted_perception`].
    Perception,
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
    /// Speech, or something the system told her. See [`TurnVoice`].
    pub voice: TurnVoice,
    /// The airc event this turn is, when it came from a room: what keys it in her view and
    /// what her read receipt advances to once a sent prompt rendered it. `None` for opaque,
    /// eval and perception turns.
    pub source: Option<TurnSource>,
}

/// The room event a turn came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurnSource {
    pub room: uuid::Uuid,
    pub event: uuid::Uuid,
    pub lamport: u64,
}

impl BurstTurn {
    /// This turn is the room event `source`.
    pub fn from_event(mut self, source: TurnSource) -> Self {
        self.source = Some(source);
        self
    }

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
            voice: TurnVoice::Speech,
            source: None,
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
            voice: TurnVoice::Speech,
            source: None,
        }
    }

    /// A PERCEPTION FACT — the system's own words to her, not anybody's speech.
    ///
    /// Renders identically to an opaque turn (unattributed `user` text, byte-for-byte the
    /// same prompt), so this changes nothing the model sees. What it changes is what the
    /// SUBSTRATE knows: [`crate::cognition::parroted_perception`] can now tell "the system
    /// told her this" from "someone said this", which is the difference between silencing a
    /// parroted brick and silencing a citizen who agreed with a teammate.
    pub fn perception(content: impl Into<String>) -> Self {
        Self {
            voice: TurnVoice::Perception,
            source: None,
            ..Self::opaque(content)
        }
    }

    /// Render this turn as ONE line of the world-state projection, matching the
    /// historical `build_workspace_burst` format exactly: `[t={ms}] {author}:
    /// {content}` when timed, `{author}: {content}` when authored-but-untimed,
    /// the bare content (no prefix, no added newline) when opaque.
    pub(super) fn write_line(&self, out: &mut String) {
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
    /// Room messages perceived while this turn was already underway. Shared with
    /// the conversation's retained attention queue; never replace this burst's
    /// task, room, or causal root. This is the existing chat projection, not a
    /// replacement for the multimodal sensory faculties.
    pub room_updates: Arc<Vec<Arc<crate::cognition::conversation::IncomingMessage>>>,
    /// The activity this turn belongs to — REQUIRED at construction, witnessed
    /// non-nil by [`ActivityRoom`]. The burst carries the room because the burst
    /// IS the perception the turn responds to: when the room rode as a separate
    /// parameter beside the burst, a caller could render room A into the header
    /// and drive the turn in room B (or nil), and nothing detected it — the #425
    /// roomless-turn shape. Same argument as [`Cause`] living here.
    pub room: crate::identity::ActivityRoom,
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
    /// WHY this turn is happening — see [`Cause`].
    ///
    /// It lives on the `Burst` because the burst IS the perception the turn responds
    /// to: threading the cause as a separate parameter through every driver would let
    /// the two drift, and provenance belongs on the thing whose provenance it is.
    pub cause: Cause,
    /// WHICH of her hands this turn acts with: the acting context of the card claim it
    /// works, or `None` for her default hands. Bound once, at the turn's admission, by
    /// whoever admitted it; every act of the turn runs in these hands and no turn moves
    /// another's (card fc9e9fd7). It lives here for the reason `room` and `cause` do: a
    /// parameter beside the burst could drift from the turn it belongs to.
    pub hands: Option<crate::identity::hands_context::HandsContext>,
    /// This turn's pre-effect mark (stop-drain S2): `Some` when the turn has a confirmed
    /// obligation the next start could re-admit, so every effect must be recorded first.
    pub effects: Option<crate::cognition::act_observe::EffectGate>,
}
