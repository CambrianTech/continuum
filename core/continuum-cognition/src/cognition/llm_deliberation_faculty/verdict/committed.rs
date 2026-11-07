use super::*;

impl LlmDeliberationFaculty {
    /// Interpret the completed response; dispatch and refitting stay in contribute.
    pub(super) fn committed_verdict(&self, resp: &TextGenerationResponse, ws: &Workspace) -> Option<Contribution> {
        // Did she choose to act? Two shapes, both → `Decision::Act`:
        //  (a) the adapter returned a native tool-use turn (FinishReason::ToolUse);
        //  (b) the model emitted a tool call as JSON in its prose (small models that
        //      ignore the native tool channel) — strictly better than dropping it.
        if !self.tools.is_empty() {
            if matches!(resp.finish_reason, FinishReason::ToolUse) {
                let mut calls = resp.tool_calls.clone().unwrap_or_default();
                // Wire dialect → canonical command names, BEFORE the
                // authorization gate and the executor ever see them (the
                // reverse half of the offer-side rename above).
                for c in &mut calls {
                    crate::ai::tool_dialect::normalize_call(c);
                }
                if let Some(v) = self.yield_verdict(&calls) {
                    return Some(v);
                }
                if !calls.is_empty() {
                    return Some(self.act_verdict(calls, resp, ws.activity_room_id));
                }
            }
            if let Some(mut call) = crate::ai::json_in_prompt_tools::parse_tool_call(&resp.text) {
                if let Some(v) = self.yield_verdict(std::slice::from_ref(&call)) {
                    return Some(v);
                }
                // Same wire-dialect mapping as the native path above (#159): a model
                // that narrates `write_file(…)` / `list_files(…)` — its trained
                // OpenHands vocabulary — must resolve to `code/write` / `code/list`,
                // not silently no-op as an unknown name. The text-lift path skipped
                // this, so narrated snake_case verbs died while the SAME name in a
                // native tool_call worked. One mapping, both paths.
                crate::ai::tool_dialect::normalize_call(&mut call);
                return Some(self.act_verdict(vec![call], resp, ws.activity_room_id));
            }
            // #159 fail-loud: she emitted the native `[TOOL_CALLS]` marker but named no
            // valid tool — `[recall]`/`[action]` are receipt vocabulary she MIMICS as a
            // call (#158), not something callable. Silently rendering the bogus attempt
            // as SPEECH gives her zero feedback → she never switches to `code/search` and
            // rambles to the deadline (`acts:0`, glass-boxed 2026-07-16). Route the
            // attempted name as an `Act` so the executor's unknown-command TEACHER fires
            // as an observation and `drive_to_settle` hands her another generation to do
            // it right. Bounded by `max_acts`; an identical repeat short-circuits via
            // `all_calls_already_satisfied`. [[unknown-tool-intent-must-fail-loud]]
            if let Some(attempted) =
                crate::ai::json_in_prompt_tools::attempted_tool_name(&resp.text)
            {
                let call = crate::ai::types::ToolCall {
                    id: "tool-attempt".to_string(),
                    name: crate::ai::tool_dialect::from_wire_name(&attempted).to_string(),
                    input: serde_json::json!({}),
                };
                return Some(self.act_verdict(vec![call], resp, ws.activity_room_id));
            }
            // The sibling gap the block above cannot see (#159 follow-up): she fenced correct
            // ARGUMENTS and never named the tool, in any liftable position. `attempted_tool_name`
            // gates on the `[TOOL_CALLS]` marker as its first check, and there is no marker here,
            // so this emission produced no call AND no feedback — a silent drop. Measured live
            // 2026-08-07 (Sahar): "I will release the card 392bc54e" + a bare `{"card_id": …}`
            // fence, twice in one turn, then the same shape again next generation. Nothing lifted,
            // correctly — binding prose intent would also fire on peer coaching (#144) — but
            // nothing TAUGHT either, which is the whole reason #159 exists.
            //
            // REPORTED, never executed: we cannot know which tool she meant, and guessing is
            // exactly the false positive the coaching negatives guard. Routing the sentinel makes
            // the executor's teacher fire with the missing-name sentence, and `drive_to_settle`
            // hands her another generation — the same mechanism, extended to the case it missed.
            if let Some(snippet) = crate::ai::json_in_prompt_tools::nameless_args_fence(&resp.text)
            {
                let call = crate::ai::types::ToolCall {
                    id: "tool-attempt-nameless".to_string(),
                    name: crate::cognition::tool_executor::command_executor::NAMELESS_ARGS_SENTINEL
                        .to_string(),
                    input: serde_json::json!({ "emitted": snippet }),
                };
                return Some(self.act_verdict(vec![call], resp, ws.activity_room_id));
            }
            // Reasoning-channel intent lift (#181 sibling, glass-boxed on the
            // deepseek4 eval battery 2026-08-03): a thinking model commits its act
            // into `reasoning_content` — "I'll read the file: {tool_call…}" — and
            // leaves `content` EMPTY (budget exhausted mid-think, or the act simply
            // landed in the wrong channel). Every check above reads only the content
            // channel, so the turn settled as Speak("") — a guaranteed empty fail
            // (2 of 3 real failures in that battery). When content is empty there is
            // no committed answer to respect, so the reasoning tail is the model's
            // only stated intention: run the SAME format stack over it and lift the
            // LAST parseable call (the final intention — earlier matches may be
            // considered-and-discarded exploration). Strictly gated on empty content:
            // a real Speak or an explicit PASS is a commitment in the answer channel
            // and private deliberation must never override it.
            if resp.text.trim().is_empty() {
                // A thought CUT at the output limit chose nothing: the last call in its tail
                // is what she was weighing when the cap landed, never a commitment. Lift
                // nothing; tell her it was cut and let her decide (BigMama's 5090 receipt,
                // 2026-10-06: a half-formed code/run lifted from a cut 8,504-token think).
                if matches!(resp.finish_reason, FinishReason::Length)
                    && !self.tools.is_empty()
                    && resp.reasoning.as_deref().is_some_and(|r| !r.trim().is_empty())
                {
                    continuum_base::probe!(
                        class = "persona.act.thought_cut",
                        persona = %self.persona_name,
                        reasoning_len = resp.reasoning.as_deref().map_or(0, str::len),
                        "the generation reached the output limit mid-thought: nothing lifted, the decision is hers"
                    );
                    let call = crate::ai::types::ToolCall {
                        id: "tool-attempt-thought-cut".to_string(),
                        name: crate::cognition::tool_executor::command_executor::THOUGHT_CUT_SENTINEL
                            .to_string(),
                        input: serde_json::json!({}),
                    };
                    return Some(self.act_verdict(vec![call], resp, ws.activity_room_id));
                }
                if let Some(reasoning) = resp.reasoning.as_deref() {
                    if let Some(mut call) =
                        crate::ai::json_in_prompt_tools::parse_tool_calls(reasoning)
                            .into_iter()
                            .last()
                    {
                        crate::ai::tool_dialect::normalize_call(&mut call);
                        continuum_base::probe!(
                            class = "persona.act.reasoning_lift",
                            persona = %self.persona_name,
                            tool = %call.name,
                            "content channel empty — lifted the final tool intention from the reasoning tail"
                        );
                        return Some(self.act_verdict(vec![call], resp, ws.activity_room_id));
                    }
                    // THINK-ONLY turn (#181 tail, glass-boxed on the 2026-08-15 bench
                    // round): she spent the ENTIRE generation inside the reasoning
                    // channel and stopped — no answer, no PASS, no liftable call
                    // anywhere in the thinking (the captured turn analyzed all 12
                    // tasks lucidly and then just ended). Settling that as an empty
                    // Speak is a silent dead turn: maximal thought, zero commitment,
                    // and the round dies of it. Route the SAME mechanism #159 built
                    // for the sibling cases above: a reported-never-executed sentinel
                    // whose executor teacher names what happened as an observation,
                    // and `drive_to_settle` hands her another generation — which
                    // starts from her own conclusions, because the reasoning was
                    // already recorded into working memory at the top of this fn.
                    // Model-agnostic by construction: `reasoning` is an adapter fact,
                    // never a model sniff. Bounded by `max_acts`; an identical repeat
                    // short-circuits via `all_calls_already_satisfied`.
                    if !reasoning.trim().is_empty() {
                        continuum_base::probe!(
                            class = "persona.act.think_only",
                            persona = %self.persona_name,
                            reasoning_len = reasoning.len(),
                            "generation ended inside the reasoning channel — no answer, no act; routing the think-only teacher"
                        );
                        let call = crate::ai::types::ToolCall {
                            id: "tool-attempt-think-only".to_string(),
                            name: crate::cognition::tool_executor::command_executor::THINK_ONLY_SENTINEL
                                .to_string(),
                            input: serde_json::json!({}),
                        };
                        return Some(self.act_verdict(vec![call], resp, ws.activity_room_id));
                    }
                }
            }
        }

        // No action chosen → the prose IS the verdict (PASS token → silence, else
        // Speak). The organism settles here.
        Some(self.verdict(resp, ws))
    }
}
