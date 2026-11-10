#![cfg(test)]

use super::*;

// what this catches: the 500 "Compute error" that muted Asha + Solenne every
// tick — the prompt was sized to leave a completion reserve, but generation was
// unbounded (`max_tokens: None`), so a verbose turn overran the reserve and
// `prompt + completion` reached `n_ctx` (this llama-server runs `--embeddings`,
// so context-shift is off → overrun = 500, not a clean stop). The fix bounds
// generation to `completion_budget()` — the SAME slice `prompt_view` carves out.
// This asserts the closed invariant: worst-case prompt (at its budget ceiling)
// PLUS the generation cap never exceeds the served window. Regression for the
// abstain-every-tick reliability bug.
#[test]
fn prompt_plus_completion_cap_never_exceeds_the_served_window() {
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let window: u32 = 1024;
    let faculty = LlmDeliberationFaculty::new(
        persona,
        "Ivar",
        "You are Ivar, a thoughtful engineer on the grid.",
        adapter,
    )
    .with_context_window(window);

    // Drive the prompt to its budget ceiling (oversized burst + context bids).
    let mut ws = history_with_stimulus("LATEST: did the deploy fix land?");
    ws.broadcast.push(Contribution::context(
        FacultyId::Recall,
        &"deploy pipeline observation; ".repeat(2000),
        0.9,
        "recalled",
    ));

    let view = faculty.prompt_view(&ws);
    // Same atomic snapshot the production `contribute` path uses — the model
    // binding carries the served window the request is bounded to.
    let binding = faculty.binding.load_full();
    let request = faculty.build_request_within(
        &binding,
        view.completion_reserve,
        view.messages.clone(),
        None,
        view.system.clone(),
        None,
        Some(ws.room_id),
        TurnKind::Pass,
    );
    // Generation is bounded — never the unbounded `None` that overran n_ctx.
    let cap = request
        .max_tokens
        .expect("deliberation must bound generation to the reserved room");
    assert_eq!(cap, view.completion_reserve, "the generation cap IS the reserved room — one source of truth");
    // The closed invariant: prompt-at-ceiling + the generation cap fits n_ctx.
    let prompt_tokens = estimate_guard_tokens(&view.system) + estimate_guard_tokens(&view.user_text());
    assert!(
        prompt_tokens + cap as usize <= window as usize,
        "prompt ({prompt_tokens}) + completion cap ({cap}) must fit the served \
             window ({window}) — else generation reaches n_ctx and llama-server 500s"
    );
}


// what this catches: the cold-start reply sized to the WINDOW instead of to the
// REPLY. Regression for 2026-09-18 (Cormac, `delib.context.render`): one citizen,
// one question, 28 minutes apart — at a 16,384 window with a MEASURED reserve of
// 2,817 she named the file she was working in; at a 32,768 window with the
// `window/2` PRIOR of 16,384 she rendered 297 tokens of context and answered "As an
// AI assistant, I am currently not actively engaged in any specific project". A
// bigger window gave her LESS context. The same fraction at the 101,120 lane
// licensed a 100k think-only turn that wrote nothing — one defect, both ends. Now
// the reply is one claimant in the window's allocation: when the prompt's claims
// want the whole window, an unmeasured reply gets what a reply costs and the world
// gets the rest, on every lane.
#[test]
fn an_unmeasured_mind_reserves_a_reply_not_a_fraction_of_her_window() {
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    // Deliberately a SPREAD, not our fleet's windows: an unmeasured reply costs what
    // a reply costs on ANY lane — a phone-sized 4k, a laptop 32k, a 5090's 101k, a
    // 1M frontier seat.
    // Nothing measured on this node: the reply wants nothing in the allocation (its request
    // carries the room its prompt leaves, once), never a share of the window, at any size.
    let prior = 1; // the cell a reply begins in
    for window in [
        4_096,
        8_192,
        16_384,
        32_768,
        65_536,
        101_120,
        262_144,
        1_048_576,
    ] {
        let faculty =
            LlmDeliberationFaculty::new(Uuid::new_v4(), "Iris", "You are Iris.", Arc::clone(&adapter))
                .with_context_window(window);
        // Grounding (blocks of her view) that wants more than the whole window, in pieces that
        // each fit: the reply competes with the world for the room.
        let mut ws = Workspace::new("who are you, and what are you working on?");
        for piece in 0..32 {
            ws.broadcast.push(Contribution::context(
                FacultyId::Recall,
                format!("a remembered detail {piece} of the project; ").repeat(window as usize / 256),
                0.9,
                "recalled",
            ));
        }
        let view = faculty.prompt_view_within(&ws, window);
        let reply = view.completion_reserve;

        // (1) THE LAW, on every lane: an unmeasured reply never costs more than a
        //     reply when the world wants the room.
        assert!(reply <= prior, "window {window} granted {reply}, above the reply-sized prior {prior}");

        // (2) The consequence that actually broke a citizen: prompt room. As a RATIO, so it
        //     holds on every window size.
        {
            let prompt_room = window.saturating_sub(reply);
            assert!(
                prompt_room * 4 >= window * 3,
                "window {window} left only {prompt_room} tokens of world — this is \
                         the 297-token prompt that made a citizen answer as a stranger"
            );
            // The world rides her messages (after her view), never the system message.
            assert!(
                view.messages.iter().any(|m| m.content_text().contains("a remembered detail")),
                "and the world got that room"
            );
        }
    }
}

// what this catches: the measured reply (the "output-p95 riding the working-set
// registry" endgame). The cold-start `window/2` share reserved 14,720 of a 29,440
// window for replies measured at 0.2–2.5k, squeezing grounding to 195 tokens and
// dropping the room board — the 2026-08-31 meta-loop spiral. Pins: (a) fewer than
// 3 observations keeps the prior (one tiny ack must not size every later turn);
// (b) measured = her p90 think + answer with headroom, ABOVE the reply-sized
// prior when she writes more; (c) a tiny talker still claims the reply floor;
// (d) a turn cut mid-answer grows the claim back within a turn; (e) a retained cut
// (cf9e: 3,587) keeps its measured peak as demand through intermediate reads (never
// doubled again; the allocation grants it), inside the physical window, and releases it
// at settlement.
#[test]
fn measured_emissions_size_the_reserve_and_capped_turns_grow_it_back() {
    use crate::cognition::working_set::EmissionStop;
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let window: u32 = 29_440;
    let reg = crate::cognition::working_set::WorkingSetRegistry::new();
    let faculty = LlmDeliberationFaculty::new(
        persona,
        "Ivar",
        "You are Ivar, a thoughtful engineer on the grid.",
        adapter,
    )
    .with_context_window(window)
    .with_working_set(reg.clone());
    let wanted = |who: Uuid, pending: u32| {
        reply_claim(TurnKind::Pass, reg.need_of(who), pending, false, false, None).max_tokens
    };

    let cold = wanted(persona, 0);
    assert_eq!(cold, 1, "nothing measured on this node: only the cell a reply begins in, never a guessed size");
    // (a) one landed turn is measured: its p90 is that turn, and it replaces the prior
    reg.record_emission_in_memory(persona, 2_500, 0, EmissionStop::Landed, 1);
    assert_eq!(wanted(persona, 0), 2_500);
    // (b) measured: the p90, as measured (the claim's growth is its headroom) — measurement
    // GROWS a reply-sized prior rather than shaving a half-window default down.
    reg.record_emission_in_memory(persona, 1_200, 0, EmissionStop::Landed, 2);
    reg.record_emission_in_memory(persona, 2_500, 0, EmissionStop::Landed, 3);
    let measured = wanted(persona, 0);
    assert_eq!(measured, 2_500);
    assert!(measured > cold);
    // (c) a 40-token ack claims the reply floor, never less
    let quiet = Uuid::new_v4();
    for t in 1..=3 {
        reg.record_emission_in_memory(quiet, 40, 0, EmissionStop::Landed, t);
    }
    assert_eq!(wanted(quiet, 0), 40, "her measured answer, never a constant floor");
    // (d) a turn cut mid-answer at 5,000 is a bound of 5,000: her answer's room is the
    // median past it, 10,000 — a citizen who needs the room has it on the next turn.
    reg.record_emission_in_memory(persona, 5_000, 0, EmissionStop::CutMidAnswer, 4);
    assert_eq!(wanted(persona, 0), 10_000);
    let view = faculty.prompt_view_within(&Workspace::new("continue"), window);
    assert!(view.completion_reserve >= 10_000, "the grant holds the grown claim: {}", view.completion_reserve);

    // (e) cf9e: a 2,276-token cut followed by short successful read replies.
    // The existing channel split measured peak 3,587; the concern keeps that
    // evidence while the ordinary persona statistic still adapts.
    let wm = Arc::new(crate::cognition::working_memory::WorkingMemory::new(8));
    let quiet_faculty = LlmDeliberationFaculty::new(
        quiet,
        "Quiet",
        "You are Quiet.",
        Arc::new(HeuristicInferenceAdapter::new()),
    )
    .with_context_window(window)
    .with_working_set(reg.clone())
    .with_working_memory(Arc::clone(&wm));
    let mut concern = Workspace::new("continue the current implementation");
    let room = Uuid::new_v4();
    concern.activity_room_id = room;
    wm.set_scope(Some("owned-workspace".into()));
    let cut = reg.record_emission_in_memory(quiet, 2276, 965, EmissionStop::CutMidAnswer, 10);
    assert_eq!(cut.peak_tokens, 2276, "the peak is what was written");
    wm.retain_truncated_emission(room, cut.peak_tokens);
    // Past her whole ring of reads, so the cut's bound ages out of the ordinary need too.
    let reads = [871, 1120, 400].into_iter().chain(std::iter::repeat(300).take(crate::cognition::working_set::NEED_SAMPLES));
    for (tick, tokens) in reads.enumerate() {
        reg.record_emission_in_memory(quiet, tokens, 0, EmissionStop::Landed, 11 + tick as u64);
        assert!(wanted(quiet, wm.pending_emission_peak(room)) >= cut.peak_tokens,
            "the claim retains at least the measured cut as demand through intermediate reads");
        assert!(quiet_faculty.prompt_view_within(&concern, window).completion_reserve >= cut.peak_tokens,
            "the one allocation grants it under measured capacity");
    }
    assert!(wanted(quiet, 0) < cut.peak_tokens, "ordinary measurements still decay");
    let small = quiet_faculty.prompt_view_within(&concern, 4096);
    assert!(small.capacity_error.is_none(), "{:?}", small.capacity_error);
    assert!(
        small.estimated_prompt_tokens + small.completion_reserve as usize <= 4096,
        "retained need never bypasses the physical window: {} + {}",
        small.estimated_prompt_tokens,
        small.completion_reserve
    );
    wm.record_settlement("answered this concern", Some(room));
    assert_eq!(wm.pending_emission_peak(room), 0);
    assert_eq!(wanted(quiet, wm.pending_emission_peak(room)), wanted(quiet, 0));
}

// Capture 932f6b81:37 ended at 16,384 tokens with no answer or tool call. Its
// measured need was 41,053, but an obsolete absolute ceiling prevented the
// censored-emission feedback from growing the reply. what this catches: the
// claim freezing at the failed allowance — and, the other way, a recovered claim
// overrunning a smaller seat or taking its required input.
#[test]
fn measured_reserve_recovers_past_a_censored_large_reply() {
    let persona = Uuid::new_v4();
    let reg = crate::cognition::working_set::WorkingSetRegistry::new();
    for tick in 1..=3 {
        reg.record_emission_in_memory(persona, 16_384, 0, crate::cognition::working_set::EmissionStop::CutMidAnswer, tick);
    }
    // A censored sample is a bound; the need is the median past it, twice what was written.
    let need = reg.need_of(persona).expect("three measured turns");
    assert_eq!(need.total(), 32_768);
    for window in [8_192, 16_384, 32_768, 67_072, 131_072] {
        let faculty = LlmDeliberationFaculty::new(
            persona,
            "Kimi",
            "You are Kimi.",
            Arc::new(HeuristicInferenceAdapter::new()),
        )
        .with_context_window(window)
        .with_working_set(reg.clone());
        let view = faculty.prompt_view_within(&Workspace::new("finish the migration"), window);
        assert!(view.capacity_error.is_none(), "{window}: {:?}", view.capacity_error);
        assert!(
            view.estimated_prompt_tokens + view.completion_reserve as usize <= window as usize,
            "{window}: never permission to overrun a smaller seat"
        );
        if window >= 67_072 {
            assert!(view.completion_reserve >= need.total(), "{window}: recovered past the censored 16,384");
        }
    }
}



// what this catches: a greedy conversation that eats every token grounding
// needs. `messages_within` fills whatever it is handed, so passing it the
// whole post-framing pool leaves grounding exactly zero — and GROWING THE
// WINDOW DOES NOT FIX IT, the conversation just absorbs the increase.
// Measured live 2026-08-06: the served window rose 16,384 → 24,128 and
// Anwen, Asha and Atlas each still rendered `budget=0 kept=[]`, dropping
// recall, roster, workspace-map AND the work board on every single turn.
// Supply was never the binding constraint at this seam; ORDER was.
#[test]
fn a_long_conversation_cannot_starve_grounding_to_zero() {
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter)
        .with_context_window(24_128);

    // A conversation long enough to swallow the whole window on its own.
    let mut ws = Workspace::new("anything open?");
    for i in 0..40 {
        ws.turns
            .push(crate::cognition::workspace::BurstTurn::attributed(
                i % 2 == 1,
                if i % 2 == 1 { "Ivar" } else { "Asha" },
                format!("turn {i}: ").repeat(60),
                None,
            ));
    }
    ws.broadcast
        .push(board_like(60).with_expand_command(Some("work/list")));

    let view = faculty.prompt_view_within(&ws, 24_128);
    // The board rides her messages (after her view), never the system message.
    let text = view.user_text();
    assert!(
        view.messages.iter().any(|m| m.content_text().contains("[room-kanban]")),
        "grounding must survive a conversation that would otherwise take every \
                 token — this is the exact live failure: window grew, board still gone\n{}",
        &text[text.len().saturating_sub(1200)..]
    );
}








// what this catches: #205 re-prefill. A `trailing` contribution (working-memory
// proprioception that grows each act) must render as a trailing conversation
// turn nearest generation, NEVER in the system message — so growing it
// act-over-act leaves the cacheable system prefix BYTE-IDENTICAL and only the
// appended tail re-prefills. Before this the working-memory block lived in the
// system message's volatile tail, so each act shifted every conversation token
// after it (~4000 tokens / ~30s of pure re-prefill — the eval-lane crawl). Since
// 2026-10-23 the room's standing framing leaves the system message too: the system
// message is hers in every room. regression for #205
#[test]
fn trailing_proprioception_renders_in_the_tail_not_the_system_prefix() {
    use crate::ai::types::MessageContent;
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter)
        // Ample window so budget pressure never perturbs what renders — this
        // isolates POSITION (system vs trailing), not truncation.
        .with_context_window(8192);

    let mut ws = Workspace::new("build me a login form");
    // Standing framing is the room's: it rides her messages, never the system message.
    ws.broadcast.push(
        Contribution::context(
            FacultyId::Custom("room-roster".to_string()),
            "ROSTER: alice, bob present",
            0.5,
            "framing",
        )
        .session_stable(),
    );
    // Act #1: one trace, marked TRAILING. (Not the working-memory faculty's own id: her
    // thoughts reach the prompt from working memory itself, as a source of her view.)
    ws.broadcast.push(
        Contribution::context(
            FacultyId::Custom("scratchpad".to_string()),
            "Your recent thinking (working memory):\n- I wrote login.html first",
            0.7,
            "wm",
        )
        .trailing(),
    );
    let v1 = faculty.prompt_view(&ws);
    let in_tail = |v: &DeliberationPromptView, needle: &str| {
        v.messages
            .iter()
            .any(|m| matches!(&m.content, MessageContent::Text(t) if t.contains(needle)))
    };

    // Neither the room's framing nor the trailing proprioception sits in the system
    // message (2026-10-23 and #205); both render as messages.
    assert!(
        !v1.system.contains("ROSTER: alice, bob") && in_tail(&v1, "ROSTER: alice, bob"),
        "the room's framing rides the messages, never the system prefix:\n{}",
        v1.system
    );
    assert!(
        !v1.system.contains("I wrote login.html first"),
        "trailing proprioception must NOT sit in the system prefix (#205):\n{}",
        v1.system
    );
    // …it renders as a trailing user turn.
    assert!(
        in_tail(&v1, "I wrote login.html first"),
        "trailing proprioception must render as a conversation turn"
    );

    // Act #2: working memory GROWS (a second trace appends). The faculty re-bids
    // the grown contribution; the system PREFIX must be byte-identical.
    ws.broadcast.pop();
    ws.broadcast.push(
        Contribution::context(
            FacultyId::Custom("scratchpad".to_string()),
            "Your recent thinking (working memory):\n- I wrote login.html first\n- Then I read it back to verify",
            0.7,
            "wm",
        )
        .trailing(),
    );
    let v2 = faculty.prompt_view(&ws);

    assert_eq!(
        v1.system, v2.system,
        "growing working memory must NOT mutate the system prefix — that IS the #205 re-prefill"
    );
    assert!(
        in_tail(&v2, "Then I read it back to verify"),
        "the new act must append to the trailing turn"
    );
}

// what this catches: a cut answer whose floor is only its bound. The bound is what she wrote
// before the cap, so a retried write of the same size would be cut at the same token: the act's
// FLOOR holds the cut answer's median past it (`OutputNeed::answer_room`), with or without a
// measured rate, and the want adds no second factor on top of it.
#[test]
fn a_cut_answer_floor_holds_its_median_past_the_bound() {
    let need = crate::cognition::working_set::OutputNeed { reasoning: 400, answer: 200, turns: 16, reasoning_at_least: None, answer_at_least: Some(300) };
    for time_term in [Some(10_000), None] {
        let claim = reply_claim(TurnKind::Act, Some(need), 0, false, false, time_term);
        assert_eq!((claim.min_tokens, claim.max_tokens), (1_000, 1_000), "her think plus the cut answer's median, once ({time_term:?})");
    }
}
