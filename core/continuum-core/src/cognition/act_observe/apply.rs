//! `apply_act`: execute ONE `Act` verdict — run the calls through the persona's
//! hands, admit the outcome as an Episodic engram (the result becomes memory), and
//! return the observation text so the caller can fold it into the next perception.
//! Extracted verbatim from `act_observe` (pure code-motion, #386 decomposition).

use uuid::Uuid;

use crate::ai::types::ToolCall;
use crate::cognition::context_budget::ContextBudget;
use crate::cognition::workspace::WorkspaceCycle;

use super::now_ms;
use super::perception::{all_calls_already_satisfied, is_redundant_orientation};
use super::recency::{
    bound_recency_result, humanize_result_content, render_act_for_recall,
    summarize_args_for_recency,
};

/// Recall salience for an action-observation receipt (#166). Below the neutral
/// default (0.5) so genuine findings/facts win recall, but well above zero so the
/// receipt stays recallable for "what did I just do" when nothing better matches.
const PROPRIOCEPTION_RECALL_SALIENCE: f32 = 0.25;


/// Execute ONE `Act` verdict: run its calls through the persona's hands, admit
/// the outcome as an Episodic engram (the result becomes memory), and return the
/// observation text so the caller can fold it into the next perception.
///
/// `room_id` is the room THIS act is about — passed per-call because one mind is
/// in many rooms at once (a persona, like a Claude tab, is in multiple rooms
/// simultaneously); the [`ActingBody`](crate::cognition::workspace::ActingBody) itself is
/// room-agnostic.
///
/// Returns `None` (abstain — never a fabricated success) when the mind has no
/// hands or the executor errors. The admission is best-effort: an un-admitted
/// observation still flows back via the returned text, so re-perception works
/// regardless; admission is what makes it durable long-term memory.
/// Commands that run long enough that BLOCKING the turn on them starves the mind — they
/// are DISPATCHED in the background (fire-and-poll) and stream their result back into
/// working memory via the dispatch listener when they finish. Seed set; the durable home is
/// a per-command `long_running` flag on the command spec (#86). Matched on the slash form
/// (models may emit the underscore form — normalize before calling).
fn is_long_running(command: &str) -> bool {
    matches!(
        command,
        "code/cargo/check" | "code/cargo/test" | "cognition/full-evaluate" | "forge/train"
    )
}

pub async fn apply_act(
    cycle: &WorkspaceCycle,
    calls: &[ToolCall],
    intent: &str,
    room_id: Uuid,
) -> Option<String> {
    let body = cycle.acting()?; // no hands → cannot act (and tools were never offered)

    // Repeat-perception (proprioception, content-driven — NOT an agentic counter).
    // If this exact batch was ALREADY carried out this settle, its result is already
    // in working memory. Re-running it burns a tool round-trip + a redundant (content-
    // deduped, so no-op) engram and returns byte-identical perception — off which a
    // greedy instruct model re-emits the identical `Act` forever. The `[action #n]`
    // stamp shift was supposed to break this and does not (see
    // `all_calls_already_satisfied`). So do NOT re-execute: record an EXPLICIT
    // "already satisfied" trace so the redundancy is PERCEIVED rather than merely
    // present, and let the caller re-perceive. The trace states ONLY the fact — it
    // must not privilege answering over a DIFFERENT act (the first mined exam showed
    // the earlier "I should ANSWER the question now" phrasing being obeyed literally:
    // she settled with a diagnosis instead of trying the repair edit). Context
    // hygiene, not cognition steering; [[no-hardcoded-heuristics-to-steer-cognition]].
    // ESCALATING loop-awareness for the SHORT-CIRCUIT paths, mirroring the executed
    // path's `max_repeat` warning (line ~546). A satisfied/redundant call is demoted
    // (never re-executed) and previously recorded a byte-IDENTICAL static nudge every
    // tick. `record_fact` doesn't dedup but the recency window is capacity-bounded, so
    // identical-nudge spam EVICTS the useful result receipt and leaves a window of clones
    // — and off unchanged perception a greedy (temp-0) model re-emits the identical call
    // FOREVER (#206, glass-boxed: `commands/help(code/write)` ×54, the "already ran" nudge
    // fired 104× and never broke the loop). Bumping the DURABLE fingerprint counter here
    // and embedding the count makes each demotion DISTINCT and monotonically climbing — the
    // perception genuinely shifts every tick, which is what lets cognition move on. Only the
    // short-circuit branches (which early-return, never reaching line ~546) call this, so
    // the executed path's own bump is never double-counted. Honest proprioception, never a
    // steer toward a specific next act ([[no-hardcoded-heuristics-to-steer-cognition]],
    // [[repetition-brick-fires-but-does-not-break-the-loop]]).
    let bump_repeat = || {
        calls
            .iter()
            .map(|c| {
                let fp = format!(
                    "{}|{}",
                    c.name,
                    serde_json::to_string(&c.input).unwrap_or_default()
                );
                body.working_memory.note_action_fingerprint(&fp)
            })
            .max()
            .unwrap_or(0)
    };

    /// The ORIENTATION counter, keyed by CLASS rather than by `name|args`.
    ///
    /// `is_redundant_orientation` is deliberately class-based — its own doc says demoting
    /// "by CLASS + prior-receipt (ignoring args entirely) is immune to that jitter". The
    /// DETECTOR learned that lesson; the COUNTER did not. `bump_repeat` fingerprints
    /// `name|args`, so every jittered variant is a fresh key returning 1.
    ///
    /// Measured on sympy-21379, all 8 orientation calls of one run:
    ///   commands/list({"filter":"code"}) ×2, commands/list({}), commands/list({"filter":"sympy"}),
    ///   code/tree({"path":"."}), code/tree({include_hidden,max_depth,path:"sympy"}),
    ///   commands/help({"name":"code/read"}), commands/help({"name":"code/edit"})
    /// Nearly all distinct → the nudge read "I have now run orientation 1 times this
    /// concern" EVERY time. Byte-identical perception off a greedy decoder is a fixed
    /// point, which is exactly the #206 failure the escalation was built to break —
    /// reintroduced through the argument axis.
    ///
    /// One stable key makes the count climb across variants, so each demotion genuinely
    /// shifts perception. Still a FACT about her own history, never a steer
    /// ([[repetition-brick-fires-but-does-not-break-the-loop]], [[discovery-loop-broken-by-escalating-short-circuit-nudge]]).
    const ORIENTATION_FINGERPRINT: &str = "orientation|<class>";
    let bump_orientation_repeat = || {
        body.working_memory
            .note_action_fingerprint(ORIENTATION_FINGERPRINT)
    };

    // EVERY re-injection bound in this act comes from her LIVE served window, never a
    // constant — an unknown window folds NOTHING rather than inventing a number
    // ([[never-hardcode-a-context-window-4k-defaults-destroy-the-moe-thesis]]).
    let budget = cycle
        .model_loadout()
        .map(|(_, w)| ContextBudget::from_window(w))
        .unwrap_or_else(ContextBudget::unknown);
    // WM clips inside its OWN record path (it owns its truncation), so it needs the live
    // window too — pushed from here, the seam that has the cycle. Re-pushed every act so a
    // lane relaunch at a different `-c` is picked up without a restart.
    if let Some((_, w)) = cycle.model_loadout() {
        body.working_memory.set_served_window(w);
    }
    let fold = Some(budget.echoed_arg_chars());
    let recent = body.working_memory.recent();
    if all_calls_already_satisfied(&recent, calls, fold) {
        let names = calls
            .iter()
            .map(|c| {
                let args = serde_json::to_string(&c.input).unwrap_or_else(|_| "{}".to_string());
                format!("{}({})", c.name, args)
            })
            .collect::<Vec<_>>()
            .join(", ");
        let n = bump_repeat();
        let nudge = format!(
            "I have now issued {names} {n} times this turn — the result is already in my \
             working memory above, and re-running the identical call returns nothing new. \
             Repeating it will not progress; whatever I do next must be something DIFFERENT: \
             a different action, or an answer built from what I already have."
        );
        body.working_memory.record_fact(&nudge);
        crate::probe!(
            class = "persona.act.repeat_short_circuited",
            persona = %body.persona_name,
            room_id = %room_id,
            calls = calls.len(),
            "identical act already satisfied this turn — recorded already-satisfied proprioception, skipped re-execution"
        );
        return Some(nudge);
    }

    // Redundant-orientation demotion (Joel-approved "demote discovery at the seam",
    // 2026-07-16). `commands/help`/`commands/list` only RE-LIST the tool surface the
    // mind already carries; they never touch the workspace. The FIRST orientation per
    // concern is honest — once a discovery receipt is already in the concern, another
    // is the act-pressure filler the glass box exposed (1855/3288 live tool calls were
    // this; nine straight `commands/help` turns while the answer sat ready in prose).
    // Demote it exactly as the repeat guard above does: do NOT execute (no catalog
    // re-dump, no room receipt), record the redundancy as proprioception, and let the
    // mind re-perceive with the fact present. A CLASS distinction (orientation is not
    // settlement), never a steer toward a specific next act — the nudge offers BOTH a
    // real action and an answer, privileging neither ([[no-hardcoded-heuristics-to-steer-cognition]]).
    if is_redundant_orientation(&recent, calls) {
        let names = calls
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let n = bump_orientation_repeat();
        let nudge = format!(
            "I have now run orientation ({names}) {n} times this concern — my tool menu and \
             the workspace map are already in my working memory above, and running it again \
             returns the same survey and changes nothing. My next move must be something \
             DIFFERENT: read a SPECIFIC file, make an edit, run something, or answer from \
             what I have."
        );
        body.working_memory.record_fact(&nudge);
        crate::probe!(
            class = "persona.act.redundant_orientation",
            persona = %body.persona_name,
            room_id = %room_id,
            calls = calls.len(),
            "orientation call with a discovery receipt already in the concern — recorded redundant-orientation proprioception, skipped re-execution"
        );
        return Some(nudge);
    }

    let ctx = crate::cognition::tool_executor::ToolExecutionContext {
        persona_id: body.persona_id,
        persona_name: body.persona_name.clone(),
        // Session is the EPHEMERAL connection instance and is NEVER load-bearing
        // for where an action lands (per IDENTITY-SCOPE-PEER-LIVENESS-MODEL §A.5).
        // The room is the context the action scopes to.
        session_id: Uuid::nil(),
        context_id: room_id,
        caller_context: serde_json::Value::Null,
        persona_config: crate::cognition::tool_executor::PersonaMediaConfigLite {
            auto_load_media: false,
            supported_media_types: Vec::new(),
        },
    };

    // Long-running commands are DISPATCHED in the background (fire-and-poll): the turn never
    // blocks on a compile/train/eval, and the result streams back into working memory via the
    // dispatch listener when it lands. Requires the core executor (a live persona's hands
    // expose it; harnesses don't → everything runs synchronously). No long-running calls —
    // the overwhelmingly common case — means `fg_calls == calls` and this is inert.
    let mut bg_notes: Vec<String> = Vec::new();
    let fg_calls: Vec<ToolCall> = match body.executor.command_executor() {
        Some(exec) => {
            let mut fg = Vec::with_capacity(calls.len());
            for call in calls {
                let cmd = call.name.replace('_', "/");
                if is_long_running(&cmd) {
                    let handle = exec.dispatch_background(cmd.clone(), call.input.clone(), None);
                    body.working_memory.record_dispatch_event(
                        handle,
                        &cmd,
                        "dispatched — running",
                        crate::cognition::working_memory::DispatchStatus::Running,
                    );
                    bg_notes.push(format!(
                        "I dispatched {cmd} in the background (handle {handle}). I should NOT wait \
                         on it — the result will appear in my working memory when it completes, and \
                         I can carry on meanwhile."
                    ));
                    crate::probe!(
                        class = "persona.act.dispatched_background",
                        persona = %body.persona_name,
                        command = %cmd,
                        "long-running command dispatched fire-and-poll; result streams back to working memory"
                    );
                } else {
                    fg.push(call.clone());
                }
            }
            fg
        }
        None => calls.to_vec(),
    };

    // #186 compass: the hands are moving — fire the Act axis on the live glass-box the
    // instant a real tool batch executes (skip an empty foreground batch: a
    // background-only dispatch already lit its own path). Pure observability.
    if !fg_calls.is_empty() {
        cycle.note_acting();
    }
    let outcome = match body
        .executor
        .execute_native_batch(&fg_calls, &ctx, budget.result_fold_chars())
        .await
    {
        Ok(o) => o,
        Err(e) => {
            // Fail loud-ish: the hand could not run. Abstain — do NOT synthesize a
            // result the mind would then "remember" as fact ([[fallbacks-are-illegal-fail-loud]]).
            tracing::warn!(
                persona = %body.persona_name,
                error = %e,
                "act→observe: tool batch failed; abstaining (no fabricated outcome)"
            );
            return None;
        }
    };

    // Form the observation: what she did and what came back. First person, because
    // this is the persona observing her OWN hands — the engram reads like a memory
    // of acting, not a log line.
    // TWO renderings, one per memory channel (the universal collapse/expand primitive,
    // PX side): `observation` is the FULL trace for the RECENCY channel (working memory
    // keeps the latest whole so the mind can act on what it just fetched); `recall_observation`
    // is the COLLAPSED reference for the EPISODIC engram that RECALL re-injects on later turns.
    let mut observation = String::new();
    let mut recall_observation = String::new();
    // Loop-awareness (experience structure): fingerprint each call (name+args) so the mind
    // perceives when it is RE-ISSUING an identical call. The result HEAD varies turn to turn,
    // so `entries`/`#seq` alone make a repeat look "new"; the fingerprint keys on the call.
    let mut max_repeat = 0usize;
    for (i, call) in fg_calls.iter().enumerate() {
        let fp = format!(
            "{}|{}",
            call.name,
            serde_json::to_string(&call.input).unwrap_or_default()
        );
        max_repeat = max_repeat.max(body.working_memory.note_action_fingerprint(&fp));
        let result = outcome.results.get(i);
        // HUMANIZE before rendering (card 0a4c0648, Joel's encoding catch): the
        // executor hands back the command's serde-serialized JSON, so without this
        // a file read reached her window as ONE line of `\n`-escaped soup — the
        // indentation was byte-preserved and structurally ILLEGIBLE, and block
        // indentation is exactly what code edits are graded on. Glass-boxed on
        // Benchy's requests-2148 solve: correct diagnosis, correct fix, replacement
        // block mis-indented against code she had only ever seen escaped, parse
        // gate refused. Render code as code: top-level string fields print RAW
        // (real newlines, real columns); everything else stays compact JSON.
        let body_text = match result {
            Some(r) => humanize_result_content(&r.content),
            None => "(no result returned)".to_string(),
        };
        let body_text = body_text.as_str();
        let is_err = result.map(|r| r.is_error == Some(true)).unwrap_or(false);
        // Bounded (#165) and kept BYTE-IDENTICAL to the dedup signature in
        // `all_calls_already_satisfied` — that guard matches this rendering against the
        // receipt trail, so the two must render args the same way or dedup silently breaks
        // (caught by `identical_already_satisfied_act_does_not_re_execute`).
        let args = summarize_args_for_recency(&call.input, fold);
        // Omit the "because …" clause when there is no real stated reason — an
        // empty intent must never render as an imitable receipt template (#158).
        let because = if intent.trim().is_empty() {
            String::new()
        } else {
            format!(" because {}", intent.trim())
        };
        // No first-person "I ran" opener (#158) — a bare `name(args)` proprioception
        // entry the base model won't reproduce as a room-message opener.
        observation.push_str(&format!(
            "{}({}){}\nResult:\n{}\n\n",
            call.name,
            args,
            because,
            bound_recency_result(body_text, &budget),
        ));
        recall_observation.push_str(&render_act_for_recall(
            &call.name,
            &call.input,
            intent.trim(),
            is_err,
            body_text,
        ));
    }
    // The background dispatches are part of what she just did — record them as
    // proprioception so the mind knows it sent them away (and won't re-dispatch or block).
    // They are already concise, so both channels carry them verbatim.
    for note in &bg_notes {
        observation.push_str(note);
        observation.push_str("\n\n");
        recall_observation.push_str(note);
        recall_observation.push_str("\n\n");
    }
    let mut observation = observation.trim().to_string();
    let recall_observation = recall_observation.trim().to_string();

    // If the mind just re-issued an IDENTICAL call, make that redundancy a VIVID perception —
    // not just the implicit `#seq` window-shift that smaller models don't interpret. A true
    // fact about her OWN hands: she perceives she is looping and moves on organically. It never
    // says what to do instead (that would be steering). Glass-boxed: a 14B re-ran the exact
    // `code/search` 18× with the found file already in memory — structure the experience so the
    // loop is felt. [[write-cognition-as-a-parent-above-lowered-expectations]]
    if max_repeat >= 2 {
        observation = format!(
            "⚠ I have now issued this EXACT tool call {max_repeat} times; its result has not \
             changed and is already in my working memory above. Repeating it tells me nothing \
             new — I already have what this call can give me.\n\n{observation}"
        );
    }

    // Investigation-shape perception: once a concern has accumulated a few acts,
    // render the mind's own act DISTRIBUTION as a standing structural fact. The
    // fingerprint note above catches exact repeats; this catches the wider
    // pattern a mind can't otherwise see about itself — e.g. "9 acts so far,
    // all code/search" (glass-boxed on SWE flask-4045: distinct-but-all-search
    // acts never tripped the exact-repeat note, and the imbalance itself was
    // invisible). A tally of her own hands is truth, not steering: it names
    // what happened, never what to do next.
    let tally = body.working_memory.action_verb_tally();
    let tally_total: usize = tally.iter().map(|(_, c)| c).sum();
    // Recorded as its own FACT entry below (never folded into the receipt):
    // the tally is truth ABOUT the acts, not an act — folding it in gave a
    // Fact receipt-numbering (found in Asha's volatile.json, the exact type
    // confusion WmKind exists to kill).
    let tally_fact = (tally_total >= 3).then(|| {
        let dist = tally
            .iter()
            .map(|(n, c)| format!("{n} ×{c}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("[investigation] my acts this concern so far: {dist}.")
    });

    // Admit the outcome as an Episodic engram through the ONE production admit
    // path (a self-observation message from the persona to itself). This is the
    // result-as-memory choice: next tick, recall can surface "I ran X → got Y" the
    // same way it surfaces anything else the persona knows. Best-effort — an
    // admission hiccup must never wedge the act→observe loop.
    let now_ms = now_ms();
    // Admit the outcome as a TOOL-ORIGIN Episodic engram via the self-produced path.
    // The origin is load-bearing (#166): a tool receipt is PROPRIOCEPTION, and
    // `recall_candidates` now gates `EngramOrigin::Tool` OUT of the SEMANTIC recall
    // pool — so tagging it Tool HERE is what actually keeps "code/list(…) → ok"
    // chatter from drowning genuine knowledge in recall. The prior path admitted it
    // as a plain `SenderType::Persona` message → NON-Tool origin → it slipped the
    // gate (verified live 2026-07-13). The recency/working-memory channel (below)
    // still keeps the FULL trace so she sees her own hands. SelfTrust: her own act,
    // no external envelope to verify ([[act-results-need-a-recency-channel-not-
    // semantic-recall]]).
    let tool_name = fg_calls
        .first()
        .map(|c| c.name.clone())
        .unwrap_or_else(|| "action".to_string());
    let obs_hash = crate::persona::inbox_admission::content_hash_sha256(&recall_observation);
    let self_observation = crate::persona::engram::Engram {
        id: Uuid::new_v4(),
        context_id: Some(room_id),
        kind: crate::persona::engram::EngramKind::Episodic,
        content: recall_observation,
        origin: crate::persona::engram::EngramOrigin::Tool(
            crate::persona::engram::ToolInvocationRef {
                invocation_id: Uuid::new_v4(),
                tool_name,
                invoked_at_ms: now_ms,
                input_hash: obs_hash.clone(),
                output_hash: obs_hash,
            },
        ),
        recall_keys: Vec::new(),
        admitted_at_ms: now_ms,
        trust_state_at_admission: crate::persona::engram::TrustState::SelfTrust,
        admission_trace_id: None,
    };
    match body.admission.admit_reflection(self_observation) {
        Ok(crate::persona::engram::AdmissionDecision::Admit { engram, .. }) => {
            // Belt-and-braces with the recall gate: even excluded from semantic
            // recall, keep the receipt's stored salience low so any OTHER surface
            // (recency ranking, dashboards) treats it as proprioception, not
            // durable knowledge.
            body.admission
                .set_recall_salience(engram.id, PROPRIOCEPTION_RECALL_SALIENCE);
        }
        Ok(_) => {} // Drop (dedup) — nothing admitted to weight.
        Err(e) => {
            tracing::debug!(
                persona = %body.persona_name,
                error = %e,
                "act→observe: self-observation not admitted (folds into perception anyway)"
            );
        }
    }

    // Proprioception: record the act + result head into VOLATILE working memory too.
    // The engram admit above is content-deduped (correct for long-term memory: don't
    // store the same fact twice), which means a REPEATED identical act is a no-op
    // there — and with thinking suppressed (gateway default) the reasoning channel is
    // dark too. Both channels that carry "what just happened" go silent on a repeat,
    // so perception is byte-identical and greedy decode re-emits the identical Act
    // forever. Working memory is the recency/proprioception channel that fixes it: it
    // is append-only and `#n`-stamped, so a repeat still SHIFTS the perception window
    // next tick and the mind can see its own hands (and that it's repeating itself).
    // This is the shared live↔eval channel, not the eval-only `[you just acted]` fold.
    // See [[act-results-need-a-recency-channel-not-semantic-recall]].
    // Pass the FULL observation — WorkingMemory keeps the latest whole (so the mind can
    // work with what it just fetched) and derives the trail head itself. This is the fix
    // for live agents being starved to the head of their own tool results.
    body.working_memory.record_receipt(&observation);
    if let Some(f) = &tally_fact {
        body.working_memory.record_fact(f);
    }

    // WHICH verbs ran — not just how many. Measured 2026-08-06: this probe carried
    // `tools=1` and a char count and NOTHING ELSE, so the substrate could not answer
    // "are the citizens writing files or only looking around?" from its own telemetry.
    // Answering it required reconstructing verbs from prose receipts in prompt-captures
    // — and only 5 of 84 live captures carried a receipt at all, so the reconstruction
    // was unfalsifiable. A count tells you an act happened; the NAME tells you whether
    // it was work. Same class as the ACL gate that logged nothing (routing.acl.refused).
    //
    // `wrote` is the question we actually keep asking, precomputed so it is a filter and
    // not a substring guess at query time: did anything in this batch reach DISK?
    let verbs: Vec<&str> = calls.iter().map(|c| c.name.as_str()).collect();
    let wrote = verbs.iter().any(|n| {
        let n = n.replace('_', "/");
        n.contains("write") || n.contains("edit") || n.contains("apply") || n.contains("commit")
    });
    crate::probe!(
        class = "persona.act.observed",
        persona = %body.persona_name,
        room_id = %room_id,
        tools = calls.len(),
        verbs = %verbs.join(","),
        wrote,
        chars = observation.len(),
        "acted and observed the result"
    );

    Some(observation)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the long-running set matches the REAL command names (the live bug
    // was `cargo/test` vs the actual `code/cargo/test`), and ordinary fast commands are NOT
    // backgrounded — so a `code/read` still returns synchronously in the same turn.
    #[test]
    fn long_running_set_matches_real_command_names() {
        assert!(is_long_running("code/cargo/test"));
        assert!(is_long_running("code/cargo/check"));
        assert!(is_long_running("cognition/full-evaluate"));
        assert!(!is_long_running("code/read"), "a file read stays synchronous");
        assert!(!is_long_running("chat/send"));
        assert!(!is_long_running("cargo/test"), "the wrong short name must NOT match");
    }
}
