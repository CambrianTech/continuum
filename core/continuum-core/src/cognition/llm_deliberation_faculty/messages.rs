//! History as chat messages, and fitting those messages to their budget.

use super::*;

impl LlmDeliberationFaculty {
    /// The share of the served window the REQUIRED room payload may occupy.
    ///
    /// Room updates are required input — a citizen must see what was said to her —
    /// so `fit_messages` may not drop them the way it drops old history. Unbounded,
    /// that makes a BUSY ROOM into a mute citizen: measured 2026-09-09 on BigMama,
    /// `delib.turn.demand` reported `conversation_tokens=53522` against a 23,040
    /// window (`over_window=3.14`). The required side then exceeded the whole
    /// budget, `grounding_budget` saturated to 0, and `GroundingPlan::minimum`
    /// refused 192 of 229 cognition cycles — 84% — with
    /// "requires 810 tokens, but only 0 are available". Both live citizens were
    /// mute for hours while `serving/status` read `ready: true`.
    ///
    /// The window is not the fault and raising it does not fix this: the room grows
    /// without bound and any fixed window is eventually 3x under. What is bounded
    /// here is what the turn REQUIRES; the rest of the room is still reachable
    /// through history and recall.
    const ROOM_UPDATE_SHARE_DENOM: u32 = 4;
    /// The action result's share of the window before its tail is DEMOTED to ordinary
    /// history. Same denominator as the room payload for the same reason: no single
    /// required item may claim more than a quarter of the window, because the moment one
    /// does, every other required item is competing for what is left and the turn refuses.
    pub(super) const RESULT_SHARE_DENOM: u32 = 4;

    /// Build the role-attributed conversation thread from the workspace's
    /// structured turns, fitted to `budget_tokens`. The persona's OWN posts become
    /// `assistant` messages (her own voice — no name prefix, matching the
    /// "do NOT prefix your message with your name" instruction); peers' posts become
    /// `user` messages prefixed `{author}: ` so several speakers stay legible inside
    /// one merged user turn. Consecutive same-role turns merge into a single message
    /// (the chat-template shape models expect). Under budget pressure the OLDEST
    /// turns drop first. This is the payoff of the structured-turns refactor: the
    /// model sees WHO said WHAT with its own past messages attributed to `assistant`,
    /// so it neither bleeds identity nor replays the transcript (the echo-loop root
    /// cause — PERSONA-COGNITION-PIPELINE §7.5).
    #[cfg(test)]
    fn messages_within(&self, ws: &Workspace, budget_tokens: usize) -> Vec<ChatMessage> {
        self.fit_messages(self.messages_unfitted(ws, None), budget_tokens)
            .expect("test workspace must fit its complete current stimulus and action result")
            .messages
    }

    /// The conversation as it stands BEFORE any budget is applied — every turn, every
    /// perception fact, every trailing proprioception block, in final render order.
    ///
    /// Split out from the fitting step so a turn's TRUE conversational size is
    /// knowable. Fitting is lossy by design (newest-first, oldest dropped), and once
    /// it has run the question "how much did this turn actually want?" is
    /// unanswerable — which is precisely why the served window had to be sized by a
    /// constant instead of by demand. [`crate::cognition::working_set`] measures this.
    pub(super) fn messages_unfitted(&self, ws: &Workspace, trailing_framing: Option<&str>) -> PromptMessages {
        self.messages_with_visual_feedback(ws, trailing_framing, None)
    }

    pub(super) fn messages_with_visual_feedback(
        &self,
        ws: &Workspace,
        trailing_framing: Option<&str>,
        visual: Option<&ActiveVisualFeedback>,
    ) -> PromptMessages {
        // Required-input cache identity, not provider wire provenance. Hash
        // borrowed payload once, never JSON, optional history, or generated
        // clock/cycle/diagnostic framing. The existing source-defined minimum
        // keeps large optional lists out of this path; do not hash content and
        // its duplicate parts. Model/window and schema rebuilds key the owner.
        let mut input = Sha256::new();
        let field = |hash: &mut Sha256, bytes: &[u8]| {
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        };
        field(&mut input, self.system_prompt.as_bytes());
        field(&mut input, ws.room_id.as_bytes());
        let standing = || {
            ws.broadcast
                .iter()
                .filter(|c| c.decision.is_none() && c.fault.is_none() && c.standing_grounding)
        };
        input.update(b"standing");
        input.update((standing().count() as u64).to_le_bytes());
        for c in standing() {
            field(&mut input, c.faculty.as_str().as_bytes());
            let piece = GroundingPiece::minimum(c);
            match piece.units {
                None => field(&mut input, c.content.as_bytes()),
                Some(units) => {
                    for part in c.parts.iter().take(units) {
                        field(&mut input, part.as_bytes());
                    }
                }
            }
        }
        input.update(b"room_updates");
        input.update((ws.room_updates.len() as u64).to_le_bytes());
        for update in ws.room_updates.iter() {
            field(&mut input, update.peer_id.as_bytes());
            field(&mut input, update.room_id.as_bytes());
            field(&mut input, update.text.as_bytes());
        }
        // Coalesce consecutive same-role turns in chronological order. Content
        // similarity cannot establish event identity: "tests passed" and "tests
        // failed" can be near-identical, and a later identical statement can
        // report another change of state. Preserve that history for the citizen;
        // the existing fitter bounds it and perception facts describe repetition.
        let mut groups: Vec<(&'static str, Vec<String>)> = Vec::new();
        // Preserve the newest external SPEECH as activity context, even when an
        // own receipt or perception fact follows it. This says what was heard,
        // never what the citizen must obey. Keep the typed boundary before
        // same-role coalescing can bury it inside room history (f09424d4).
        // Source-item/ambient causal provenance remains card 27fdf307; do not
        // infer an instruction or a work assignment from these bytes.
        let stimulus_index = ws
            .turns
            .iter()
            .rposition(|turn| !turn.is_self && turn.voice == TurnVoice::Speech);
        let mut stimulus = None;
        let mut after_stimulus = false;
        // Every display name in the window (peers + self) — the participant set
        // vocative geometry matches against so a message that names its addressee
        // renders `Asha (to Anwen): …` / `(to you)`. Glass-boxed 2026-07-10: a
        // prose-only vocative ("Sure, Anwen. Could you post your implementation…")
        // let the wrong persona answer AS the addressee — identity capture.
        let mut participants: Vec<String> = ws
            .turns
            .iter()
            .filter(|t| !t.author.is_empty())
            .map(|t| t.author.clone())
            .collect();
        participants.push(self.persona_name.clone());
        participants.sort();
        participants.dedup();
        input.update(b"stimulus");
        input.update([stimulus_index.is_some() as u8]);
        for (index, turn) in ws.turns.iter().enumerate() {
            let role = if turn.is_self { "assistant" } else { "user" };
            let line = turn_message_line_addressed(turn, &participants, &self.persona_name);
            if Some(index) == stimulus_index {
                // The event time and addressed author are required provenance too.
                field(&mut input, line.as_bytes());
                stimulus = Some(ChatMessage::text(role, line));
                after_stimulus = true;
                continue;
            }
            match groups.last_mut() {
                Some((r, lines)) if *r == role && !after_stimulus => lines.push(line),
                _ => groups.push((role, vec![line])),
            }
            after_stimulus = false;
        }
        let mut messages: Vec<ChatMessage> = groups
            .into_iter()
            .map(|(role, lines)| ChatMessage::text(role, lines.join("\n")))
            .collect();

        // PERCEPTION FACTS (docs/architecture/PERCEPTION-FACTS.md slice 2b):
        // own-repetition (#134), peer-echo (#152), [context] bounds (#152),
        // and the steps-taken ledger (#151) render through ONE ordered
        // registry — one seam to add a fact, a `perception.fact` probe per
        // fact per tick, and `FactPolicy` toggles as A/B arms. Each fact's
        // full history and doctrine lives on its impl in perception_facts.rs.
        // Facts, never instructions ([[no-hardcoded-heuristics-to-steer-cognition]]).
        //
        // Facts are GROUNDING, not the ask — so they go just BEFORE the final user
        // turn (the actionable message she's answering), never after it. A model
        // continues the LAST thing it sees; when bracketed meta-facts sat last, the
        // model parroted them instead of answering — glass-boxed 2026-07-20: Devstral
        // echoed the [context]/[steps]/working-memory scaffold verbatim into a
        // finish=length loop and scored 0/50 on humaneval-rs because the ask was buried
        // BEFORE the facts. Inserting before the final user turn keeps every property the
        // trailing placement wanted (still the newest grounding, still survives the
        // newest-first budget fit, still adjacent to the reply, still out of the cacheable
        // system prefix) while leaving the ask last, where she answers it. Proven by
        // bisect: ask-last → clean code; facts-last → parrot loop.
        let spoken = crate::cognition::deliberation_budget::recent_own_speech(
            crate::identity::PeerId::from_uuid(self.persona_id),
            ws.room_id,
        );
        let room_speech = crate::cognition::deliberation_budget::recent_room_speech(ws.room_id);
        let fact_cx = crate::cognition::perception_facts::FactContext {
            turns: &ws.turns,
            own_speech: &spoken,
            room_speech: &room_speech,
            working_memory: self.working_memory.as_ref(),
            room_id: Some(ws.activity_room_id),
            persona_id: Some(self.persona_id),
        };
        let mut ledger = Vec::new();
        let mut facts = Vec::new();
        let mut notices = Vec::new();
        for fact in crate::cognition::perception_facts::render_facts(
            &fact_cx,
            &crate::cognition::perception_facts::FactPolicy::default(),
        ) {
            use crate::cognition::perception_facts::FactPlacement;
            match fact.placement {
                FactPlacement::History => facts.push(fact.text),
                FactPlacement::Ledger => ledger.push(fact.text),
                FactPlacement::Notice => notices.push(ChatMessage::text("user", fact.text)),
            }
        }
        // MESSAGE ORDER IS MONOTONE IN STABILITY (2026-08-23, the byte-diff
        // verdict). Consecutive-act prompt captures showed the flickering
        // content — perception facts whose presence/wording changes per act,
        // trailing grounding — rendering AHEAD of the append-only history
        // block, so the 20k+ stable mass never extended the KV prefix and
        // reuse pinned at the system head (~36%). The assembly below is
        // explicit phases, replacing insert-before-ask arithmetic:
        //
        //   old turns → history (append-only) → grounding (flickers)
        //   → facts (flicker) → ASK → pinned newest result
        //
        // Every prior lesson is preserved by construction: facts stay BEFORE
        // the ask and the ask stays after them (the 2026-07-20 parrot-loop
        // bisect); the pinned result stays nearest generation (#392); trailing
        // grounding stays out of the system prefix (#205/#2415). The ask is
        // newest external Speech, retained as a typed stimulus before coalescing
        // and attached after the churn. A later own receipt or perception cannot
        // silently turn it into evictable history.

        // TRAILING proprioception (#205): contributions marked [`Contribution::trailing`]
        // — the working-memory reasoning trail, the FULL most-recent action result,
        // dispatched background handles — render as the NEWEST user content, after the
        // conversation AND the perception facts, so the full result sits nearest
        // generation (most actionable) and every act's growth appends to the very end
        // of the token stream. They are deliberately absent from the system message
        // (see `render_assembled_context_within`), which is what keeps the cacheable
        // system prefix byte-stable across a settle-act instead of re-prefilling the
        // whole tail. Broadcast insertion order preserved (one trailing contributor
        // today: the working-memory faculty).
        // PINNED just-executed result (#392, run-18057-f1). The full result of the act
        // she just ran is appended DIRECTLY from the shared working-memory buffer as the
        // newest trailing turn — deliberately NOT routed as a faculty bid. As part of the
        // working-memory contribution at `WORKING_MEMORY_SALIENCE` it competed in
        // `arbiter.focus()` top-k and was evicted whole under capacity pressure, so the
        // persona generated blind to her own grep/read output (the 0-byte SWE-bench patch).
        // Reading it here puts no attention pass between the hands and the mind.
        // Native images from this exact active receipt join its required text below.
        // Text estimates are only a lower bound for these messages: the adapter's
        // exact image quote admits the finalized body or returns capacity feedback.
        // Trailing (#205): appended last, KV prefix stays stable. Settlement-gated
        // by active_action_full so it stops re-prefilling after settlement (#139/#165).
        // GROUNDING BEFORE THE RING (2026-08-24, wire-diffed on round 706215f4):
        // with the ring append-only mid-settle (the 8/24 eviction fix), each new
        // result APPENDED after grounding DISPLACED the byte-identical
        // workspace-map behind it — four consecutive wire diffs showed the
        // divergence at exactly that seam, re-prefilling the whole 75% tail on a
        // model that cannot KV-shift. Reversed from the 8/23 cut deliberately:
        // that A/B lost because the ring ALSO churned then (per-act eviction),
        // so either order re-prefilled; with the ring extend-only, stable
        // grounding ahead of it extends the prefix, and a grounding CHANGE costs
        // one re-prefill instead of every act paying displacement.
        // STABILITY-ORDERED TAIL, second wire-diff (2026-08-24 evening): the first
        // reorder moved ALL trailing contributions ahead of the ring — including the
        // working-memory TRAIL, which REWRITES itself every act (measured: an 11.5k
        // interior-state block at msg[1] mutating per tick, prefix dead at 2-4%).
        // Split by churn class: non-WM grounding (semi-stable) → append-only ring →
        // the trail (churniest standing block) → facts/ask/pin. One rule, monotone
        // in stability, same law as the system-side phases.
        let grounding_at = messages.len();
        let render_trailing = |messages: &mut Vec<ChatMessage>, wm_trail: bool| {
            // Canonical wire order (churn class, then name) — the same rule the standing
            // grounding and the system's stable tier follow, so two trailing sources can
            // never swap places between acts (`deliberation_prompt::stable_prefix_order`).
            let mut pieces: Vec<&Contribution> = ws
                .broadcast
                .iter()
                .filter(|c| c.decision.is_none() && c.trailing && !c.standing_grounding)
                .filter(|c| {
                    (c.faculty.as_str() == crate::cognition::working_memory::WM_FACULTY_ID)
                        == wm_trail
                })
                .collect();
            pieces.sort_by(|a, b| {
                deliberation_prompt::stable_prefix_order(a.faculty.as_str(), b.faculty.as_str())
            });
            for c in pieces {
                if !c.content.trim().is_empty() {
                    // Same `[faculty]` banner the system block gives its sections —
                    // grounding that moved here for KV reuse (volatile-content
                    // sources, 2026-08-23) must stay as legible as it was in the
                    // system prefix. One sized allocation, no intermediate format!.
                    let mut body =
                        String::with_capacity(c.faculty.as_str().len() + c.content.len() + 4);
                    body.push('[');
                    body.push_str(c.faculty.as_str());
                    body.push_str("]\n");
                    body.push_str(&c.content);
                    messages.push(ChatMessage::text("user", body));
                }
            }
        };
        render_trailing(&mut messages, false);
        let mut result_pointers = Vec::new();
        if let Some(wm) = &self.working_memory {
            // The append-only results ring — its appends land at the growth
            // frontier of the stable region, displacing nothing.
            for (block, pointer) in wm.recent_results_with_pointers() {
                if let Some(pointer) = pointer {
                    result_pointers.push((messages.len(), ChatMessage::text("user", pointer)));
                }
                messages.push(ChatMessage::text("user", block));
            }
        }
        render_trailing(&mut messages, true);

        // Facts flicker per act — they render after every append-only block,
        // immediately before the ask (parrot-fix order preserved).
        for fact in facts {
            messages.push(ChatMessage::text("user", fact));
        }
        // The volatile framing (clock + own-time/presence flip) — same flicker
        // class as the facts, so it rides HERE: before the ask (the parrot-fix
        // order), after everything stable. It lived at the tail of the system
        // message until 2026-09-01, where its per-act mutation (the [now]
        // minute tick, DIRECTED↔SILENCE) invalidated the KV prefix behind it
        // on nearly every act — hit_rate 0.0 with per-resident slots and a
        // server whose reuse measured perfect the same hour.
        if let Some(framing) = trailing_framing {
            messages.push(ChatMessage::text("user", framing.to_string()));
        }
        // Use the actual received action payload. The old pinned block clipped
        // its head before this fitter clipped its tail: the two cuts could leave
        // only a truncation notice. Oversized output paging belongs to the
        // existing spill/tool-output producer, never to a generic byte reducer.
        input.update(b"active_result");
        let (latest_result, demoted_result) = match self.working_memory.as_ref() {
            None => (Vec::new(), None),
            Some(wm) => match wm.active_action_full() {
                None => (Vec::new(), None),
                Some((seq, full)) => {
                    let message = wm.full_result_message(seq, &full);
                    // Provenance remains part of request identity. Typed calls and
                    // producer payloads below additionally identify the native exchange.
                    field(&mut input, message.as_bytes());
                    let observations = wm.active_action_observations(seq);
                    let calls = wm.active_action_native_calls(seq);
                    let (mut group, tail) = if calls.is_empty() {
                        // Legacy receipts have no native IDs to echo.
                        let (head, tail) = self.split_action_result(message);
                        (vec![head], tail)
                    } else {
                        let results: Vec<_> = observations.iter().map(|o| o.output.result.clone()).collect();
                        for observation in &observations {
                            field(&mut input, observation.output.result.tool_use_id.as_bytes());
                            field(&mut input, observation.output.result.content.as_bytes());
                            input.update([match observation.output.result.is_error { None => 0, Some(false) => 1, Some(true) => 2 }]);
                        }
                        for call in &calls {
                            field(&mut input, call.id.as_bytes());
                            field(&mut input, call.name.as_bytes());
                            field(&mut input, call.input.to_string().as_bytes());
                        }
                        let calls = ChatMessage::assistant_tool_use(&calls);
                        let mut results = ChatMessage::tool_results(&results);
                        // Preserve producer-owned whole-or-pointer payloads, never expand
                        // spill handles or reconstruct native protocol from receipt prose.
                        if let crate::ai::types::MessageContent::Parts(parts) = &mut results.content {
                            parts.push(crate::ai::types::ContentPart::Text { text: wm.full_result_message(seq, "") });

                        }
                        // This entire required group survives fitting or faults together.
                        (vec![calls, results], None)
                    };
                    if let Some(visual) = visual.filter(|v| v.seq == seq && !observations.is_empty()) {
                        // Legacy checkpoints can still own pixels without original
                        // native call provenance. Preserve either representation.
                        if let Some(result) = group.last_mut() {
                            let mut parts = match std::mem::replace(&mut result.content, crate::ai::types::MessageContent::Parts(Vec::new())) {
                                crate::ai::types::MessageContent::Text(text) => vec![crate::ai::types::ContentPart::Text { text }],
                                crate::ai::types::MessageContent::Parts(parts) => parts,
                            };
                            for (hash, part) in &visual.parts {
                                field(&mut input, hash.as_bytes());
                                match part {
                                    crate::ai::types::ContentPart::Text { text } => field(&mut input, text.as_bytes()),
                                    _ => field(&mut input, b"native-image"),
                                }
                                parts.push(part.clone());
                            }
                            result.content = crate::ai::types::MessageContent::Parts(parts);
                        }
                    }
                    (group, tail)
                }
            },
        };

        // An empty conversation is a legitimate state (a quiet room on a
        // self-initiated tick): the situation lives in the system prompt's assembled
        // context, not in a conversation turn. Adapters still require ≥1 message, so
        // emit the rendered world-state projection as one user message — the exact
        // single-message shape the faculty sent before this refactor. NOT a fallback
        // hiding a defect ([[fallbacks-are-illegal-fail-loud]]): zero turns is a
        // real, valid input, and this is its faithful representation.
        if messages.is_empty() && stimulus.is_none() && latest_result.is_empty() {
            field(&mut input, ws.world_state.as_bytes());
            stimulus = Some(ChatMessage::text("user", ws.world_state.clone()));
        }
        input.update([(!latest_result.is_empty()) as u8]);
        // Demoted room updates ride in ordinary history: still shown when the window
        // has room, trimmed by the existing front-drop only under pressure. They are
        // chronologically newer than prior history, so they append to its end.
        let (room_updates, demoted_updates) = self.split_room_updates(ws);
        let mut messages = messages;
        // The action result's demoted TAIL rides the same way, and goes in FIRST: it is
        // the outcome of the act that preceded these room updates, so appending it before
        // them keeps history in the order it happened. Without this the tail would be
        // silently dropped, which is the discarding this whole split exists to avoid.
        messages.extend(demoted_result);
        messages.extend(demoted_updates);
        let action_statement = self
            .working_memory
            .as_ref()
            .and_then(|memory| memory.action_statement(ws.activity_room_id))
            .map(|text| ChatMessage::text("assistant", text));
        if let Some(statement) = &action_statement {
            field(&mut input, statement.content_text().as_bytes());
        }
        PromptMessages {
            input_identity: input.finalize().into(),
            history: messages,
            result_pointers,
            grounding_tokens: 0,
            grounding_at,
            stimulus,
            latest_result,
            action_statement,
            room_updates,
            notices,
            ledger: ledger.into_iter().next().map(|text| ChatMessage::text("user", text)),
        }
    }

    /// Split this turn's room updates into the part that is REQUIRED and the part
    /// that merely wants to be there. Nothing is discarded.
    ///
    /// Room updates are required input — `required_tokens` counts them and
    /// `fit_messages` never drops them — so an unbounded room is a citizen who can
    /// no longer think. But simply CUTTING the excess would be worse than the bug:
    /// a task, a correction, or an instruction that arrived earlier in the batch
    /// would be deleted before she ever read it, and she would have no way to know
    /// something was taken.
    ///
    /// So the overflow is DEMOTED, not dropped. The newest share stays required; the
    /// older remainder is returned to be appended to ordinary history, where it is
    /// still shown whenever the window has room and is trimmed by the existing
    /// front-drop only under real pressure — the same treatment every other past
    /// message gets. The information survives; only its claim on the required budget
    /// does not.
    ///
    /// Returns `(required_newest, demoted_older)`, both chronological.
    /// Split the action result into the REQUIRED head and a DEMOTED tail.
    ///
    /// The result of the citizen's last act is required input — she cannot reason about
    /// what her hands just did if it is dropped — and it was also UNBOUNDED. A single
    /// `code/read` or `code/shell` output therefore claimed the whole window as
    /// irreducible payload, and `fit_messages` refused the turn, and she went mute while
    /// holding her card.
    ///
    /// Measured 2026-09-10 on a 23,040-token lane: `required_payload = 18,629` against
    /// `after_reserves = 15,688`, over by 2,941, with `grounding_tokens = 113` proving
    /// grounding innocent and `received = 2` proving it was ONE enormous message. 836
    /// consecutive refusals, zero acts, zero inference, for the whole of a benchmark
    /// round. The build had already bounded room updates (#3947) and stopped grounding
    /// vetoing the turn (#3954) — this was the same class, one field over.
    ///
    /// The code here used to say oversized output "belongs to the existing
    /// spill/tool-output producer, never to a generic byte reducer". That is a good rule
    /// about WHO SHOULD SHRINK IT, and it was enforced nowhere: nothing checked that the
    /// producer had, so an unspilled result walked straight into required input.
    ///
    /// DEMOTED, never discarded (Joel, 2026-09-09: "maybe trim clipped or killed
    /// important information the persona needed to complete things"). The head stays
    /// required so the act's outcome — status, error, the first lines that say what
    /// happened — is always in front of her; the tail becomes ordinary history, still
    /// rendered whenever the window allows, and still re-readable through its spill
    /// handle. A cut on a CHAR boundary would split a UTF-8 sequence, so the split walks
    /// to a line boundary and says how much moved.
    pub(super) fn split_action_result(&self, message: String) -> (ChatMessage, Option<ChatMessage>) {
        let window = self.binding.load().context_window as usize;
        let cap = window / Self::RESULT_SHARE_DENOM as usize;
        let whole = ChatMessage::text("user", message.clone());
        if Self::messages_cost(std::slice::from_ref(&whole)) <= cap {
            return (whole, None);
        }
        // WHICH HALF IS REQUIRED IS DECIDED BY WIRE ORDER, not by preference.
        //
        // History renders BEFORE the required slot (`prompt.history.extend(latest_result)`
        // appends the required part last). So keeping the HEAD required would place the
        // result's beginning AFTER its middle on the wire — she would read it backwards.
        // Keeping the TAIL required puts the demoted beginning in history and the ending
        // in the required slot, which is the original order, and it keeps the payload
        // ending byte-for-byte where the existing wire test expects it.
        //
        // Walk lines from the END while the tail still fits its share.
        let lines: Vec<&str> = message.split_inclusive('\n').collect();
        let mut take = 0usize;
        let mut tail = String::new();
        for line in lines.iter().rev() {
            let candidate = format!("{line}{tail}");
            let cost = Self::messages_cost(std::slice::from_ref(&ChatMessage::text(
                "user",
                candidate.clone(),
            )));
            // The LAST line is always required even if it alone exceeds the share: a
            // required slot holding none of her result tells her nothing about what her
            // hands just did, and a genuinely impossible payload is the existing capacity
            // fault's job to report honestly.
            if take == 0 || cost <= cap {
                tail = candidate;
                take += 1;
            } else {
                break;
            }
        }
        if take >= lines.len() {
            return (whole, None);
        }
        let head: String = lines[..lines.len() - take].concat();
        let head_msg = ChatMessage::text("user", head);
        let tail_msg = ChatMessage::text("user", tail);
        crate::probe!(
            class = "delib.action_result.demoted",
            persona = %self.persona_name,
            window = window,
            cap = cap,
            required_tokens = Self::messages_cost(std::slice::from_ref(&tail_msg)),
            demoted_tokens = Self::messages_cost(std::slice::from_ref(&head_msg)),
            "the action result exceeded its share of the window — its ending stays              REQUIRED and the earlier part was demoted to ordinary history, still rendered              when the window allows and still re-readable via its spill handle. Nothing              was discarded: an unbounded required result is what makes one big tool output              a mute citizen."
        );
        (tail_msg, Some(head_msg))
    }

    pub(super) fn split_room_updates(&self, ws: &Workspace) -> (Vec<ChatMessage>, Vec<ChatMessage>) {
        let window = self.binding.load().context_window as usize;
        let cap = window / Self::ROOM_UPDATE_SHARE_DENOM as usize;
        let mut required: Vec<ChatMessage> = Vec::new();
        let mut demoted: Vec<ChatMessage> = Vec::new();
        let mut used = 0usize;
        // Newest first so the REQUIRED half is the freshest room state.
        for message in ws.room_updates.iter().rev() {
            let rendered = ChatMessage::text("user", message.render_room_update());
            let cost = Self::messages_cost(std::slice::from_ref(&rendered));
            // The newest update is always required, even if it alone exceeds the
            // share: one oversized message must not silently demote the very
            // stimulus the turn exists to answer. If it genuinely cannot fit, the
            // existing capacity fault says so honestly.
            if required.is_empty() || used.saturating_add(cost) <= cap {
                used += cost;
                required.push(rendered);
            } else {
                demoted.push(rendered);
            }
        }
        required.reverse();
        demoted.reverse();
        if !demoted.is_empty() {
            let demoted_tokens = Self::messages_cost(&demoted);
            crate::probe!(
                class = "delib.room_updates.demoted",
                persona = %self.persona_name,
                window = window,
                cap = cap,
                required = required.len(),
                required_tokens = used,
                demoted = demoted.len(),
                demoted_tokens = demoted_tokens,
                "required room payload exceeded its share of the window — the newest                  stay REQUIRED and the older were demoted to ordinary history, which                  is still rendered when the window allows. Nothing was discarded: an                  unbounded required payload is what makes a busy room a mute citizen."
            );
        }
        (required, demoted)
    }

    /// The per-message chat-template overhead every message pays, whether it is being
    /// measured for demand or fitted to a budget. ONE constant so the two cannot drift
    /// — an under-count here is an llama-server 400 at the window edge.
    ///
    /// Chat templates wrap each message in role markers
    /// (`<|im_start|>role\n…<|im_end|>`, ~4-5 tokens). The original +2 was optimistic:
    /// at the budget edge the fitted thread measured over the window by a token
    /// (glass-boxed 2026-07-13). Round UP like every other estimate here.
    // context-budget-exempt: fixed chat-template overhead per message (role tags + separators) — a property of the prompt FORMAT, not a budget that should scale with the window
    pub(super) const PER_MESSAGE_TEMPLATE_TOKENS: usize = 5;

    pub(super) fn context_piece_tokens(faculty: &str, body_bytes: usize) -> usize {
        // The actual rendered shape is "\n[faculty]\n<body>\n".
        (faculty.len() + body_bytes + "\n[]\n\n".len()).div_ceil(GUARD_CHARS_PER_TOKEN)
    }

    pub(super) fn working_context_header_cost() -> usize {
        deliberation_prompt::WORKING_CONTEXT_HEADER
            .len()
            .div_ceil(GUARD_CHARS_PER_TOKEN)
    }

    pub(super) fn framing_cost(system: &str) -> usize {
        // System is a message too; the template also opens the assistant's
        // generation turn. Charge both wrappers using the same format allowance.
        est_tokens(system) + 2 * Self::PER_MESSAGE_TEMPLATE_TOKENS
    }

    /// How many MESSAGES a cluster walk may cross — how far the front may advance past a
    /// broken cluster, and how deep the fill cap looks back for her own last move. A
    /// count of messages, not of tokens or seconds: there is nothing here to measure.
    pub(super) const CLUSTER_SCAN_MESSAGES: usize = 6;

    /// HER OWN move: an assistant turn.
    pub(super) fn is_own_move(message: &ChatMessage) -> bool {
        message.role == "assistant"
    }

    /// The OUTCOME of one of her moves: a tool-result / receipt block.
    pub(super) fn is_move_result(message: &ChatMessage) -> bool {
        let body = message.content_text();
        body.starts_with("Full result of") || body.starts_with('⚙')
    }

    /// Does this message CONTINUE a cluster rather than open one? One owner for the
    /// predicate: the evictor uses it so a window never opens on an answer without its
    /// question, and [`PromptMessages::recent_move_evidence`] uses it to find the run at
    /// the END that must not be evicted at all.
    pub(super) fn continues_cluster(message: &ChatMessage) -> bool {
        Self::is_own_move(message) || Self::is_move_result(message)
    }

    /// What this conversation costs whole, with no budget applied — the conversational
    /// half of a turn's demand ([`crate::cognition::working_set`]).
    pub(super) fn messages_cost(messages: &[ChatMessage]) -> usize {
        messages.iter().map(Self::message_cost).sum()
    }

    pub(super) fn message_cost(message: &ChatMessage) -> usize {
        // Text-only lower bound, calibrated by the provider's measured prompt
        // count. Never count base64 characters as image tokens. Native images
        // require the finalized-body image quote before local generation.
        let tokens = crate::cognition::token_budget::estimate_message_content_tokens(&message.content, est_tokens);
        tokens + Self::PER_MESSAGE_TEMPLATE_TOKENS
    }

    /// The window-start advance QUANTUM divisor: when the conversation outgrows
    /// its budget, the front is dropped in chunks of `budget/8` (floored at 512
    /// tokens) rather than message-by-message. See [`Self::fit_messages`].
    // context-budget-exempt: a stability/latency trade ratio, not a token budget — the actual chunk derives from the live budget at the call
    const FRONT_DROP_QUANTUM_DIVISOR: usize = 8;

    /// Fit an already-built conversation to `budget_tokens` — with a
    /// QUANTIZED, whole-message front drop, because the fit's cut point is the
    /// byte-stability of the entire conversation prefix.
    ///
    /// The old shape walked NEWEST-first, spending the budget backward: correct
    /// per-act, and a KV catastrophe across acts — every act appends new tokens
    /// at the tail, so the exact-fit cut point advanced the window START by a
    /// few messages per act, and the straddling message was tail-trimmed to a
    /// different byte offset each time. Measured 2026-09-01 (wire-capture
    /// diffs): consecutive prompts diverging at ~1% depth on exactly this seam,
    /// re-prefilling the other 99%.
    ///
    /// Now the drop is quantized: the minimum front mass that must go is
    /// rounded UP to a chunk (`budget/8`, ≥512 tokens), whole messages only, so
    /// the window start stays BYTE-IDENTICAL for ~a chunk's worth of new
    /// conversation and then advances once, in one jump. The cost is a window
    /// that runs up to one chunk under-full — a deliberate stability/capacity
    /// trade, same law as the scratch slot: reuse of a 20-80k prefix is worth
    /// vastly more than the last few k of oldest history
    /// ([[collapse-dont-clip-condense-the-past-never-erode-it]] — and when the
    /// jump does land, it drops whole turns, never a mid-message shear).
    pub(super) fn fit_messages(
        &self,
        mut prompt: PromptMessages,
        budget_tokens: usize,
    ) -> Result<FittedMessages, PromptCapacityError> {
        let required_tokens = prompt.required_tokens();
        // A quiet self-tick still needs one complete situation message. It has
        // no external stimulus to protect, but returning zero messages is not a
        // valid provider request. Preserve its newest whole context or fault.
        let ambient_tokens = if required_tokens == 0 {
            match prompt.history.last() {
                Some(message) => Self::message_cost(message),
                None => 0, // no history and no required payload: assembly supplies the quiet world-state
            }
        } else {
            0
        };
        let minimum_tokens = required_tokens + ambient_tokens;
        if minimum_tokens > budget_tokens {
            return Err(PromptCapacityError {
                required_tokens: minimum_tokens,
                budget_tokens,
            });
        }
        let history_budget = budget_tokens - required_tokens;
        // 82416bb1: during an ongoing concern WM's ring only appends. Before
        // dropping a result AND its expansion handle, price the source's compact
        // alternative. No second store or mutation of WM: custody stays with the
        // existing saved result, and only the fitted view changes. Unsaved results
        // never supply an alternative. Ordinary eviction still bounds this view.
        let mut history_tokens = Self::messages_cost(&prompt.history);
        for (index, pointer) in std::mem::take(&mut prompt.result_pointers) {
            let Some(message) = prompt.history.get_mut(index) else { continue };
            let old_cost = Self::message_cost(message);
            let pointer_cost = Self::message_cost(&pointer);
            if history_tokens > history_budget && pointer_cost < old_cost {
                history_tokens = history_tokens.saturating_sub(old_cost - pointer_cost);
                *message = pointer;
            } else {
                prompt.result_pointers.push((index, pointer));
            }
        }
        // Read BEFORE the history is borrowed for costing: what her own last move and its
        // outcome cost, so the drop below can refuse to reach them.
        let evidence = prompt.recent_move_evidence();
        let messages = &prompt.history;
        // Per-message template overhead: `Self::PER_MESSAGE_TEMPLATE_TOKENS`, shared
        // with `messages_cost` so measurement and fitting charge identically.
        let costs: Vec<usize> = messages.iter().map(Self::message_cost).collect();
        let total: usize = costs.iter().sum();
        let mut start = 0usize;
        let quantum = (budget_tokens / Self::FRONT_DROP_QUANTUM_DIVISOR).max(512);
        if total > history_budget {
            let min_drop = total - history_budget;
            let drop_q = min_drop.div_ceil(quantum).saturating_mul(quantum);
            let mut dropped = 0usize;
            // Only optional history yields. Stimulus and action payload are typed
            // slots outside this eviction range, even when both render as user.
            let drop_end = if required_tokens == 0 {
                messages.len().saturating_sub(1)
            } else {
                messages.len()
            };
            // HER OWN LAST MOVE IS THE LAST THING TO GO, never the first.
            //
            // Front-dropping is already recency-ordered, so the oldest turns yield first
            // — but HOW FAR the drop reaches is decided by cost alone, and a tight fill
            // budget walks it right up to the run at the end: her act, its result, and
            // the turn that asked for it. That run is the evidence that she ALREADY DID
            // THE THING. Evict it and she re-issues the same act — which the M5 records
            // as `persona.act.repeat_short_circuited` ("identical act already satisfied
            // this turn"), then `persona.act.no_deliverable_yet`, then an hour line with
            // zero writes that reads as a lazy persona.
            //
            // Protected whenever the budget can hold it at all. When it CANNOT, nothing
            // here can save it — stopping short would put the assembled prompt over the
            // window, which mutes her for the whole tick — but it is never silent: a
            // `CannotFit` is an outcome this code declares and names, with what was lost.
            //
            // `min`, not `saturating_sub`: the quiet-tick branch above already holds the
            // LAST message back, and that message is inside the evidence suffix —
            // subtracting would reserve one extra old turn and leave the fit over budget.
            let evidence_fit = evidence.fit_within(history_budget);
            if evidence_fit == EvidenceFit::CannotFit {
                // A REFUSAL, NOT A QUIET DROP (Astra's review of #4290, and her test
                // `continuity_insufficient_capacity_refuses_instead_of_dropping_evidence`).
                //
                // This branch is only reached when every other optional turn has already
                // yielded and her act and its result STILL do not fit. Continuing here
                // would forget a completed act and let her re-issue it, with a probe as
                // the only trace — "required action/result continuity must not become
                // optional when it exceeds capacity". So it is a capacity error, the same
                // outcome the irreducible stimulus and `latest_result` already produce,
                // on the same path the caller already handles.
                //
                // It is loud in both directions: the probe names what did not fit, and the
                // error carries the real requirement, which is the number the untruncated
                // demand receipt feeds to `serving_plan` — so a window too small for one
                // act gets PROVISIONED rather than silently trimmed forever.
                crate::probe!(
                    class = "delib.fill.evidence_cannot_fit",
                    persona = %self.persona_name,
                    verdict = evidence_fit.as_str(),
                    evidence_messages = evidence.messages,
                    evidence_tokens = evidence.tokens,
                    largest_message_tokens = evidence.largest_message,
                    history_budget,
                    truncated_chain = evidence.truncated,
                    "her most recent act and its result do NOT fit the conversation \
                     budget — refusing the turn rather than forgetting a completed act. \
                     The window is too small for a turn of this shape"
                );
                return Err(PromptCapacityError {
                    required_tokens: required_tokens.saturating_add(evidence.tokens),
                    budget_tokens,
                });
            }
            if evidence_fit == EvidenceFit::TooLargeToCarry {
                // Loud, but NOT a refusal — see [`EvidenceFit::TooLargeToCarry`]. The
                // oversized member yields like any other optional turn and the rest of
                // the drop proceeds; refusing here is the 836-refusals outage.
                crate::probe!(
                    class = "delib.fill.evidence_cannot_fit",
                    persona = %self.persona_name,
                    verdict = evidence_fit.as_str(),
                    evidence_messages = evidence.messages,
                    evidence_tokens = evidence.tokens,
                    largest_message_tokens = evidence.largest_message,
                    history_budget,
                    truncated_chain = evidence.truncated,
                    "one message of her last act is on its own larger than the whole \
                     conversation budget — it cannot be carried whole by any policy, so \
                     it yields rather than muting the turn. The answer is condensing it, \
                     never a capacity fault"
                );
            }
            let drop_end = if evidence_fit == EvidenceFit::Protected {
                drop_end.min(messages.len().saturating_sub(evidence.messages))
            } else {
                drop_end
            };
            while start < drop_end && dropped < drop_q {
                dropped += costs[start];
                start += 1;
            }
            // CLUSTERS ARE THE EVICTION GRANULARITY (Joel 2026-09-01:
            // "information flows in groups … a room, code we're looking at,
            // any context that flows semantically together"). A window that
            // opens mid-cluster — an assistant answer without its question, a
            // tool result without its act — costs tokens while carrying broken
            // meaning, worse than dropping the fragment too. Advance the start
            // past continuation-shaped messages to a clean opener. Structural
            // v1 (no model call): an assistant turn or a result/receipt block
            // continues a cluster; a plain user turn opens one. Bounded so a
            // pathological run can't eat the window; the causal-thread work
            // (ladder rung 4) replaces this with true causal subtrees.
            let mut opener_advance = 0usize;
            while start + 1 < messages.len()
                && start < drop_end
                && opener_advance < Self::CLUSTER_SCAN_MESSAGES
                && Self::continues_cluster(&messages[start])
            {
                start += 1;
                opener_advance += 1;
            }
        }
        let receipt = FitReceipt {
            history_messages: costs.len(),
            history_tokens: total,
            kept_messages: costs.len() - start,
            dropped_messages: start,
            dropped_tokens: costs[..start].iter().sum(),
            required_tokens,
            history_budget,
            quantum,
        };
        // Move the surviving messages; fitting never clones the entire prompt.
        prompt.history.drain(..start);
        let grounding_at = prompt.grounding_at.saturating_sub(start);
        let refit = PromptRefit {
            history_len: prompt.history.len(),
            grounding_at,
            grounding_len: 0,
            result_pointers: prompt.result_pointers.into_iter()
                .filter_map(|(index, pointer)| index.checked_sub(start).map(|i| (i, pointer)))
                .collect(),
        };
        // The steps ledger changes on every act (a new action at its tail, an old one aged
        // out at its head). In a multi-act turn it rides after everything append-only
        // across the acts and just before the pinned result, which stays last (card
        // 5b09111e: on the M5 it sat ahead of the framing, the ask and 18 room updates that
        // were byte-identical between acts, and forfeited all of them). With no pinned
        // result it keeps its place ahead of the ask: bracketed meta after the ask gets
        // answered and parroted in its place (the 2026-07-20 humaneval fix).
        if !prompt.latest_result.is_empty() {
            prompt.history.extend(prompt.stimulus);
            prompt.history.extend(prompt.room_updates);
            prompt.history.extend(prompt.ledger);
            prompt.history.extend(prompt.notices);
            prompt.history.extend(prompt.action_statement);
            prompt.history.extend(prompt.latest_result);
        } else {
            prompt.history.extend(prompt.action_statement);
            prompt.history.extend(prompt.ledger);
            prompt.history.extend(prompt.notices);
            prompt.history.extend(prompt.stimulus);
            prompt.history.extend(prompt.room_updates);
        }
        Ok(FittedMessages {
            messages: prompt.history,
            grounding_at,
            receipt,
            refit,
        })
    }

    /// Reduce only the already admitted optional snapshot after an exact quote.
    /// The conversion proposes a reduction, not a tokenizer guarantee; the adapter
    /// must quote the resulting request again. Each successful correction consumes
    /// a whole optional message or its one saved-result representation.
    pub(super) fn refit_rejected_prompt(
        &self,
        mut view: DeliberationPromptView,
        measured: u32,
        available: u32,
        reply: u32,
    ) -> Result<DeliberationPromptView, PromptCapacityError> {
        let old_estimate = view.estimated_prompt_tokens;
        let old_reductions = view.refit.remaining_reductions();
        let refusal = || PromptCapacityError {
            required_tokens: (measured as usize).saturating_add(reply as usize),
            budget_tokens: available as usize,
        };
        if measured == 0 || old_estimate == 0 || old_reductions == 0 {
            return Err(refusal());
        }
        // Local admission requires prompt + reply < window (serving_guard).
        // The additional token represents that strict boundary, not a safety fudge.
        let deficit = (measured as usize).saturating_add(reply as usize)
            .saturating_sub((available as usize).saturating_sub(1));
        let reduction = ((deficit as u128 * old_estimate as u128)
            .div_ceil(measured as u128)).min(usize::MAX as u128) as usize;
        if reduction == 0 { return Err(refusal()); }
        let input_budget = old_estimate.saturating_sub(reduction);
        let fixed_tokens = old_estimate.saturating_sub(Self::messages_cost(&view.messages));
        let grounding: Vec<_> = view.messages.drain(
            view.refit.grounding_at..view.refit.grounding_at + view.refit.grounding_len
        ).collect();
        let required = view.messages.split_off(view.refit.history_len);
        let frozen_grounding_tokens = Self::messages_cost(&grounding);
        let prompt = PromptMessages {
            input_identity: view.input_identity,
            history: view.messages,
            result_pointers: view.refit.result_pointers,
            grounding_tokens: 0,
            grounding_at: view.refit.grounding_at,
            stimulus: None,
            latest_result: required,
            action_statement: None,
            room_updates: Vec::new(),
            ledger: None,
            notices: Vec::new(),
        };
        let mut fitted = self.fit_messages(prompt, input_budget
            .saturating_sub(fixed_tokens).saturating_sub(frozen_grounding_tokens))?;
        if fitted.refit.remaining_reductions() >= old_reductions {
            return Err(refusal());
        }
        fitted.refit.grounding_len = grounding.len();
        fitted.messages.splice(fitted.grounding_at..fitted.grounding_at, grounding);
        let estimate = fixed_tokens.saturating_add(Self::messages_cost(&fitted.messages));
        if estimate >= old_estimate { return Err(refusal()); }
        view.messages = fitted.messages;
        view.refit = fitted.refit;
        view.estimated_prompt_tokens = estimate;
        view.segments = segment_map(&view.system, &view.messages);
        view.capacity_error = None;
        Ok(view)
    }
}
