//! The semantic distiller: N related episodic engrams in, one durable fact out.

use super::*;

/// One durable fact distilled from a cluster of episodic engrams.
///
/// Carries full provenance: `source_ids` is every episodic engram that fed the
/// distillation, in input order. The region that turns this into a `Semantic`
/// `Engram` decides how to record that provenance against the engram model
/// (`EngramOrigin::SelfReflection` carries a single `parent_engram_id` today;
/// multi-source provenance on the engram is a follow-up slice). `tags` is the
/// union of the sources' `recall_keys`, so the distilled fact is retrievable by
/// the same keys its sources were.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistilledFact {
    /// The consolidated fact, as the model wrote it (trimmed).
    pub content: String,
    /// Every source engram id that fed this distillation, in input order.
    pub source_ids: Vec<Uuid>,
    /// Union of the sources' recall keys (first-seen order), so the fact is
    /// retrievable by the same keys its sources were.
    pub tags: Vec<String>,
    /// Prior-belief engrams the MODEL judged replaced/contradicted by this new
    /// fact (#221 slice 2 — supersession). Populated only when the distillation
    /// was shown prior beliefs to review; the caller demotes these to the
    /// salience floor so the fresh belief out-ranks them in recall. Empty =
    /// nothing superseded (the common case).
    pub supersedes: Vec<Uuid>,
}

/// Why a distillation could not be produced. Typed + loud — there is no silent
/// "return nothing" path (`[[fallbacks-are-illegal-fail-loud]]`): a caller that
/// gets `Ok` gets a real fact, and any failure names its cause.
#[derive(Debug, Error)]
pub enum DistillError {
    /// The caller passed no source engrams. A distillation of nothing is a
    /// clustering bug in the region, not a runtime condition to paper over.
    #[error("cannot distill: no source engrams provided")]
    NoSources,
    /// The inference adapter returned an error.
    #[error("distillation inference failed: {0}")]
    Inference(String),
    /// The model returned empty text. We do NOT fabricate a fact from the raw
    /// transcript — an empty distillation is surfaced, not hidden.
    #[error("distillation produced empty output")]
    EmptyDistillation,
}

/// Distills clusters of episodic engrams into durable semantic facts via the
/// LLM. Holds an inference adapter; the region (a later slice) owns one of these
/// and feeds it clusters on its idle-tick cadence.
pub struct SemanticDistiller {
    adapter: Arc<dyn AIProviderAdapter>,
    /// The served-model id to ask the adapter for. `None` only for adapters
    /// with a single implicit model (the test heuristic); production callers
    /// MUST thread the persona's live binding model — omitting it caused the
    /// first live dream's degenerate role-token output (2026-07-12).
    model: Option<String>,
    /// Max chars the observation block may occupy (#175 budget-at-assembly). The
    /// distiller BUDGETS the cluster to fit the served slot by dropping whole
    /// trailing engrams — it never truncates an engram's text (that malforms the
    /// prompt) — so an over-large cluster can never overflow the per-slot window
    /// and 500 "Compute error" / poison the lane
    /// ([[budget-at-assembly-never-clamp-the-prompt]]). Injected at construction
    /// (where the live served window is known) so the distiller itself stays a
    /// pure fn of (engrams, budget). Defaults conservative.
    max_observation_chars: usize,
}

/// A sub-personal LENS — one inner voice of the mind-wanderer arc (#145,
/// [[mind-wanderers-subpersonal-processes]]). Each lens is the SAME machinery
/// (walk engrams → one LLM pass → admit_reflection with content-hash dedup)
/// with a different way of looking. The clinical mapping that motivates the
/// design: multiplicity done right is lenses over a shared store, never
/// separate selves — and every lens's output carries its provenance tag
/// IN-CONTENT (`[thought:<lens>]`) so recall renders it typed and inner speech
/// can never masquerade as perception (the anti-source-monitoring-failure
/// invariant; the consolidator's plain facts are the one exception — a
/// distilled durable fact IS first-class knowledge, not a passing thought).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lens {
    /// Stable name — telemetry, the `[thought:<name>]` tag, purpose string.
    pub name: &'static str,
    /// The lens's way of looking, as the system prompt.
    pub system_prompt: &'static str,
    /// Inference-accounting purpose string (stable across renames — the
    /// consolidator keeps its historical "dream-consolidation" telemetry key).
    pub purpose: &'static str,
    /// Whether output is tagged `[thought:<name>]` (true for wanderer thoughts)
    /// or admitted as a plain durable fact (the consolidator).
    pub tag_output: bool,
}

/// The consolidator — the original dream lens: episodic clusters → one durable
/// semantic fact, admitted untagged (it IS knowledge, not commentary).
pub const LENS_CONSOLIDATOR: Lens = Lens {
    name: "consolidator",
    system_prompt: "\
You are consolidating your own episodic memories into long-term knowledge. \
Below are several things you observed or experienced, in order. Distill them \
into a SINGLE durable fact: the general, reusable knowledge they share, stated \
independently of when or how you learned it. Output ONLY the consolidated fact \
as one or two plain sentences — no numbering, no preamble, no commentary, no \
quotes. If the observations share no single consolidatable fact, state the one \
most important durable takeaway.",
    purpose: "dream-consolidation",
    tag_output: false,
};

/// The historian — mind-wanderer outlier A (#145): looks across her OWN recent
/// history for the pattern she is living but not seeing (repeated attempts,
/// what worked vs what didn't, a habit forming). Continuous consolidation in
/// the ext4 sense — the running gist maintained in small increments.
pub const LENS_HISTORIAN: Lens = Lens {
    name: "historian",
    system_prompt: "\
You are the historian voice of your own mind, quietly reviewing your recent \
experiences. Below are several of your own memories, in order. Notice the \
PATTERN across them that you may be living without seeing: something you have \
tried repeatedly, what actually worked versus what did not, a habit forming, a \
thread you dropped. Output ONE short observation about your own recent history \
— one or two plain sentences, addressed to yourself, no preamble, no quotes. \
If there is truly no pattern, name the single most notable thing that happened.",
    purpose: "wanderer-historian",
    tag_output: true,
};

/// The belief-review lens (#221 slice 2c — review-only dreams): on a quiet day
/// (no fresh experience), the dream re-examines a batch of her OLDEST
/// consolidated beliefs on their own — restate what still holds, supersede what
/// doesn't. Without this, reviewing old beliefs required new experience as a
/// vehicle, and a repeat-work day (deduped lessons) starved the review queue
/// entirely (glass-boxed 2026-07-22: six ludicrous cycles, zero supersede
/// probes — dedup correctly dropped identical lessons, so dreams never woke).
pub const LENS_REVIEWER: Lens = Lens {
    name: "reviewer",
    system_prompt: "\
You are re-examining several of your own OLDER long-term beliefs to keep your \
knowledge current. Below are beliefs you consolidated some time ago. Restate, \
as one or two plain sentences, the most important understanding among them \
that remains true and useful today — no numbering, no preamble, no quotes. \
Follow the instructions after the list to flag any that are outdated, overly \
specific to past work, or contradicted by your current understanding.",
    purpose: "dream-belief-review",
    tag_output: false,
};

impl SemanticDistiller {
    /// A distiller whose observation block is budgeted to `observation_chars` (#175). The
    /// caller derives it from the LIVE served per-slot window (tokens → chars, minus
    /// system-prompt + reply reserve) so the dream prompt is composed WITHIN the slot and
    /// never overflows. There is no default: a window guessed for an unwired build is a second
    /// budget. A single engram larger than it still distills ([`Self::observations_block`]
    /// always keeps one whole).
    pub fn new(adapter: Arc<dyn AIProviderAdapter>, observation_chars: usize) -> Self {
        Self {
            adapter,
            model: None,
            max_observation_chars: observation_chars,
        }
    }

    /// Ask the adapter for a specific served model (the persona's live binding).
    pub fn with_model(mut self, model: Option<String>) -> Self {
        self.model = model;
        self
    }

    /// Consolidate a cluster of related episodic engrams into one durable fact.
    ///
    /// `persona_id` attributes the inference to its owning persona for
    /// per-persona resource accounting (the dream IS attributable work, not an
    /// ad-hoc probe). The distiller stays persona-agnostic otherwise — it does
    /// not read the persona's store, only the engrams handed to it. Source order
    /// is preserved in [`DistilledFact::source_ids`].
    pub async fn distill(
        &self,
        persona_id: Option<Uuid>,
        sources: &[Engram],
    ) -> Result<DistilledFact, DistillError> {
        self.distill_reviewing(LENS_CONSOLIDATOR, persona_id, sources, &[])
            .await
    }

    /// Distill through a specific [`Lens`] — the generalized wanderer pass.
    /// Tagged lenses get their `[thought:<name>]` provenance prefixed onto the
    /// content HERE, at the one synthesis point, so no admit path can forget it.
    pub async fn distill_with(
        &self,
        lens: Lens,
        persona_id: Option<Uuid>,
        sources: &[Engram],
    ) -> Result<DistilledFact, DistillError> {
        self.distill_reviewing(lens, persona_id, sources, &[]).await
    }

    /// [`distill_with`](Self::distill_with) plus a SUPERSESSION REVIEW (#221
    /// slice 2): the model is additionally shown up to a handful of the
    /// persona's PRIOR consolidated beliefs (retrieved by recall-key overlap —
    /// mechanics, like recall itself) and asked, as part of the SAME single
    /// generation, which if any are replaced or contradicted by the new
    /// understanding. The JUDGMENT is the model's — never a similarity
    /// threshold ([[cognition-is-always-ml-never-heuristic]]). Its verdict
    /// comes back as a trailing `SUPERSEDES: <numbers>` line that is parsed
    /// OUT of the fact content and mapped to engram ids in
    /// [`DistilledFact::supersedes`]. No priors → identical to `distill_with`.
    pub async fn distill_reviewing(
        &self,
        lens: Lens,
        persona_id: Option<Uuid>,
        sources: &[Engram],
        prior_beliefs: &[Engram],
    ) -> Result<DistilledFact, DistillError> {
        if sources.is_empty() {
            return Err(DistillError::NoSources);
        }

        // #175 budget-at-assembly: compose the observation block WITHIN the served
        // slot by including whole engrams up to the budget, dropping the tail — never
        // truncating an engram (that malforms). `kept` is the prefix that actually
        // fed the distillation; provenance (source_ids/tags) must reflect ONLY those,
        // so the dropped engrams stay unconsolidated and get another pass in a smaller
        // future cluster ([[budget-at-assembly-never-clamp-the-prompt]]).
        // #3847: PRIOR BELIEFS SHARE THE OBSERVATION BUDGET.
        //
        // The observations block has always been budgeted (whole engrams, tail
        // dropped, never truncated). The prior-beliefs list appended below was
        // bounded only by COUNT (SUPERSESSION_REVIEW_LIMIT +
        // ROTATING_REVIEW_PER_PASS) - and a belief is distilled prose of any
        // length, so N bounded beliefs times unbounded text is an unbounded
        // block. Both halves land in ONE user message, so the careful
        // observation budget bought nothing.
        //
        // Measured on BigMama: one user message of 141,984 approx tokens against
        // a 25,344-token served slot - 5.6x over, and every overshoot was
        // timestamp-identical to a dream.cluster.budgeted row. The slot rejects
        // it, so the pass yields nothing and the engrams stay unconsolidated.
        //
        // A floor is reserved for beliefs so supersession cannot be starved to
        // silence by a large cluster: with none, observations filling the budget
        // would leave zero beliefs, and 'superseded nothing' would be
        // indistinguishable from 'was shown no belief'.
        // Charged ONLY when beliefs will actually be written. Unconditional, it
        // capped observations at 75% on every pass with no prior beliefs — a new
        // persona, a fresh domain, any cluster whose topic has no history —
        // reserving a quarter of the budget for a list that never gets appended,
        // on exactly the passes carrying the most new material (IntelMac, #3861).
        let belief_floor = if prior_beliefs.is_empty() {
            0
        } else {
            self.max_observation_chars / 4
        };
        let observation_budget = self.max_observation_chars.saturating_sub(belief_floor);
        let (mut block, kept_n) = Self::observations_block(sources, observation_budget);
        let kept = &sources[..kept_n];
        if !prior_beliefs.is_empty() {
            // Numbered list FIRST, then an UNCONDITIONAL format slot at the very
            // end — recency beats primacy for instruction-following on small
            // models, and a mandatory `SUPERSEDES:` line (with `none` as the
            // explicit no-op) separates "chose none" from "ignored the
            // instruction". Glass-boxed 2026-07-22: 25 live reviews, zero
            // verdict lines, with the old conditional instruction placed BEFORE
            // the list.
            block.push_str("\n\nPRIOR BELIEFS (numbered):\n");
            // Whole beliefs up to the remaining budget, dropping the tail - the
            // SAME rule the observations block follows, because a half-belief is a
            // malformed premise, not a smaller one
            // ([[budget-at-assembly-never-clamp-the-prompt]]).
            let mut beliefs_kept = 0usize;
            for (i, b) in prior_beliefs.iter().enumerate() {
                let line = format!("{}. {}\n", i + 1, b.content.trim());
                if block.len() + line.len() > self.max_observation_chars {
                    break;
                }
                block.push_str(&line);
                beliefs_kept += 1;
            }
            if beliefs_kept < prior_beliefs.len() {
                // The size of the belief that did NOT fit, so `kept = 0` is a
                // READABLE state rather than a mystery: with none shown,
                // "superseded nothing" and "was shown no belief" are otherwise
                // indistinguishable, which is the ambiguity the reserved floor
                // was reaching for. A named row achieves it without the
                // unconditional-first-belief case that can blow the slot
                // (IntelMac, #3861).
                let dropped_first_chars = prior_beliefs
                    .get(beliefs_kept)
                    .map(|b| b.content.trim().len())
                    .unwrap_or(0); // unwrap_or: kept == len means nothing was dropped, so there is no first-dropped belief to size
                tracing::info!(
                    probe_class = "dream.beliefs.budgeted",
                    beliefs = prior_beliefs.len(),
                    kept = beliefs_kept,
                    dropped_first_chars,
                    budget_chars = self.max_observation_chars,
                    block_chars = block.len(),
                    "prior-belief list exceeded the served slot budget - reviewing the \
                     first {beliefs_kept}, deferring the rest to a later pass (#3847)"
                );
            }
            // The instruction's key phrase is ONE constant the Speak gate shares
            // (`framing_echo::CONSOLIDATOR_SUPERSEDES_INSTRUCTION`): a citizen who
            // speaks this scaffold in a room is passing, never posting.
            block.push_str(&format!(
                "\nAfter your reply, on its own final line, {}, wrong, or replaced by better understanding — \
                 exactly `SUPERSEDES: 2` or `SUPERSEDES: 1,3` — or exactly \
                 `SUPERSEDES: none` if every one still holds. This final line is REQUIRED.",
                crate::cognition::framing_echo::CONSOLIDATOR_SUPERSEDES_INSTRUCTION
            ));
        }
        if kept_n < sources.len() {
            tracing::info!(
                probe_class = "dream.cluster.budgeted",
                cluster = sources.len(),
                kept = kept_n,
                budget_chars = self.max_observation_chars,
                "dream cluster exceeded the served slot budget — distilling the first \
                 {kept_n} engrams, deferring the rest (never overflow the slot, #175)"
            );
        }

        // max_tokens stays None — the adapter owns generation length (#45/#46);
        // no per-call clamp. The distillation's faithfulness is gated by VDD
        // with a real model, not by hand-tuned sampling knobs here.
        let request = TextGenerationRequest {
            messages: vec![ChatMessage::text("user", block)],
            system_prompt: Some(lens.system_prompt.to_string()),
            model: self.model.clone(),
            provider: None,
            temperature: None,
            max_tokens: None,
            reasoning_budget: None,
            top_p: None,
            top_k: None,
            repeat_penalty: None,
            frequency_penalty: None,
            repeat_last_n: None,
            stop_sequences: None,
            tools: None,
            tail_tools: None,
            view_messages: None,
            tool_choice: None,
            response_format: None,
            active_adapters: None,
            request_id: None,
            user_id: None,
            room_id: None,
            purpose: Some(lens.purpose.to_string()),
            persona_id: persona_id.map(|id| id.to_string()),
            turn_bound: None,
        };

        // Take a NON-directed serving lane before the dream's model call. Dream
        // distillation is InferenceHeavy background work, but UNGUARDED it called
        // the adapter directly and grabbed a physical llama decode lane WITHOUT
        // counting against the non-directed budget — silently bypassing the #139
        // reservation that always holds one lane for a directed chat turn. A
        // directed reply then queued INSIDE llama behind the dream, so an idle
        // persona looked comatose and unresponsive (glass-boxed live 2026-07-26:
        // 4 personas dreamed continuously 00:00→05:03, no reply to direct chat).
        // Acquiring the permit here caps ALL dream inference at MAX_LANES-1 and
        // guarantees a waiting chat turn wins the lane — the same guarantee the
        // turn path already relies on (llm_deliberation_faculty acquires it too).
        // Held across the generate call, released on drop.
        //
        // DEFER BEFORE THE LANE (2026-08-29, the leaked-permit ledger): this call
        // acquired the lane and THEN parked at the adapter's measured-hold defer —
        // holding the very permit the held solve's next generation needed. Two
        // such dreams drained the whole pool; the solve starved at every tick-2
        // (grants=3/releases=1, both leaks timestamp-paired to dream defers).
        // The rule every path owes: hold-defer FIRST holding NOTHING, gates second.
        crate::inference::measured_hold::defer_while_held(
            crate::inference::slots::class_for(request.purpose.as_deref()),
            persona_id,
            request.purpose.as_deref(),
        )
        .await;
        let _lane = crate::inference::resource_admission::acquire_serving_lane(
            persona_id,
            crate::inference::resource_admission::LanePriority::Ambient,
            None,
        )
        .await;
        let response = self
            .adapter
            .generate_text(request)
            .await
            .map_err(DistillError::Inference)?;

        let raw = response.text.trim();
        if raw.is_empty() {
            return Err(DistillError::EmptyDistillation);
        }
        // Lift the model's supersession verdict OUT of the fact text (the fact
        // must read as knowledge, not as a grading transcript). The raw tail is
        // probed FIRST — the one glass-box eye on what the model actually wrote
        // before the parse strips it (25 live reviews were undiagnosable
        // without this; the distiller path has no prompt-capture yet).
        if !prior_beliefs.is_empty() {
            let tail = raw.lines().last().unwrap_or("").trim();
            continuum_base::probe!(
                class = "dream.review.raw_tail",
                persona = ?persona_id,
                tail = %tail,
                "belief-review raw verdict line"
            );
        }
        let (raw, supersedes) = parse_supersedes_line(raw, prior_beliefs);
        if raw.is_empty() {
            return Err(DistillError::EmptyDistillation);
        }
        // Provenance is prefixed at the ONE synthesis point (see distill_with
        // docs): a tagged lens's output always reads as typed inner speech.
        let content = if lens.tag_output {
            format!("[thought:{}] {}", lens.name, raw)
        } else {
            raw.to_string()
        };

        Ok(DistilledFact {
            content,
            source_ids: kept.iter().map(|e| e.id).collect(),
            tags: Self::union_recall_keys(kept),
            supersedes,
        })
    }

    /// Render the cluster as a numbered observation list, BUDGETED to `budget_chars`
    /// (#175). Includes whole engrams in order until the next would exceed the budget,
    /// then stops — dropping the tail rather than truncating an engram's text (which
    /// would malform the prompt). Always includes at least the first engram so a
    /// single large memory still distills (the adapter's overflow backstop is the
    /// last-resort net for that degenerate case). Returns the block AND how many
    /// engrams it kept, so the caller's provenance reflects only what was distilled.
    pub(super) fn observations_block(sources: &[Engram], budget_chars: usize) -> (String, usize) {
        let mut block = String::new();
        let mut kept = 0usize;
        for (i, e) in sources.iter().enumerate() {
            let line = format!("{}. {}\n", i + 1, e.content.trim());
            // First engram always goes in; after that, stop before overflowing.
            if kept > 0 && block.len() + line.len() > budget_chars {
                break;
            }
            block.push_str(&line);
            kept += 1;
        }
        (block, kept)
    }

    /// Union of every source's recall keys, first-seen order preserved.
    pub(super) fn union_recall_keys(sources: &[Engram]) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for e in sources {
            for k in &e.recall_keys {
                if seen.insert(k.clone()) {
                    out.push(k.clone());
                }
            }
        }
        out
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Slice 3: the DreamConsolidationRegion — the organism's rest-state servicer.
// ───────────────────────────────────────────────────────────────────────────
