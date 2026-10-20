use super::*;

impl LlmDeliberationFaculty {
    pub(in super::super) fn messages_with_visual_feedback(
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
        let mut groups: Vec<(&'static str, Vec<(SourceId, String)>)> = Vec::new();
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
        // The system's own words to her (`TurnVoice::Perception`: a [pattern] notice, a
        // board anchor) are perception facts, not conversation. In the conversation they
        // sat AHEAD of the append-only results ring, so each new notice displaced the ring
        // and everything behind it: Benchy's engine reuse fell to the system prompt alone
        // (7,441 tokens) the turn a `[pattern]` notice landed (the M5, 2026-10-18 16:16Z).
        // They render with the facts, after every append-only block, before the ask.
        let mut perceived: Vec<String> = Vec::new();
        let turn_sources = epoch_view::turn_sources(&ws.turns);
        for ((index, turn), source) in ws.turns.iter().enumerate().zip(turn_sources) {
            let role = if turn.is_self { "assistant" } else { "user" };
            let line = turn_message_line_addressed(turn, &participants, &self.persona_name);
            if turn.voice == TurnVoice::Perception && Some(index) != stimulus_index {
                // A fact, like every other fact: not part of the request's input identity, so
                // diagnostic churn never reads as new input to a retained refusal.
                perceived.push(line);
                continue;
            }
            if Some(index) == stimulus_index {
                // The event time and addressed author are required provenance too.
                field(&mut input, line.as_bytes());
                stimulus = Some(ChatMessage::text(role, line));
                after_stimulus = true;
                continue;
            }
            match groups.last_mut() {
                Some((r, lines)) if *r == role && !after_stimulus => lines.push((source, line)),
                _ => groups.push((role, vec![(source, line)])),
            }
            after_stimulus = false;
        }
        // The body: every source that only grows or changes rarely, keyed so her epoch
        // (`epoch_view`) appends only what is new. Conversation, then the results ring, then
        // the trailing grounding; facts, the clock and the ask follow it every act.
        let mut body: Vec<epoch_view::Block> =
            groups.into_iter().map(|(role, items)| epoch_view::Block::turns(role, items)).collect();

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
        let spoken = super::super::super::deliberation_budget::recent_own_speech(
            crate::identity::PeerId::from_uuid(self.persona_id),
            ws.room_id,
        );
        let room_speech = super::super::super::deliberation_budget::recent_room_speech(ws.room_id);
        let fact_cx = super::super::super::perception_facts::FactContext {
            turns: &ws.turns,
            own_speech: &spoken,
            room_speech: &room_speech,
            working_memory: self.working_memory.as_ref(),
            room_id: Some(ws.activity_room_id),
            persona_id: Some(self.persona_id),
        };
        let mut ledger = Vec::new();
        let mut facts = perceived;
        let mut notices = Vec::new();
        for fact in super::super::super::perception_facts::render_facts(
            &fact_cx,
            &super::super::super::perception_facts::FactPolicy::default(),
        ) {
            use super::super::super::perception_facts::FactPlacement;
            match fact.placement {
                FactPlacement::History => facts.push(fact.text),
                FactPlacement::Ledger => ledger.push(fact.text),
                FactPlacement::Notice => notices.push((fact.id, ChatMessage::text("user", fact.text))),
            }
        }
        // The required tail (ask, pinned newest result) is assembled by the fit. Facts stay
        // BEFORE the ask (the 2026-07-20 parrot-loop bisect); the pinned result stays nearest
        // generation (#392); trailing grounding stays out of the system prefix (#205/#2415).
        //
        // THE BODY IS AN EPOCH (docs/architecture/MIND-CONTEXT-AND-ENGINE-CONFIG.md). The
        // order inside it was flipped four times (2026-08-23, 08-24 twice, 10-18) chasing
        // byte-stability between re-renders; each order moved the prefix somewhere else. The
        // epoch appends this act's news after the last act's body instead, so the order here
        // is only the order of a fresh epoch. Saved results carry their compact pointer for
        // the fit; the active result is pinned below, not here.
        if let Some(wm) = &self.working_memory {
            body.extend(wm.recent_results_with_pointers().into_iter().map(epoch_view::Block::from));
        }
        // Trailing proprioception (#205) and trailing grounding: the working-memory trail
        // last, as the churniest standing block.
        let trailing = |wm_trail: bool| {
            ws.broadcast
                .iter()
                .filter(|c| c.decision.is_none() && c.trailing && !c.standing_grounding)
                .filter(move |c| {
                    (c.faculty.as_str() == crate::cognition::working_memory::WM_FACULTY_ID) == wm_trail
                })
                .filter(|c| !c.content.trim().is_empty())
                .map(|c| (c.faculty.as_str(), c.content.as_str()))
        };
        body.extend(epoch_view::contribution_blocks(trailing(false)));
        body.extend(epoch_view::contribution_blocks(trailing(true)));
        let result_pointers = body
            .iter()
            .enumerate()
            .filter_map(|(index, block)| block.alternative().map(|p| (index, ChatMessage::text("user", p))))
            .collect();
        let grounding_at = body.len();
        let mut messages: Vec<ChatMessage> = body.iter().map(epoch_view::Block::message).collect();
        // Facts flicker per act: after the body, immediately before the ask (parrot-fix
        // order). The volatile framing (the [now] clock, the DIRECTED/SILENCE flip) is the
        // same class: in the system message it invalidated the KV prefix on nearly every act
        // (hit_rate 0.0, 2026-09-01).
        for fact in facts {
            messages.push(ChatMessage::text("user", fact));
        }
        if let Some(framing) = trailing_framing {
            messages.push(ChatMessage::text("user", framing.to_string()));
        }
        // PINNED just-executed result (#392): read straight from working memory, never a
        // faculty bid that attention could evict (the 0-byte SWE-bench patch, run-18057-f1).
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
            body,
            headroom: 0,
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
    pub(in super::super) fn split_action_result(&self, message: String) -> (ChatMessage, Option<ChatMessage>) {
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

    pub(in super::super) fn split_room_updates(&self, ws: &Workspace) -> (Vec<ChatMessage>, Vec<ChatMessage>) {
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
}
