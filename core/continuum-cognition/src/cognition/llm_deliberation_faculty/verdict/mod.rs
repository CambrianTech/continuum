use super::*;

mod contribute;
mod newest_refit;

/// The working-memory record of a cut act: that it did not land, how long it ran, and
/// how it began and ended, so the next generation starts from what she was composing
/// rather than re-deriving it. Head and tail only: the whole payload is what was too
/// large to land, and repeating it would crowd the retry the same way. Pure.
pub(super) fn cut_act_fact(text: &str) -> String {
    const EDGE: usize = 240;
    let chars = text.chars().count();
    let head: String = text.chars().take(EDGE).collect();
    let tail: String = if chars > 2 * EDGE {
        let skip = chars - EDGE;
        text.chars().skip(skip).collect()
    } else {
        String::new()
    };
    if tail.is_empty() {
        format!("my last act was CUT at the output limit after {chars} chars before a tool call was committed — it did not land, nothing ran. I was composing: {head}")
    } else {
        format!("my last act was CUT at the output limit after {chars} chars before a tool call was committed — it did not land, nothing ran. I was composing: {head} … and it ended: {tail}")
    }
}

/// Apportion the server's output count between the reasoning channel and the answer
/// by the bytes each channel carried — the server counts tokens, the adapter splits
/// text. No channel text at all (a native tool call) is all answer.
pub(super) fn reasoning_answer_split(output_tokens: u32, reasoning_chars: usize, answer_chars: usize) -> (u32, u32) {
    let total = reasoning_chars.saturating_add(answer_chars);
    if total == 0 || reasoning_chars == 0 {
        return (0, output_tokens);
    }
    let reasoning = ((output_tokens as u64 * reasoning_chars as u64) / total as u64) as u32;
    let reasoning = reasoning.min(output_tokens);
    (reasoning, output_tokens - reasoning)
}

/// Native tool arguments are generated answer bytes too. `text` is the adapter's
/// plain-text projection, so counting it alone turns thinking + tools into a
/// think-only sample. Do not count Text parts again: they mirror that projection.
pub(super) fn answer_payload_bytes(text: &str, parts: Option<&[crate::ai::types::ContentPart]>) -> usize {
    parts.unwrap_or_default().iter().fold(text.len(), |bytes, part| {
        match part {
            crate::ai::types::ContentPart::ToolUse { name, input, .. } => {
                bytes.saturating_add(name.len()).saturating_add(input.to_string().len())
            }
            _ => bytes,
        }
    })
}

impl LlmDeliberationFaculty {
    /// Turn an emitted tool-call into an [`Decision::Act`] verdict — the persona
    /// has chosen to use her hands. The faculty does NOT run the calls; the
    /// act→observe driver ([`super::super::act_observe`]) executes them, remembers the
    /// result, and re-perceives. `intent` is the model's own stated reasoning when
    /// present (so the engram records WHY she acted), else a plain default.
    pub(super) fn act_verdict(
        &self,
        calls: Vec<crate::ai::types::ToolCall>,
        resp: &TextGenerationResponse,
        activity_room: Uuid,
    ) -> Contribution {
        if let Some(memory) = &self.working_memory {
            let native = resp.tool_calls.as_deref().filter(|original| {
                matches!(resp.finish_reason, FinishReason::ToolUse)
                    && original.iter().map(|call| &call.id).eq(calls.iter().map(|call| &call.id))
            }).unwrap_or_default();
            // Every new action replaces pending provenance, including legacy/text
            // decisions after an executor-wide failure produced no receipt.
            memory.record_native_action_statement(
                if native.is_empty() { "" } else { &resp.text }, activity_room, native,
            );
        }
        // The model's OWN stated reasoning (a `<think>` block) when present — so the
        // engram records WHY she acted. When ABSENT, leave it EMPTY, never fabricate
        // a reason. The old default — "{name} is acting on the current situation" —
        // was content-free AND the worst mimicry fuel (#158): it rendered into every
        // receipt as "because {name} is acting…", the model imitated that template as
        // speech, and because the phrasing is identical for everyone, personas
        // cross-copied each other's names (Anwen's turn narrating "because Asha is
        // acting" — the identity bleed). An empty intent renders a receipt with no
        // "because" clause: nothing template-shaped to imitate.
        let intent = resp
            .reasoning
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_default();
        Contribution::verdict(
            Decision::Act { calls, intent },
            0.9,
            format!("{} chose to act", self.persona_name),
        )
        .with_metrics(metrics_from(&self.persona_name, resp))
        // #210: carry the verbatim generation so the glass box can attribute a fumbled
        // tool envelope to the model, not the parser. The Act's `intent` is only the
        // model's `<think>` reasoning; the raw text is the actual emitted call bytes.
        .with_raw_generation(resp.text.clone())
    }

    /// A draft that reproduces what the system just told her is not a contribution — it is
    /// the prompt coming back out. Downgrade it to silence.
    ///
    /// Measured live 2026-08-06: the repetition brick fired on Anwen, and her next room
    /// message WAS the brick, verbatim, second person intact. The mechanism built to break
    /// the loop became the loop's next turn. See [`parroted_perception`] for why this is a
    /// containment comparison against the burst's own opaque turns and emphatically NOT a
    /// reserved-word ban — a citizen discussing her own cognition must stay speakable.
    ///
    /// Silence, not stripping: a turn whose content is a reflection of its own prompt
    /// contributed nothing, and deleting the echoed span would only make it LOOK like it did.
    /// The probe is the glass box — an unexplained silence is its own defect
    /// ([[a-probe-that-can-only-fail-is-worse-than-no-probe]]), so this always says which
    /// fact was echoed.
    fn silence_a_parroted_draft(&self, decision: Decision, ws: &Workspace) -> Decision {
        let Decision::Speak { text } = &decision else {
            return decision;
        };
        let mut facts = parroted_perception::perception_facts(&ws.turns);
        // Her IDENTITY block is the system's own words too — and measurably the one she
        // reproduces most. 2026-09-14, IntelMac, from her own prompt capture: a 6,187-char
        // turn opening with 123 chars of "Yes, I can assist with that task. Please provide
        // more details..." and then her entire identity prompt verbatim ("Identity (never
        // drift from this): - You are Paige. You are NOT Claude, GPT, ..."). ~98% of the
        // turn was the thing she was told, and it reached the room.
        //
        // It reached the room because every gate we had was looking somewhere else:
        // `framing_echo` is ANCHORED (a marker counts only when it LEADS), so 123
        // characters of filler walked straight past it — correctly, since un-anchoring it
        // would silence every citizen REPORTING an echo. And this gate only saw
        // `TurnVoice::Perception` turns, which the system prompt is not: it is composed by
        // `deliberation_prompt` into the system message, never as a burst turn. The system's
        // most-repeated words were the one thing "did you speak what the system said to you"
        // did not check.
        //
        // Containment is asymmetric and that is what makes this safe to add rather than a
        // new false-positive surface: `containment(draft, fact)` asks how much of the FACT
        // reappears in the DRAFT. The identity block is large enough that a citizen who
        // merely talks ABOUT herself ("I'm Paige, I work on the grid") shares a handful of
        // its tokens and scores near zero. Only near-total reproduction reaches 0.8. No new
        // threshold, no new measure, no phrase list.
        //
        // PER BLOCK, NEVER WHOLE — measured, after I first wrote this as a single
        // `facts.push(&self.system_prompt)` and checked it against her real turn:
        //
        //     containment(draft, WHOLE composed prompt) = 0.135   INERT
        //     containment(draft, identity block)        = 0.875   fires
        //     next-highest block                        = 0.433   clean margin
        //
        // Because containment measures how much of the FACT came back, a fact gets HARDER to
        // trip the longer it is. She recited one 56-token block of an 850-token prompt —
        // entirely — and the whole-prompt score calls that speech. Whatever this field holds
        // (identity alone, or identity plus more framing) the split is what makes the gate
        // work, so this does not depend on knowing which.
        facts.extend(parroted_perception::prompt_blocks(&self.system_prompt));
        let Some(echoed) = parroted_perception::parroted_fact(
            text,
            &facts,
            parroted_perception::PARROT_CONTAINMENT_THRESHOLD,
        ) else {
            return decision;
        };
        continuum_base::probe!(
            class = "persona.speech.parroted_perception",
            persona = %self.persona_name,
            echoed_fact = %echoed.chars().take(120).collect::<String>(),
            draft_len = text.len(),
            "draft reproduced a perception fact she was handed — settling to silence instead \
             of speaking the prompt back into the room (#158)"
        );
        // Typed like every other gate refusal: an hour of parroted recitals is the
        // most on-the-nose mindless hour there is (Cormac's block on #4081).
        Decision::gate_refused("parroted_perception", "perception_fact", "the draft reproduced a fact she was handed")
    }

    /// She called the yield verb — settle the turn as the silence it names.
    ///
    /// The STRUCTURED half of #271/#264. A citizen with nothing to add has, until now,
    /// had no way to say so except to write a paragraph announcing it — which is itself
    /// a room message that wakes the next peer into announcing theirs. Now silence is a
    /// verb, and recognising it is a NAME match on a verb we defined (protocol), not a
    /// phrase match on prose (semantics). See
    /// [`super::super::persona_tools::verdict_tool_specs`] for why this is not a command.
    ///
    /// Checked BEFORE `act_verdict` on both lift paths, so a yield never reaches the
    /// authorization gate or the executor: there is no world-effect to authorize, and
    /// routing it as an `Act` would burn an act from the budget and re-enter the settle
    /// loop — the opposite of ending the turn.
    ///
    /// If she calls the yield ALONGSIDE real work, the work wins: a turn that both did
    /// something and declined to speak is an act with nothing to say, and dropping the
    /// act to honour the yield would silently discard work she actually did.
    pub(super) fn yield_verdict(&self, calls: &[crate::ai::types::ToolCall]) -> Option<Contribution> {
        if !calls
            .iter()
            .any(|c| super::super::persona_tools::is_yield_turn(&c.name))
        {
            return None;
        }
        if calls
            .iter()
            .any(|c| !super::super::persona_tools::is_yield_turn(&c.name))
        {
            return None;
        }
        continuum_base::probe!(
            class = "persona.verdict.yield_turn",
            persona = %self.persona_name,
            "she yielded the turn through the structured verb — silent Pass, no room message"
        );
        Some(Contribution::verdict(
            Decision::pass(),
            0.9,
            format!("{} yielded the turn (yield_turn)", self.persona_name),
        ))
    }

    /// Turn the model's final text into a participation verdict. `salience` is
    /// the faculty's own confidence in its verdict — a placeholder for a model-
    /// derived signal (logprob / uncertainty), NOT a caste weight; it's how sure
    /// THIS mind is, which the arbiter integrates.
    pub(super) fn verdict(&self, resp: &TextGenerationResponse, ws: &Workspace) -> Contribution {
        // The names she can SEE this turn — the room's roster, the same view her
        // prompt was handed — so a first-person claim of a peer's name is refused at
        // the seam (card d41b6fc1). Read, never inserted; a room with no projection
        // hands an empty list and the rule is simply inert.
        let present = super::super::room_roster::present_names(ws.room_id);
        let decision = self.silence_a_parroted_draft(
            decision_from_response(&resp.text, Some(&self.persona_name), &present),
            ws,
        );
        // THE MINDLESS RECEIPT (card aed15611): every verdict counts, and a gate refusal
        // — the substrate silencing a framing echo or a not-speech envelope — counts
        // against her; the hour's share decides whether her seat rests.
        // THE MINDLESS RECEIPT (card aed15611) and THE GATE BECOMES A LESSON (card
        // 657e74de) read the SAME typed fact: a pass the substrate decided, not her.
        let refusal = decision.gate_refusal().map(|(g, m)| (g.to_string(), m.to_string()));
        crate::cognition::health_ledger::note_verdict_of(
            self.persona_id,
            &self.persona_name,
            refusal.is_some(),
        );
        // A framing-echo / not-speech pass on an UNDIRECTED, LIVED turn stages
        // {burst → PASS} as a speech-discipline example. A directed turn never trains
        // silence (a human's question is not a "nothing to say"), a synthetic burst is
        // not her experience, and a PARROTED draft is not staged: its correct completion
        // was speech in her own words, not silence.
        if let Some((gate, _marker)) = refusal.as_ref() {
            let gate = gate.as_str();
            if gate != "parroted_perception"
                && !ws.directed_at_self()
                && ws.cause != super::super::workspace::Cause::Synthetic
            {
                crate::cognition::extensions::produce_speech_discipline(
                    self.persona_id,
                    self.persona_name.clone(),
                    self.binding.load().model.clone().unwrap_or_default(), // unwrap_or_default: an unbound faculty stages under an empty base — the trigger keys buckets by base and files it honestly under ""
                    ws.world_state.clone(),
                    if gate == "framing_echo" { "framing_echo" } else { "not_speech" },
                );
            }
        }
        let (salience, reasoning) = match &decision {
            Decision::Pass { reason } => (
                0.5,
                match reason {
                    Some(r) => format!("{} passed: {r}", self.persona_name),
                    None => format!("{} chose silence (PASS)", self.persona_name),
                },
            ),
            _ => (
                0.85,
                format!(
                    "{} deliberated over the assembled context",
                    self.persona_name
                ),
            ),
        };
        Contribution::verdict(decision, salience, reasoning)
            .with_metrics(metrics_from(&self.persona_name, resp))
            // #210: for a Speak, the raw text IS the artifact (the `<<!DOCTYPE` HTML she
            // wrote to the room / a file); carrying it verbatim lets the glass box show a
            // leading-char fumble is the model's, distinct from the parsed decision.
            .with_raw_generation(resp.text.clone())
    }
}

fn metrics_from(
    persona: &str,
    resp: &TextGenerationResponse,
) -> crate::cognition::workspace::TurnMetrics {
    // The lane's PREFILL-vs-DECODE split (llama-server `timings`), when present:
    // cache_n/prompt_n is the KV-cache hit/miss, prompt_ms/predicted_ms the
    // wall-clock split that lets the harness see where Metal time actually goes.
    // Absent (cloud / older endpoints) → 0, and the breakdown rows read "n/a".
    let t = resp.timing.as_ref();
    let m = crate::cognition::workspace::TurnMetrics {
        input_tokens: resp.usage.input_tokens,
        output_tokens: resp.usage.output_tokens,
        latency_ms: resp.response_time_ms,
        cached_tokens: t.map(|t| t.cached_tokens).unwrap_or(0),
        prefill_tokens: t.map(|t| t.prefill_tokens).unwrap_or(0),
        prefill_ms: t.map(|t| t.prefill_ms.round() as u64).unwrap_or(0),
        decode_ms: t.map(|t| t.decode_ms.round() as u64).unwrap_or(0),
    };

    // THE CACHE-REUSE GLASS BOX — the row whose ABSENCE cost two weeks.
    //
    // Every generation funnels through here, so one probe covers every turn of every
    // citizen with no caller change. Emitted only when the lane actually reported
    // timings (cloud/older endpoints omit them); a fabricated 0.0 hit-rate for a
    // provider that never measured one is a lying receipt, so those stay silent.
    //
    // Why this did not exist and why that mattered: `delib` carried `turn.demand` and
    // `context.render` — how big the prompt WAS — and nothing about what the lane did
    // with it. So a prompt whose prefix we were destroying every turn looked identical
    // in the probe stream to one served entirely from cache. The only way to see it on
    // 2026-08-21 was hand-diffing a citizen's raw prompt captures, which is exactly the
    // manual step a probe exists to delete.
    //
    // What it proves, in one row: `hit_rate` near 0 with a large `prefill_tokens` means
    // the prefix is being invalidated upstream — measured that day at ~306s of wasted
    // re-prefill PER ACT while `--cache-reuse 256` was set and working. After the
    // canonical stable-tier ordering above, the same row is the verification: hit_rate
    // should climb off the floor from the second act of a task onward, WITHOUT anyone
    // re-reading a capture by hand.
    if t.is_some() {
        continuum_base::probe!(
            class = "delib.generate.cache",
            persona = %persona,
            cached_tokens = m.cached_tokens,
            prefill_tokens = m.prefill_tokens,
            // The ratio the humans and the citizens both read. 1.0 = fully warm prefix;
            // near 0 = re-encoding the prompt every act, the inefficiency to attack.
            hit_rate = m.cache_hit_rate(),
            prefill_ms = m.prefill_ms,
            decode_ms = m.decode_ms,
            input_tokens = m.input_tokens,
            output_tokens = m.output_tokens,
            latency_ms = m.latency_ms,
            "prompt cache reuse for this generation"
        );
    }
    m
}
