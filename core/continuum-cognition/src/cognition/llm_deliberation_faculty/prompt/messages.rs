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
        // Every turn is its own item of her view, keyed by when it occurred and who spoke,
        // so a new turn appends one message and the turns before it stay a reused prefix.
        // Content similarity cannot establish event identity: "tests passed" and "tests
        // failed" can be near-identical, and a later identical statement can report
        // another change of state; perception facts describe repetition.
        let mut turns: Vec<crate::cognition::context_streams::conversation::Turn> = Vec::new();
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
        for (index, turn) in ws.turns.iter().enumerate() {
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
                continue;
            }
            // A room turn is its event (unique, and what her read receipt names); a turn without
            // one keys by time and author, an opaque stimulus by its position.
            let key = match (turn.source, turn.occurred_at_ms) {
                (Some(source), _) => source.event.to_string(),
                (None, Some(ms)) => format!("{ms}/{}", turn.author),
                (None, None) => format!("#{index}"),
            };
            let role = if turn.is_self {
                crate::cognition::context_streams::Role::Assistant
            } else {
                crate::cognition::context_streams::Role::User
            };
            turns.push(crate::cognition::context_streams::conversation::Turn { key, role, line, pointer: None });
        }
        // Messages that reached her room during this turn are conversation too, after the
        // burst, keyed by their event: in her view they append like any turn, and one
        // already in the burst keeps its place. They rode the fresh tail as required input
        // under a quarter-of-the-window share (ROOM_UPDATE_SHARE_DENOM), a second budget.
        for update in ws.room_updates.iter() {
            let key = update.event_id.to_string();
            if turns.iter().any(|turn| turn.key == key) {
                continue;
            }
            let line = update.render_room_update();
            turns.push(crate::cognition::context_streams::conversation::Turn { key, role: crate::cognition::context_streams::Role::User, line, pointer: None });
        }
        // One line after them that never collapses: how many arrived and where the rest
        // are. An update her window cannot hold whole then leaves only this, never
        // vanishes unknown; a handle per update would put every one in the conversation's
        // floor and crowd out the newest (40 handles took its room in the busy-room test).
        if let Some(newest) = ws.room_updates.last() {
            let line = format!(
                "[{} room message(s) arrived during this turn; any not shown above: reread with chat/history]",
                ws.room_updates.len()
            );
            turns.push(crate::cognition::context_streams::conversation::Turn {
                key: format!("room-updates/{}", newest.event_id),
                role: crate::cognition::context_streams::Role::User,
                pointer: Some(line.clone()),
                line,
            });
        }
        // What follows the view in this act's prompt: facts, the clock, demoted payloads.
        let mut messages: Vec<ChatMessage> = Vec::new();

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
        let mut facts = perceived;
        facts.extend(super::super::super::perception_facts::render_facts(
            &fact_cx,
            &super::super::super::perception_facts::FactPolicy::default(),
        ));
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
        // newest trailing turn — deliberately NOT routed as a faculty bid. As part of a
        // working-memory faculty's bid (that faculty is deleted) it competed in
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
        // Every grounding block that rides the conversation (the workspace map, the board,
        // the wall, the roster, standing or not) is a source of her view, one block each, in
        // the canonical wire order (churn class, then name) so two blocks never swap places
        // between acts (`deliberation_prompt::stable_prefix_order`). Outside the view a block
        // sat behind it, and every act's append landed before it and re-sent it: on the M5
        // (2026-10-21 19:56Z) Aris re-prefilled 49k chars from `[workspace-map]` on. Her
        // thoughts and actions are not among them: they are their own streams in her view.
        let mut pieces: Vec<&Contribution> = ws
            .broadcast
            .iter()
            .filter(|c| c.decision.is_none() && c.fault.is_none() && c.trailing)
            .filter(|c| !c.content.trim().is_empty())
            .collect();
        pieces.sort_by(|a, b| deliberation_prompt::stable_prefix_order(a.faculty.as_str(), b.faculty.as_str()));
        let blocks: Vec<crate::cognition::context_streams::blocks::Grounding> = pieces
            .into_iter()
            .map(|c| {
                // Rendered by the owner of a block's body, so it reads exactly as it did:
                // whole, and for a standing source also at its truthful minimum, what the
                // flexbox collapses it to when the window is short.
                let render = |piece: GroundingPiece| {
                    let mut text = String::new();
                    piece.append_body(&mut text);
                    text
                };
                let text = render(GroundingPiece::whole(c));
                let minimum = if c.standing_grounding { render(GroundingPiece::minimum(c)) } else { text.clone() };
                crate::cognition::context_streams::blocks::Grounding { persona: self.persona_id, faculty: c.faculty.as_str().to_owned(), text, minimum, units: std::sync::Arc::clone(&c.units) }
            })
            .collect();
        let result_pointers = Vec::new();
        let grounding_at = 0;

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
        let latest_result = match self.working_memory.as_ref() {
            None => Vec::new(),
            Some(wm) => match wm.active_action_full() {
                None => Vec::new(),
                Some((seq, full)) => {
                    let message = wm.full_result_message(seq, &full);
                    // Provenance remains part of request identity. Typed calls and
                    // producer payloads below additionally identify the native exchange.
                    field(&mut input, message.as_bytes());
                    let observations = wm.active_action_observations(seq);
                    let calls = wm.active_action_native_calls(seq);
                    let mut group = if calls.is_empty() {
                        // Legacy receipts have no native IDs to echo. Whole: the one allocation
                        // reduces it by level when it alone exceeds her window
                        // (`fit_newest_result`), never a head cut here (FOCUS-CONTRACT rule 4).
                        vec![ChatMessage::text("user", message)]
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
                            parts.push(crate::ai::types::ContentPart::Text { text: wm.result_label(seq) });
                        }
                        // This entire required group survives fitting or faults together.
                        vec![calls, results]
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
                    group
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
            streams: super::types::Streams {
                conversation: crate::cognition::context_streams::conversation::Conversation { persona: self.persona_id, turns },
                blocks,
            },
            result_pointers,
            grounding_tokens: 0,
            grounding_at,
            stimulus,
            latest_result,
            action_statement,
        }
    }
}
