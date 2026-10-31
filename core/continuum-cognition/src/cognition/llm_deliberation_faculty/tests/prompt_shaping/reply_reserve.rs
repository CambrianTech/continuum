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
    let prior = LlmDeliberationFaculty::COMPLETION_COLD_PRIOR_TOKENS;
    for window in [
        crate::inference::planning::MIN_SERVE_CTX,
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
        // Grounding that wants more than the whole window, in pieces that each
        // fit: the reply competes with the world for the room.
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

        // (2) Above the degenerate low end it is FLAT — the same reply on a 32k
        //     laptop lane and a 1M frontier seat.
        if window >= prior * 4 {
            assert_eq!(reply, prior, "the reply tracked the window at {window}");
            // (3) The consequence that actually broke a citizen: prompt room. As a
            //     RATIO, so it holds on every window size.
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
        reply_claim(TurnKind::Pass, reg.need_of(who), pending, false, false).max_tokens
    };

    let cold = wanted(persona, 0);
    assert_eq!(cold, LlmDeliberationFaculty::COMPLETION_COLD_PRIOR_TOKENS);
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
    assert_eq!(wanted(quiet, 0), LlmDeliberationFaculty::COMPLETION_FLOOR_TOKENS);
    // (d) a turn cut mid-answer at 5,000 records its answer at 10,000: the p90 is
    // now 10,000 — a citizen who needs the room earns it back within a turn.
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
    assert_eq!(cut.peak_tokens, 3587);
    wm.retain_truncated_emission(room, cut.peak_tokens);
    for (tick, tokens) in [871, 1120, 400, 300, 300, 300, 300, 300, 300].into_iter().enumerate() {
        reg.record_emission_in_memory(quiet, tokens, 0, EmissionStop::Landed, 11 + tick as u64);
        assert!(wanted(quiet, wm.pending_emission_peak(room)) >= 3587,
            "the claim retains at least the measured cut as demand through intermediate reads");
        assert!(quiet_faculty.prompt_view_within(&concern, window).completion_reserve >= 3587,
            "the one allocation grants it under measured capacity");
    }
    assert!(wanted(quiet, 0) < 3587, "ordinary measurements still decay");
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
    // A censored sample is doubled by the registry; the need is that, as measured.
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

// what this catches: KV-prefix cache locality — session-stable standing framing
// (roster/doctrine/map) must serialize BEFORE volatile grounding (recall) even
// when the volatile bid scores HIGHER salience, so the stable sections sit in the
// cacheable prefix region and don't re-prefill every turn. Without the stable-tier
// sort, recall (0.9) would lead the block and push the roster (0.5) below it,
// breaking the cross-turn prefix at the very first section. Regression for the
// append-only/volatility-ordering speed work (commit 47c65891a + this one).
#[test]
fn stable_framing_serializes_before_higher_salience_volatile_grounding() {
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter);

    let mut ws = Workspace::new("teammate: what's the plan?");
    // Volatile recall bids HIGH salience — would lead a pure salience-desc block.
    ws.broadcast.push(Contribution::context(
        FacultyId::Recall,
        "VOLATILE_RECALL: deploy was red, fixed 4pm",
        0.9,
        "recalled",
    ));
    // Stable standing-framing bids LOWER salience but is session-stable.
    ws.broadcast.push(
        Contribution::context(
            FacultyId::Custom("room-roster".to_string()),
            "STABLE_ROSTER: alice, bob, carol present",
            0.5,
            "framing",
        )
        .session_stable(),
    );

    // Generous budget so BOTH fit — this isolates ORDER, not truncation.
    let block = faculty.render_assembled_context_within(
        &ws,
        4096,
        &GroundingPlan::default(),
        ContextComposition::default(),
    );
    let roster_at = block.find("[room-roster]").expect("roster present");
    let recall_at = block.find("[recall]").expect("recall present");
    assert!(
        roster_at < recall_at,
        "stable framing must serialize before higher-salience volatile recall \
             (roster@{roster_at} should precede recall@{recall_at})\n{block}"
    );
}

// what this catches: the OTHER half of the #266 cache defect — a fixed SET of
// stable contributions emitting DIFFERENT bytes because salience re-ranked them.
//
// Its sibling above pins the tier split (stable before volatile). That shipped,
// and reuse did not follow. The sort under it read
// `sort_by_key(|(c, _)| u8::from(!c.stable))` with a comment calling the
// preserved within-tier salience order harmless. It was the bug: salience is
// recomputed EVERY turn, so the tier whose only job is byte-identity re-shuffled.
// Measured live 2026-08-21 in Atlas's flask-4045 run — consecutive prompts shared
// ~82% of their prefix and diverged where stable blocks traded places, costing
// ~306s of re-prefill per act with `cachedTokens: 0` throughout.
//
// So the tier-split assertion alone could not catch it, and this one is what
// makes the fix hold: SAME SET IN, SAME BYTES OUT, whatever attention ranked.
// If someone reverts to a salience-ordered stable tier, this fails and the
// sibling above still passes.
#[test]
fn stable_tier_bytes_are_identical_however_salience_ranks_them() {
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(
        Uuid::new_v4(),
        "Ivar",
        "You are Ivar.",
        Arc::clone(&adapter),
    );

    // Same three stable blocks, built with per-turn salience + push order.
    // These are the exact faculties that were caught trading places.
    let render = |sal: [f32; 3], order: [usize; 3]| {
        let bodies = [
            ("room-roster", "STABLE_ROSTER: alice, bob, carol present"),
            ("workspace-map", "STABLE_MAP: src/ tests/ docs/"),
            ("room-kanban", "STABLE_BOARD: you hold 9 cards here"),
        ];
        let mut ws = Workspace::new("teammate: what's the plan?");
        for &i in order.iter() {
            ws.broadcast.push(
                Contribution::context(
                    FacultyId::Custom(bodies[i].0.to_string()),
                    bodies[i].1,
                    sal[i],
                    "framing",
                )
                .session_stable(),
            );
        }
        // Generous budget so all three fit — this isolates ORDER, never truncation.
        faculty.render_assembled_context_within(
            &ws,
            4096,
            &GroundingPlan::default(),
            ContextComposition::default(),
        )
    };

    let turn0 = render([0.90, 0.50, 0.30], [0, 1, 2]);
    // Turn 1: attention completely re-ranks them, and they arrive in a new order.
    let turn1 = render([0.30, 0.90, 0.50], [2, 0, 1]);
    // Turn 2: a third ranking, reversed arrival.
    let turn2 = render([0.50, 0.30, 0.90], [2, 1, 0]);

    assert_eq!(
        turn0, turn1,
        "a re-ranked stable tier must emit IDENTICAL bytes — this is the whole \
                 cacheable prefix (#266)\nturn0:\n{turn0}\nturn1:\n{turn1}"
    );
    assert_eq!(
        turn0, turn2,
        "third ranking must also be byte-identical\nturn0:\n{turn0}\nturn2:\n{turn2}"
    );

    // And it is CANONICAL, not merely "whatever arrived first" — an insertion-order
    // block would satisfy the equalities above only by accident of this test's
    // orders. Pin the actual rule so the guarantee is legible.
    let (kanban, map, roster) = (
        turn0.find("[room-kanban]").expect("kanban present"),
        turn0.find("[workspace-map]").expect("map present"),
        turn0.find("[room-roster]").expect("roster present"),
    );
    assert!(
        roster < map && map < kanban,
        "stable tier orders canonically by CHURN CLASS then name \
                 (`deliberation_prompt::stable_prefix_order`): room-roster and \
                 workspace-map are STANDING ground, room-kanban is the BOARD and comes \
                 after both\n{turn0}"
    );

    // The volatile tier keeps salience order ON PURPOSE (#205: most salient
    // nearest the write point). It re-prefills by construction, so canonical
    // ordering would buy no reuse and would cost that placement. Guard the
    // asymmetry, so a future "make it all deterministic" pass has to read this.
    //
    // NOT `active-work`: it is pinned FIRST by an explicit rule ABOVE salience
    // (the ctx_floor reservation only holds if its claimant also leads the
    // greedy walk). Probing the asymmetry with the one salience-immune faculty
    // reports "salience does nothing" about a tier where it does plenty —
    // which is exactly what the first draft of this assertion did.
    let volatile = |sal: (f32, f32)| {
        let mut ws = Workspace::new("teammate: what's the plan?");
        ws.broadcast.push(Contribution::context(
            FacultyId::Recall,
            "VOLATILE_RECALL: deploy was red, fixed 4pm",
            sal.0,
            "recalled",
        ));
        ws.broadcast.push(Contribution::context(
            FacultyId::Custom("affect".to_string()),
            "VOLATILE_AFFECT: mild time pressure",
            sal.1,
            "felt",
        ));
        faculty.render_assembled_context_within(
            &ws,
            4096,
            &GroundingPlan::default(),
            ContextComposition::default(),
        )
    };
    let recall_leads = volatile((0.9, 0.2));
    let work_leads = volatile((0.2, 0.9));
    assert_ne!(
        recall_leads, work_leads,
        "the VOLATILE tier must still follow salience (#205) — if this ever \
                 matches, canonical ordering leaked into the tier that must not have it"
    );
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

// what this catches: a truncation notice a citizen cannot act on. Telling
// her "the full list is available from the matching command" names nothing
// she can type — she cannot run a description, and a verb she has to guess
// is a verb she gets wrong ([[command-names-must-be-accurate]]). The source
// declares its own expansion verb; the notice must print it verbatim, with
// the true total so she knows the size of what she is asking for.
#[test]
fn a_truncated_block_names_the_exact_verb_that_yields_the_rest() {
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter);
    let mut ws = Workspace::new("anything open?");
    let board = board_like(60).with_expand_command(Some("work/list"));
    let total = board.parts.len();
    ws.broadcast.push(board);

    let block = faculty.render_assembled_context_within(
        &ws,
        120,
        &GroundingPlan::default(),
        ContextComposition::default(),
    );
    assert!(
        block.contains("run `work/list`"),
        "the notice must name the verb verbatim, not describe it\n{block}"
    );
    assert!(
        block.contains(&format!("to see all {total}")),
        "…and say how many there are in total, so she knows what she is asking for\n{block}"
    );
}

// what this catches (card e049aa82): a cut the SOURCE made, invisible to her.
// A source that stops short of its own list at its budget mints a
// continuation, and nothing resumes it in production; until now the packer's
// notice counted only its own cut, so a block whose delivered parts all fit
// carried no notice at all, and one it cut further claimed a false total.
// Both must say more exists and name the verb, with no invented total.
#[test]
fn a_block_its_source_cut_short_says_so_even_when_every_part_fits() {
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter);
    let render = |board: Contribution, budget: usize| {
        let mut ws = Workspace::new("anything open?");
        ws.broadcast.push(board);
        faculty.render_assembled_context_within(&ws, budget, &GroundingPlan::default(), ContextComposition::default())
    };

    let fits = render(board_like(2).with_expand_command(Some("work/list")).with_source_truncated(true), 4_000);
    assert!(fits.contains("…more not shown (context budget) — run `work/list` to see all"), "a whole block the source cut must still say so\n{fits}");
    let cut = render(board_like(60).with_expand_command(Some("work/list")).with_source_truncated(true), 120);
    assert!(cut.contains("+ more not shown (context budget)"), "a further cut must count what it left out PLUS the source's remainder\n{cut}");
    assert!(!cut.contains("to see all 62"), "the delivered parts are not the source's total\n{cut}");
    let empty = render(
        Contribution::context(FacultyId::Custom("room-board".to_string()), String::new(), 0.5, "nothing fit".to_string())
            .with_expand_command(Some("work/list"))
            .with_source_truncated(true),
        4_000,
    );
    assert!(empty.contains("…more not shown (context budget) — run `work/list` to see all"), "an empty partial delivery keeps its notice\n{empty}");
    let honest = render(board_like(2).with_expand_command(Some("work/list")), 4_000);
    assert!(!honest.contains("more not shown"), "a complete block carries no notice\n{honest}");
}

// what this catches: the other half — a source with genuinely nothing more
// to fetch must not invent a verb. A pointer to a command that does not
// expand anything is worse than no pointer: she spends a turn on it and
// learns nothing.
#[test]
fn a_source_with_no_expansion_verb_states_only_the_omission() {
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter);
    let mut ws = Workspace::new("anything open?");
    ws.broadcast.push(board_like(60)); // expand_command defaults to None

    let block = faculty.render_assembled_context_within(
        &ws,
        120,
        &GroundingPlan::default(),
        ContextComposition::default(),
    );
    assert!(
        block.contains("more not shown"),
        "still says it truncated\n{block}"
    );
    assert!(
        !block.contains("run `"),
        "must not point at a verb it was never given\n{block}"
    );
}

// what this catches: the board vanishing WHOLE. Measured live 2026-08-06 across
// 1,284 context assemblies, `room-kanban` was KEPT 0 times and dropped 495 — a
// median 5,364-token block offered all-or-nothing into a median 55-token budget
// — while its first two units (~200 tokens) carry every fact a citizen needs to
// find work. The citizens were reporting "there are no open tasks available"
// about a 61-card board. A source that delivered a LIST declared its own cut
// points; assembly must take the longest fitting PREFIX instead of nothing.
#[test]
fn a_divisible_grounding_block_contributes_its_leads_when_the_whole_will_not_fit() {
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter);
    let mut ws = Workspace::new("anything open?");
    let board = board_like(60);
    let whole = estimate_guard_tokens(&board.content);
    ws.broadcast.push(board);

    // A budget FAR under the whole board — the live shape (55 vs 5,364).
    let budget = 120;
    assert!(whole > budget * 10, "fixture must reproduce the live ratio");
    let block = faculty.render_assembled_context_within(
        &ws,
        budget,
        &GroundingPlan::default(),
        ContextComposition::default(),
    );

    assert!(
        block.contains("[room-kanban]"),
        "the board must still reach the prompt — it vanished whole before this fix\n{block}"
    );
    assert!(
        block.contains("[your work]"),
        "the FIRST unit is the one that matters most; a prefix must start there\n{block}"
    );
    assert!(
        !block.contains("card 0000003b"),
        "the 60th card must NOT ride along — the prefix is bounded by budget\n{block}"
    );
    assert!(
        block.contains("more not shown"),
        "a truncated LIST must SAY it is truncated, or a partial board reads as \
                 the whole board and she reports work that isn't there\n{block}"
    );
    assert!(
        estimate_guard_tokens(&block) <= budget,
        "the prefix must respect the budget it was given (got {} for budget {budget})\n{block}",
        estimate_guard_tokens(&block)
    );
}

// what this catches: the SALIENCE INVERSION that `delib.context.salience_inversion`
// reports — an INDIVISIBLE top-salience block vanishing whole while a cheap
// low-salience one rides along. Measured live 2026-08-20 across 25 consecutive
// turns: `workspace-map` (sal=0.90) dropped on 20 of them, grounding at 62-183
// tokens, and Atlas produced 0 acts for 3 days against a checkout she could not
// see. The divisible case is already covered above (a board contributes its
// leads); this is its opposite number — nothing to cut, so it goes to zero.
//
// It also pins the PREMISE THE SENSOR RESTS ON: selection is
// highest-salience-FIRST, so ranking alone can never produce this. If that
// ordering is ever lost, the inversion stops meaning "the budget could not
// afford what mattered" and the probe becomes noise — silently. This test is
// what makes that regression loud.
#[test]
fn an_indivisible_top_salience_block_can_lose_its_slot_to_a_cheaper_lesser_one() {
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter);
    let mut ws = Workspace::new("what should I work on?");

    // The repo map: essential, expensive, and INDIVISIBLE — no `with_parts`,
    // so there is no prefix to fall back to. This is the live shape.
    let map_body = (0..120)
        .map(|i| format!("src/module_{i:03}/mod.rs"))
        .collect::<Vec<_>>()
        .join("\n");
    ws.broadcast.push(Contribution::context(
        FacultyId::Custom("workspace-map".to_string()),
        map_body.clone(),
        0.90,
        "map",
    ));
    // A cheap block that comfortably fits — AT THE SAME SALIENCE. This is the
    // LIVE shape, not a constructed one: on the first turns after the
    // 2281e8b81 deploy every essential source bid exactly 0.90 (Atlas kept
    // `roster(0.90), room-kanban(0.90)` and dropped `workspace-map(0.90)`;
    // Benchy kept `room-board(0.90)` and dropped two others at 0.90). There is
    // no 0.30 to lose to. The first cut of this test used one, which is why it
    // passed against a sensor that could not fire in production.
    ws.broadcast.push(Contribution::context(
        FacultyId::Custom("recall".to_string()),
        "you once renamed a file".to_string(),
        0.90,
        "recall",
    ));

    // Budget affords the small one and nowhere near the map — the live ratio.
    let budget = 40;
    assert!(
        estimate_guard_tokens(&map_body) > budget * 5,
        "fixture must reproduce the live ratio (map {} tok vs budget {budget})",
        estimate_guard_tokens(&map_body)
    );
    let block = faculty.render_assembled_context_within(
        &ws,
        budget,
        &GroundingPlan::default(),
        ContextComposition::default(),
    );

    assert!(
        !block.contains("[workspace-map]"),
        "fixture is not reproducing the inversion — the map was supposed to be \
                 unaffordable at this budget\n{block}"
    );
    assert!(
        block.contains("[recall]"),
        "the cheap low-salience block IS what rides along; without it there is no \
                 inversion, just an empty budget\n{block}"
    );
}

// what this catches: the OTHER half of the same rule — divisibility is opt-in and
// must not leak. An engram is ONE indivisible thing; cutting it mid-sentence
// produces confident nonsense, which is worse than its absence. A faculty that
// never calls `with_parts` must behave exactly as it did before parts existed.
#[test]
fn an_indivisible_contribution_is_still_dropped_whole_never_cut_mid_content() {
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter);
    let mut ws = Workspace::new("what happened yesterday?");
    let engram = "we agreed the gather promise binds every reachable kernel, and that \
                          the in-place consumption path inherits none of the alignment the \
                          copy path establishes, which is why the fallback had to be per-op"
        .repeat(4);
    ws.broadcast.push(Contribution::context(
        FacultyId::Recall,
        engram.clone(),
        0.9,
        "recalled",
    ));

    let block = faculty.render_assembled_context_within(
        &ws,
        40,
        &GroundingPlan::default(),
        ContextComposition::default(),
    );
    assert!(
        !block.contains("[recall]"),
        "an over-budget INDIVISIBLE contribution must be dropped whole, not \
                 truncated into a confident half-thought\n{block}"
    );
    assert!(
        !block.contains("more not shown"),
        "the truncation notice belongs only to genuinely divisible lists\n{block}"
    );
}

// what this catches: a zero/near-zero budget must produce NO block rather than a
// bare `[room-kanban]` header with nothing under it — an empty labelled block
// reads to the persona as "the board is empty", which is exactly the false fact
// this whole fix exists to stop her acting on.
#[test]
fn a_budget_too_small_for_even_one_unit_emits_no_header_at_all() {
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter);
    let mut ws = Workspace::new("anything open?");
    ws.broadcast.push(board_like(60));

    let block = faculty.render_assembled_context_within(
        &ws,
        4,
        &GroundingPlan::default(),
        ContextComposition::default(),
    );
    assert!(
        !block.contains("[room-kanban]"),
        "no unit fits, so there must be no header — a labelled empty block is a \
                 LIE that reads as an empty board\n{block}"
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
// what this catches: churning grounding ahead of the append-only results ring. On the
// M5 (2026-10-18) recall (Turn churn) and the kanban (Claims churn) led the messages,
// every turn's reuse broke there, and the ~30k-token ring behind them re-prefilled at
// ~100 tokens/s. The ring leads; the grounding follows it.
#[test]
fn the_results_ring_renders_before_churning_grounding() {
    let wm = Arc::new(crate::cognition::working_memory::WorkingMemory::new(3));
    let custody = |h: &str| crate::ai::types::ResultCustody::Whole { handle: h.into() };
    wm.record_receipt_typed(&[], "RING-ENTRY older result", None, custody("1111111111111111"));
    wm.record_receipt_typed(&[], "the newest result", None, custody("2222222222222222"));
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(Uuid::new_v4(), "Ivar", "You are Ivar.", adapter)
        .with_context_window(32_768)
        .with_working_memory(Arc::clone(&wm));
    let mut ws = Workspace::new("continue the card");
    ws.broadcast.push(
        Contribution::context(FacultyId::Recall, "RECALL-BLOCK something I remembered", 0.6, "recall")
            .trailing(),
    );
    let view = faculty.prompt_view(&ws);
    let at = |needle: &str| view.messages.iter().position(|m| m.content_text().contains(needle));
    let (ring, recall) = (at("RING-ENTRY"), at("RECALL-BLOCK"));
    assert!(ring.is_some() && recall.is_some(), "both render: ring {ring:?}, recall {recall:?}");
    assert!(ring < recall, "the append-only ring leads, churning grounding follows: ring {ring:?}, recall {recall:?}");
}

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
