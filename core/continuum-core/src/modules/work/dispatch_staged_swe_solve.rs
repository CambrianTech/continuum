use super::*;

/// The #346 claim→solve dispatch. Fires a detached [`crate::commands::agent::solve::AgentSolve`]
/// for the claimer when the claimed card matches a checkout `benchmark/swe-setup` staged
/// into HER workspace: one staged instance directory whose name appears in the card
/// title. Task text = the card body (the real issue, gold held out). Model = the live
/// served base (dynamic — never a hardcoded id). All outcomes on the
/// `benchmark.dispatch` probe; silence is not an outcome.
///
/// No longer fired by `work/claim` (a claim starts nothing; card c4f4b8cd step 1). Its
/// remaining callers are the round driver's edges in `benchmark/dispatch`, `agent/solve`,
/// `benchmark/resume` and the settle edge in `benchmark_grade` — removed in step 2.
pub(crate) async fn dispatch_staged_swe_solve(
    ctx: &Ctx,
    airc: &std::sync::Arc<airc_lib::Airc>,
    dispatch: StagedSolveDispatch,
) {
    let StagedSolveDispatch {
        claimer,
        card: card_id,
        room,
        teammates,
    } = dispatch;
    // ONE PAIR OF HANDS (2026-08-29): a persona's ToolExecutor / file-engine
    // re-root is PROCESS-GLOBAL (see root_acting_workspace / #312), so two
    // concurrent solve forks of the SAME persona contend on the same hands —
    // glass-boxed as ticks parked 25min before perception in exactly the rooms
    // sharing a claimer, reaped by the tick deadline in a retry loop. A second
    // solve for a busy persona is deferred to the round driver's next edge
    // (retry machinery already exists), never run beside the first. This is a
    // MIND fact wearing a mutex: one body, one workspace, one solve at a time.
    let _hands = match HandsLease::try_take(claimer.as_uuid()) {
        Some(lease) => lease,
        None => {
            continuum_base::probe!(
                class = "work.solve.hands_busy",
                card_id = %card_id.as_uuid(),
                claimer = %claimer,
                "claimer already mid-solve — one pair of hands; deferred to the driver's next edge"
            );
            return;
        }
    };
    // Read the RUN ROOM's board — the dispatch HAS the room (StagedSolveDispatch
    // requires it), and boards are per-room. The previous read went through the
    // caller's GLOBAL paginated projection (work_board_complete), where a card
    // beyond the page silently vanished: measured live 2026-08-26 — of two cards
    // on one run-room board, one dispatched and one "was not in the caller's
    // board projection" purely by page position. Room-scoped is both correct
    // and unpaginated for the one board that matters.
    let run_room = {
        let subs = match airc.subscription_set().await {
            Ok(s) => s,
            Err(e) => {
                continuum_base::probe!(
                    class = "benchmark.dispatch",
                    card_id = %card_id.as_uuid(),
                    claimer = %claimer,
                    error = %e.to_string(),
                    "dispatch aborted: caller's subscriptions unreadable — retried on                      the next edge"
                );
                return;
            }
        };
        let found = subs
            .all()
            .into_iter()
            .map(|sub| sub.as_room())
            .find(|r| r.channel == room);
        found
    };
    let Some(run_room) = run_room else {
        continuum_base::probe!(
            class = "benchmark.dispatch",
            card_id = %card_id.as_uuid(),
            claimer = %claimer,
            room = %room.as_uuid(),
            "dispatch aborted: the claimer is not subscribed to the run room (her              subscription may still be resuming post-boot) — retried on the next edge"
        );
        return;
    };
    let Ok(board) = airc.work_board_in(&run_room).await else {
        continuum_base::probe!(
            class = "benchmark.dispatch",
            card_id = %card_id.as_uuid(),
            claimer = %claimer,
            "dispatch aborted: the run room's board could not be read — retried on              the next edge"
        );
        return;
    };
    let board = board.snapshot();
    let Some(card) = board.cards.iter().find(|c| c.card_id == card_id) else {
        continuum_base::probe!(
            class = "benchmark.dispatch",
            card_id = %card_id.as_uuid(),
            claimer = %claimer,
            board_cards = board.cards.len() as u64,
            "dispatch aborted: card not on the RUN room's board (stale card id, or              the board is still replicating) — retried on the next edge"
        );
        return;
    };
    // Her staged SWE checkouts. ONE expression of that layout lives in
    // `persona::staged_workspace` — the work turn roots her hands with the same resolver,
    // so a change to staging can never leave the two disagreeing about which repo a card
    // is about (they did disagree in kind already: this walk accepted any directory, that
    // one requires a `.git`, so an interrupted clone read as a staged instance here).
    // ENV GATE: an instance whose env failed THIS boot's pre-warm cannot grade —
    // firing a solve would burn an attempt on a wall we already measured. Skip
    // with a named abort; the card stays open and re-fires on the first boot
    // whose pre-warm heals it (the automatic retake).
    let env_gate = |inst: &str| crate::modules::work::env_broken_this_boot().contains(inst);
    let (instance, workspace) = match crate::persona::staged_workspace::resolve_for_titles(
        // typed PeerId (canary's #425 struct) → the resolver's raw Uuid
        &claimer.as_uuid(),
        [card.title.as_str()],
    ) {
        crate::persona::staged_workspace::CardWorkspace::One { instance, path } => {
            if env_gate(&instance) {
                continuum_base::probe!(
                    class = "benchmark.dispatch",
                    card_id = %card_id.as_uuid(),
                    instance = %instance,
                    "dispatch deferred: this instance's env failed THIS boot's \
                     pre-warm — an attempt now would burn on a measured wall; the \
                     open card re-fires the first boot the env heals"
                );
                return;
            }
            (instance, path.to_string_lossy().to_string())
        }
        // No staged checkout for her matching this card — an ordinary (non-SWE) claim.
        crate::persona::staged_workspace::CardWorkspace::None => {
            continuum_base::probe!(
                class = "benchmark.dispatch",
                card_id = %card_id.as_uuid(),
                claimer = %claimer,
                "dispatch: no staged checkout matches this card for this claimer —                  ordinary claim, no solve to fire"
            );
            return;
        }
        crate::persona::staged_workspace::CardWorkspace::Ambiguous { candidates } => {
            continuum_base::probe!(
                class = "benchmark.dispatch",
                card_id = %card_id.as_uuid(),
                claimer = %claimer,
                matches = candidates.len(),
                candidates = candidates.join(","),
                "claim matched MULTIPLE staged instances — refusing to guess, no dispatch"
            );
            return;
        }
    };
    // ONE CARD, ONE LIVE RUN — a STOPGAP, and named as one (2026-08-21).
    //
    // WHAT THIS IS, honestly: a doorman with a clipboard. A citizen re-affirming a claim
    // is an ordinary, legitimate act — a student saying "this one's mine" a second time.
    // The school's answer today is to seat a SECOND her at a second desk with a copy of
    // the same worksheet, in the same room, both writing on it. This gate turns that into
    // a refusal at the door. It stops the damage; it does not remove the incoherence.
    //
    // THE INCOHERENCE, which is the real defect: we keep TWO REGISTRIES FOR ONE FACT.
    // The board holds "this citizen holds this card" (enrollment). A JSON file on disk
    // holds "run claim-<id> is running" (attendance). Two records of one truth, so they
    // can and do disagree — and this gate is a clerk reconciling them on every claim.
    // The structural fix is that the ENROLLMENT IS THE SESSION: re-affirming a claim
    // finds the work already under way and continues it, because there is only one place
    // the fact lives and nothing to duplicate. That is #371 (round lifecycle as
    // recipe-owned state) plus #49 (the workspace held for the life of the claim, so the
    // worksheet is hers rather than a directory two runs can both open). Neither is
    // built; until they are, this doorman is the thing standing between a citizen and a
    // clobbered afternoon.
    //
    // `run_id` is DETERMINISTIC per card (`claim-<uuid>`), and this dispatch had no
    // guard, so every re-claim of the same card fired ANOTHER detached solve. Two runs
    // then shared, simultaneously: one ledger file (their pulse ticks overwrite each
    // other, so `benchmark/runs` reports whichever wrote last — an honest-looking
    // `acts: 0` that is really run B stomping run A's marker), one WORKSPACE (two sets
    // of hands editing the same repo, which is a live candidate for the empty
    // `files_changed` this box keeps producing), and one exclusive warm slot they each
    // ask to quiesce the other out of.
    //
    // Observed: Atlas re-claimed card df9f4ad5 three times in one evening on
    // pallets__flask-4045; `benchmark/runs` caught the third at `age_secs: 8, acts: 0`.
    // A re-claim is LEGAL (airc permits claiming a lapsed card, and #331/#2286 both
    // depend on that) — what is not legal is a second RUN. The claim is the citizen's
    // intent; the run is the machinery, and the machinery must be idempotent under a
    // repeated intent.
    //
    // Keyed on `in_flight_solve_runs` — the SAME `state: "running"` predicate the boot
    // reaper uses, so this gate and the reaper can never disagree about what "live"
    // means. That also gives the gate its release valve for free: a run whose core died
    // is reaped to `state: "failed"` at next boot, so a dead run never blocks its card
    // forever. It is a refusal, not a kill: the live run keeps working, and nothing
    // touches the citizen's claim.
    let run_id = format!("claim-{}", card_id.as_uuid());
    if let Some((_, live_instance)) = crate::cognition::swe_bench::in_flight_solve_runs()
        .into_iter()
        .find(|(id, _)| id == &run_id)
    {
        continuum_base::probe!(
            class = "benchmark.dispatch",
            card_id = %card_id.as_uuid(),
            claimer = %claimer,
            instance = %instance,
            run_id = %run_id,
            live_instance = %live_instance,
            "re-claim of a card whose run is ALREADY IN FLIGHT — refusing to start a \
             second solve. A duplicate would share this run's ledger, its workspace and \
             its warm slot, and silently discard whichever half lost the race. The live \
             run continues; the claim stands."
        );
        return;
    }

    // WAIT for the boot-gate, don't guard against it. A claim can land while the serving lane
    // is still proving it can decode (the ~10-15s window after core-ready); parking here until
    // the lane is decode-verified means the solve fires the moment serving is up instead of the
    // claim silently no-op-ing (Joel 2026-08-11: "persona should boot beforehand"). None after
    // the deadline = a genuinely dead lane; the claim stands and re-fires on the next serving edge.
    let model =
        crate::inference::llama_server::await_ready_serving(std::time::Duration::from_secs(30))
            .await
            .and_then(|s| s.active_model)
            .unwrap_or_default(); // no recorded activity yet = first dispatch of this card; mint path follows
    if model.is_empty() {
        continuum_base::probe!(
            class = "benchmark.dispatch",
            card_id = %card_id.as_uuid(),
            claimer = %claimer,
            instance = %instance,
            "serving not decode-ready within 30s — dispatch skipped; claim stands, re-fires on next serving edge"
        );
        return;
    }
    // THE SOLVE'S OWN ACTIVITY — mint-or-rejoin (Joel 2026-08-26: "benchmarks
    // without new activities (unless rejoining)" is not allowed; rooms are 1:1
    // with activities). The run room stays the BOARD's home (cards, kickoffs,
    // the round's denominator); the solve itself is its own activity: a child
    // room per (card, instance) where her acts radiate. This is also the KV
    // fix becoming real — each concurrent solve's (persona, room) key leases
    // its OWN warm slot instead of N solves thrashing one.
    //
    // REJOIN: the round recorded this card's activity at first mint (resume
    // after a reboot, a retry attempt — same room, continuity). MINT: spawn a
    // real activity room, child of the run room, and record it.
    let solve_room = match crate::cognition::bench_round::card_activity(card_id.as_uuid()) {
        Some(mut act) => {
            continuum_base::probe!(
                class = "work.solve.room_rejoined",
                card_id = %short8(card_id.as_uuid()),
                claimer = %short8(claimer.as_uuid()),
                room = %act.solve_room,
                "solve REJOINS its recorded activity room — resume and dispatch are one motion"
            );
            // BACKFILL the room's name for activities recorded before names
            // existed: the mint name is deterministic (`swe--<instance>--<card8>`),
            // so a rejoin can restore it — and with it, presence adoption +
            // nav for this room (#2606). Idempotent; new records carry it.
            if act.room_name.is_empty() {
                act.room_name = format!("swe--{}--{}", instance, short8(card_id.as_uuid()));
                crate::cognition::bench_round::record_card_activity(card_id.as_uuid(), act.clone());
            }
            act.solve_room
        }
        None => {
            // Spawn AS the claimer when her runtime is live (she is joined to her
            // own workroom); the caller's handle is the fallback so a dispatch
            // fired before she is resident still names a real activity.
            let spawner =
                crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global()
                    .and_then(|reg| reg.get(claimer.as_uuid()))
                    .map(|rt| rt.airc().clone())
                    .unwrap_or_else(|| airc.clone()); // assignee runtime gone → curator's own handle spawns; probed below
            let name = format!("swe--{}--{}", instance, short8(card_id.as_uuid()));
            let recipe = crate::experience::source::RecipeExperienceSource::shipped_purpose(
                crate::experience::source::shipped::BENCHMARK_HARD_RS,
            )
            .unwrap_or_default(); // empty name renders as unnamed room in the probe; display only
            match crate::modules::activity::spawn_activity_room(
                &spawner,
                &name,
                &recipe,
                Some(room),
                &std::collections::BTreeMap::new(),
                None,
                None, // no pipeline on this recipe
            )
            .await
            {
                Ok(spawned) => {
                    // RESTORE THE SPAWNER'S FOCUS (glass-boxed 2026-08-26, the
                    // one-resident boot): spawn_activity_room's documented side
                    // effect moves the caller's current-room pointer to the new
                    // solve room — and when the dispatch's curator IS the
                    // assignee (any_live_citizen with one resident), that same
                    // handle creates the NEXT card, which then lands on the
                    // SOLVE room's board instead of the run room's. Measured:
                    // card 1 of 3 visible, cards 2-3 "not on the RUN room's
                    // board" forever — two solves dead at dispatch and a live
                    // card ghost-settled. Until airc grows subscribe-without-
                    // focus (#290), the mint restores the pointer itself.
                    if let Err(e) = spawner.join(&run_room.name).await {
                        continuum_base::probe!(
                            class = "work.solve.room_mint_failed",
                            card_id = %short8(card_id.as_uuid()),
                            error = %e.to_string(),
                            "could not restore the spawner's focus to the run room                              after the mint — subsequent card writes may land on the                              wrong board"
                        );
                    }
                    let act = crate::cognition::bench_round::CardActivity {
                        teammates: teammates.iter().map(|p| p.as_uuid()).collect(),
                        solve_room: spawned.room_id.as_uuid(),
                        room_name: name.clone(),
                        assignee: claimer.as_uuid(),
                    };
                    crate::cognition::bench_round::record_card_activity(
                        card_id.as_uuid(),
                        act.clone(),
                    ); // clone: probe below still reads the local
                    continuum_base::probe!(
                        class = "work.solve.room_minted",
                        card_id = %short8(card_id.as_uuid()),
                        claimer = %short8(claimer.as_uuid()),
                        room = %act.solve_room,
                        name = %name,
                        "solve MINTED its own activity room (child of the run room)"
                    );
                    act.solve_room
                }
                Err(e) => {
                    // A mint failure must not strand the work invisible: fall back
                    // to the run room (a real activity, just coarser-grained) and
                    // say so. The next re-fire retries the mint.
                    continuum_base::probe!(
                        class = "work.solve.room_mint_failed",
                        card_id = %short8(card_id.as_uuid()),
                        claimer = %short8(claimer.as_uuid()),
                        error = %e.to_string(),
                        "activity mint failed — solve runs in the RUN room this time (coarser \
                         KV granularity, still visible); mint retries on the next fire"
                    );
                    room.as_uuid()
                }
            }
        }
    };

    // TEAM MEMBERSHIP (#team-proof gap 1): teammates JOIN the solve room and
    // receive an addressed charge. Membership is room-level (their normal
    // multi-room cognition participates from here on); accountability stays
    // card-level with the single claimer. Join is idempotent; a failed join or
    // invite is REPORTED and never blocks the solve (the claimer works either
    // way — a smaller team is a degraded run, not a dead one).
    if !teammates.is_empty() {
        let team_room_name = format!("swe--{}--{}", instance, short8(card_id.as_uuid()));
        for mate in &teammates {
            let Some(rt) =
                crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global()
                    .and_then(|reg| reg.get(mate.as_uuid()))
            else {
                continuum_base::probe!(
                    class = "work.team.join_skipped",
                    card_id = %short8(card_id.as_uuid()),
                    mate = %short8(mate.as_uuid()),
                    "teammate not resident — solve proceeds with a smaller team"
                );
                continue;
            };
            if mate.as_uuid() == claimer.as_uuid() {
                // Teamed with yourself is a roster accident, not a team.
                continue;
            }
            if let Err(e) = rt.join_room(&team_room_name).await {
                continuum_base::probe!(
                    class = "work.team.join_failed",
                    card_id = %short8(card_id.as_uuid()),
                    mate = %short8(mate.as_uuid()),
                    error = %e.to_string(),
                    "teammate join failed — reported, never blocks the claimer"
                );
                continue;
            }
            // AUTHOR THE CHARGE AS SOMEONE ELSE — the same law the kickoff path
            // learned (#417, benchmark.rs "AUTHOR IT AS SOMEONE ELSE"): a citizen's
            // inbound stream skips messages she is recorded as having said, so a
            // charge published through the MATE'S OWN airc is dropped by her
            // self-filter and never wakes her. Measured on the first team round
            // (2026-08-30): Kira held the review charge in five rooms and produced
            // zero turns in 28 minutes — the charge was her own voice. The natural
            // author is the CLAIMER (it is their patch being reviewed); fall back
            // to any other live citizen, and skip loudly rather than send a
            // message that cannot be heard.
            let registry =
                crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global();
            let voice_rt = registry
                .as_ref()
                .and_then(|reg| reg.get(claimer.as_uuid()))
                .or_else(|| {
                    registry
                        .as_ref()
                        .and_then(|reg| reg.any_live_citizen_other_than(Some(mate.as_uuid())))
                });
            let Some(voice_rt) = voice_rt else {
                continuum_base::probe!(
                    class = "work.team.charge_unvoiced",
                    card_id = %short8(card_id.as_uuid()),
                    mate = %short8(mate.as_uuid()),
                    "no live citizen other than the mate can voice the charge — \
                     skipped loudly; a self-authored charge would be self-filtered"
                );
                continue;
            };
            // Membership is what makes the room deliverable for the voice too
            // (idempotent; same rule as the kickoff's voice-join).
            if let Err(e) = voice_rt.join_room(&team_room_name).await {
                continuum_base::probe!(
                    class = "work.team.voice_join_failed",
                    card_id = %short8(card_id.as_uuid()),
                    mate = %short8(mate.as_uuid()),
                    error = %e.to_string(),
                    "charge voice could not join the solve room — charge skipped"
                );
                continue;
            }
            let claimer_name = registry
                .as_ref()
                .and_then(|reg| reg.get(claimer.as_uuid()))
                .map(|c| c.agent_name().to_string())
                .unwrap_or_else(|| short8(claimer.as_uuid()));
            let charge = format!(
                "@{mate_name} (to you): you are teamed with {claimer_name} on \
                 `{instance}` in this room. Their patch must be reviewed by a \
                 teammate before it submits: read the diff when it lands, run what \
                 you can, and SPEAK your findings here — a miss a reviewer catches \
                 is the whole point of the team.",
                mate_name = rt.agent_name(),
            );
            match crate::persona::airc_citizen::publish_text_in_room(
                voice_rt.airc(),
                solve_room,
                &charge,
            )
            .await
            {
                Ok(_) => continuum_base::probe!(
                    class = "work.team.joined",
                    card_id = %short8(card_id.as_uuid()),
                    mate = %short8(mate.as_uuid()),
                    room = %solve_room,
                    "teammate joined the solve room and holds the review charge"
                ),
                Err(e) => continuum_base::probe!(
                    class = "work.team.invite_failed",
                    card_id = %short8(card_id.as_uuid()),
                    mate = %short8(mate.as_uuid()),
                    error = %e.to_string(),
                    "teammate joined but the charge did not send — reported"
                ),
            }
        }
    }
    // Her HANDS must resolve `python`/`pytest`/`pip` to THIS instance's venv, not the system
    // interpreter. Without this, `code/shell pytest` hits homebrew python3.14 (no pytest, no
    // repo), she loops `pip install pytest` into the wrong interpreter, and burns every action
    // without ever validating her edit (glass-boxed 2026-08-11 from Anon's astropy turn). The
    // venv is built at staging (benchmark.rs) / on first grade; the path is deterministic, and
    // solve.rs already `.exists()`-filters it, so prepending a not-yet-built bin is harmless.
    let venv_bin = crate::cognition::swe_bench::swe_cache_dir()
        .join("envs")
        .join(&instance)
        .join("bin")
        .to_string_lossy()
        .to_string();
    let params = crate::commands::agent::solve::AgentSolveParams {
        persona_id: claimer.to_string().into(),
        base_model_id: model,
        workspace,
        task: card.body.clone().unwrap_or_else(|| card.title.clone()),
        deliverable: Some(crate::commands::agent::solve::Deliverable::Workspace),
        scored: Some(true),
        detach: Some(true),
        // The SAME expression the in-flight gate above keyed on — one definition of
        // "this card's run id", so the guard can never check a different id than the
        // dispatch actually uses.
        run_id: Some(run_id.clone()),
        // CAPTURE the detached solve's turns (was None → the whole scored run was
        // INVISIBLE: a reviewer — Opus or a self-grading citizen — could not read a single
        // tool-call output to verify it, forcing inference from a STALE main-loop capture
        // and a near-misdiagnosis 2026-08-25). eval.rs already scopes this per-run
        // (run_artifact_dir), so it does not collide with the main-loop sink; a measured run
        // is meant to be inspectable (eval.rs `capture_dir` doc). Same base dir the tooling
        // (`dataset/from-captures`) and the main-loop sink use.
        capture_dir: std::env::var("HOME").ok().map(|h| {
            std::path::Path::new(&h)
                .join(".continuum/fixtures/prompt-captures")
                .to_string_lossy()
                .to_string()
        }),
        learn: crate::cognition::learning_policy::LearningPolicy::LearnFromThisWork,
        max_acts: None,
        // The solve's OWN activity (minted-or-rejoined above) — NOT the run room.
        // `AgentSolveParams::room` stays `Option` because `agent/solve` is also an
        // operator-invocable command (an omitted room mints there).
        room: Some(solve_room),
        path_prepend: Some(vec![venv_bin]),
        suppress_recall: None,
        prev_failed_patch_sha: None,
        // The SWE claim adapter's N (Joel, 2026-08-08): a failed grade re-enters the
        // same workspace with the named failing tests — learning to investigate your
        // own failure is part of the exam. Three chances: first attempt, one informed
        // retry, one consolidation — beyond that the failures repeat, not teach.
        attempts: Some(SWE_CLAIM_ATTEMPTS),
        // Participants stay awake through the measured-work quiesce (gap 3) —
        // the reviewers this dispatch just joined into the solve room.
        teammates: teammates.iter().map(|m| m.as_uuid()).collect(),
    };
    match crate::commands::agent::solve::AgentSolve
        .run(ctx, params)
        .await
    {
        Ok(ack) => continuum_base::probe!(
            class = "benchmark.dispatch",
            card_id = %card_id.as_uuid(),
            claimer = %claimer,
            instance = %instance,
            run_id = %ack.run_id.unwrap_or_default(),
            "claim dispatched a detached work session"
        ),
        Err(e) => continuum_base::probe!(
            class = "benchmark.dispatch",
            card_id = %card_id.as_uuid(),
            claimer = %claimer,
            instance = %instance,
            error = %e.to_string(),
            "claim→solve dispatch FAILED — claim stands, session missing"
        ),
    }
}

// ─────────────────────────── work/create ─────────────────────────
/// Create a new work card on the shared board.
pub struct WorkCreate {
    pub registry: PersonaAircRuntimeRegistry,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct WorkCreateParams {
    /// The room whose board gets the card (id or name).
    // Required: a "current room" default put project cards in #general.
    pub room: String,
    /// owner/name; defaults to actionable card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub repo: Option<String>,
    /// Card title.
    pub title: String,
    /// Card body.
    #[serde(default)]
    pub body: Option<String>,
    /// p0 (urgent) to p3 (whenever). Defaults to p2.
    #[serde(default)]
    pub priority: Option<CardPriority>,
}

// A card's priority on the wire: a closed set serde refuses anything outside, never a
// string read loosely into a default. (`//`, not `///`: a doc comment ships in her tool schema.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CardPriority {
    P0,
    P1,
    P2,
    P3,
}

impl From<CardPriority> for Priority {
    fn from(priority: CardPriority) -> Self {
        match priority {
            CardPriority::P0 => Priority::P0,
            CardPriority::P1 => Priority::P1,
            CardPriority::P2 => Priority::P2,
            CardPriority::P3 => Priority::P3,
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct WorkCreateResult {
    pub card_id: String,
}

#[async_trait]
impl ActionCommand for WorkCreate {
    const NAME: &'static str = "work/create";
    // Making cards toward an activity's goals is every participant's work (Joel,
    // 2026-09-28): a citizen who could claim and move cards but not create them could not
    // break a project into slices. The card is created under HER airc identity.
    const NATIVE: bool = true;
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Create a card on a room's board for known work (a slice, follow-up, review).";
    type Params = WorkCreateParams;
    type Output = WorkCreateResult;

    async fn run(&self, ctx: &Ctx, p: WorkCreateParams) -> Result<WorkCreateResult, CommandError> {
        let airc = persona_airc(&self.registry, ctx, "work commands")?;
        Self::create(&airc, p).await
    }
}

/// The repo of the card she is focused on, read from the board (the holds authority
/// her turns use; the roster can be empty after a reboot while the board holds work)
/// and chosen by the same focus rule. A failed read is a read failure, never "you hold
/// no card" (Codex on #4571).
pub(super) async fn held_repo(airc: &Airc) -> Result<Option<RepoId>, CommandError> {
    let held = crate::persona::airc_runtime::board_held_by(airc)
        .await
        .map_err(|e| CommandError::Internal(format!("work/create: could not read the cards you hold: {e}")))?;
    Ok(crate::persona::work_focus::focus_actionable_card(held.iter()).map(|c| c.repo.clone()))
}

/// The repo the ROOM declares: a project activity's recipe binding carries `params.repo`
/// (project.json), bound at `activity/spawn`. `Ok(None)` = the room is not a project or
/// declares none (an org room like cambriantech has no binding at all, and that is right:
/// an org is not one repo). A read failure is a read failure.
pub(super) async fn room_repo(airc: &Airc, room: &airc_lib::Room) -> Result<Option<RepoId>, CommandError> {
    let posts = airc
        .wall_posts_in(room, Some(crate::experience::binding::RECIPE_WALL_CATEGORY))
        .await
        .map_err(|e| CommandError::Internal(format!("work/create: could not read the room's recipe binding: {e}")))?;
    let binding = crate::experience::binding::project_binding(&posts)
        .map_err(|e| CommandError::Internal(format!("work/create: the room's recipe binding is unreadable: {e}")))?;
    let Some(repo) = binding.as_ref().and_then(|b| b.declared_repo().map(str::to_string)) else {
        return Ok(None);
    };
    RepoId::new(repo.clone())
        .map(Some)
        .map_err(|e| CommandError::Internal(format!("work/create: the room declares repo {repo:?}, which is not a repo key: {e:?}")))
}

/// The distinct repos of every card she holds, in any state, for the refusal's hint.
/// Best-effort: an unreadable board names no repos; the refusal still stands.
pub(super) async fn repos_she_holds(airc: &Airc) -> Vec<String> {
    let Ok(held) = crate::persona::airc_runtime::board_held_by(airc).await else {
        return Vec::new();
    };
    let mut repos: Vec<String> = held.iter().map(|c| c.repo.to_string()).collect();
    repos.sort();
    repos.dedup();
    repos
}

/// The words when nothing supplies a repo: her one argument first, with an example
/// she can copy; the room's option second; the org-room fact last.
pub(crate) fn repo_refusal(room: &str, repos_she_holds: &[String]) -> String {
    let example = repos_she_holds.first().map(String::as_str).unwrap_or("owner/name");
    let seen = if repos_she_holds.is_empty() {
        String::new()
    } else {
        format!(" Repos of cards you hold: {}.", repos_she_holds.join(", "))
    };
    format!(
        "work/create: add repo to this call, e.g. repo=\"{example}\".{seen} Room {room:?} declares no repo \
         (a project room can, at activity/spawn --params {{\"repo\":\"owner/name\"}}; an org room never does), \
         and no card you hold is actionable here."
    )
}
