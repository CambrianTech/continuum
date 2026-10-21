use super::*;

impl AgentSolve {

    pub(super) async fn solve_body_attended_by_nobody(p: AgentSolveParams) -> Result<AgentSolveResult, CommandError> {
        let run_id = p.run_id.clone();
        // Short-form persona ids resolve too (#164): rosters/benchmark harnesses DISPLAY
        // 8-char short ids, so accept the id a caller was shown — a clean UUID passes
        // straight through, a short/mistyped form expands against the live persona registry
        // (the ONE shared id_resolve primitive), instead of failing "is not a UUID".
        let persona_uuid = continuum_base::id_resolve::resolve(
            p.persona_id.as_str().trim(),
            &crate::persona::card::ids(),
            "persona",
        )
        .map_err(CommandError::Invalid)?;
        let workspace = p.workspace.trim().to_string();
        if !std::path::Path::new(&workspace).is_dir() {
            return Err(CommandError::Invalid(format!(
                "workspace '{workspace}' does not exist or is not a directory"
            )));
        }
        let max_acts = p.max_acts.unwrap_or(DEFAULT_MAX_ACTS).max(1) as usize;

        // 1) Stand up a dedicated measurement lane for the model (her genome pages in on top),
        //    exactly as cognition/eval does — held for the whole drive, dropped after.
        //
        //    BOUNDED, loudly (glass-boxed 2026-08-08, n8/n11: both forks sat 2h+ with
        //    ZERO generations, parked somewhere inside lane acquisition — three
        //    candidate parks (warm-pool spawn gate held by a wedged cold-load; the
        //    share-check's adapter.initialize() HTTP round-trip against a saturated
        //    lane, whose sibling endpoint is DOCUMENTED to block mid-generation;
        //    pressure defer) and no receipt discriminated them because the whole
        //    acquisition was one silent await. The timeout converts any park into a
        //    loud named error; the bracket probes make the NEXT stall name its line.
        // Admission gate (held for the whole drive, released on drop): WAIT for one of the
        // serving-lane solve slots before acquiring/using a lane, so a dispatch fan-out +
        // claim + restore can never run more solves than the box has lanes and thrash one to
        // a Connection-refused death mid-generation. `.ok()` because the semaphore is never
        // closed; binding to `_solve_permit` keeps the permit alive across the drive.
        let _solve_permit = solve_admission().acquire().await.ok();
        continuum_base::probe!(
            class = "benchmark.solve.phase",
            run_id = %run_id.as_deref().unwrap_or("-"), // unwrap_or: probe label only, "-" marks an unnamed run
            phase = "admission.acquired",
            available_slots = solve_admission().available_permits() as u64,
            "solve admitted — holding one serving-lane solve slot for the drive"
        );
        // 1a) Quiesce BEFORE the lane, and let the plan RESETTLE before anything
        //     pins it (glass-boxed 2026-08-11, Atlas on sympy-24152). The old order
        //     was lane-then-quiesce, which built a catch-22: the lease's lowered
        //     lane demand (#2238/#2239) would let the planner collapse a crowded
        //     4-lane × 16k layout into one big-window lane — but by then the solve
        //     already held the lane, and the reconcile (correctly) refuses to
        //     relaunch under a live measurement ("eval holds the lane steady"). The
        //     solve froze the cramped layout under itself, and the persona worked a
        //     SWE repo through a ~400-token keyhole: 13.1k of her 16.4k window was
        //     fixed overhead (tools 6300 + framing 2741 + completion reserve 4096),
        //     over-window on every single compose. Quiesce-first + an EVENT-GATED
        //     settle (the daemon's own snapshot watch — never a sleep-poll) means
        //     the lane below is acquired against the settled big-window plan, and
        //     `lane.served_ctx` carries it into her fork with no further plumbing.
        //     Bounded loudly: a planner that keeps the layout is a legitimate
        //     outcome, so `Unchanged` proceeds — it can never park the solve.
        // The measured-work hold (restore-economy 1.a): the quiesce lease below
        // pauses the other citizens' SERVICE LOOPS, but background cognition
        // (dreams, module inference) never runs in those loops — measured
        // 2026-08-28, dream-belief-review took 52 of 109 generations during a
        // held solve and evicted the warm KV each time (~32.9s re-prefill vs
        // ~0.1s restore). The hold is consulted at the adapter seam, where every
        // generation crosses; RAII, so any exit path releases it.
        let _measured_hold = crate::inference::measured_hold::acquire(
            persona_uuid,
            run_id.as_deref().unwrap_or("agent-solve"),
        );
        // PARTICIPANTS stay awake, not just the solver (team-proof gap 3): a
        // teammate holding the review charge must be able to take turns DURING
        // the solve — the first team round (2026-08-30) quiesced the reviewers
        // and made review structurally impossible. The lease's demand override
        // counts the whole except-set, so serving budgets one warm slot per
        // participant.
        let mut participants = vec![persona_uuid];
        participants.extend(p.teammates.iter().copied());
        let _quiesce_lease =
            crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global().map(
                |reg| {
                    let lease = reg.lease_measured_work_excluding(&participants);
                    continuum_base::probe!(
                        class = "benchmark.solve.phase",
                        run_id = %run_id.as_deref().unwrap_or("-"), // unwrap_or: probe label only, "-" marks an unnamed run
                        phase = "lease_measured_work",
                        quiesced_peers = lease.count() as u64,
                        awake_participants = participants.len() as u64,
                        "measured solve holds an exclusive warm slot — non-participants quiesced so the KV prefix survives turn-to-turn"
                    );
                    lease
                },
            );
        if let Some(rx) = crate::inference::llama_server::serving_state_receiver() {
            let pre = crate::inference::llama_server::current_serving();
            // Wide enough for one planner tick + a resident-model relaunch (weights
            // already on disk); a settle that needs longer is a serving fault the
            // lane-acquire timeout below will name, not something to wait out here.
            const SERVING_RESETTLE_BOUND: Duration = Duration::from_secs(180);
            let outcome = crate::inference::llama_server::await_snapshot_resettle(
                rx,
                pre.lanes,
                pre.served_context_window,
                SERVING_RESETTLE_BOUND,
            )
            .await;
            match outcome {
                crate::inference::llama_server::SnapshotSettle::Resettled { lanes, window } => {
                    continuum_base::probe!(
                        class = "benchmark.solve.phase",
                        run_id = %run_id.as_deref().unwrap_or("-"), // unwrap_or: probe label only, "-" marks an unnamed run
                        phase = "serving_resettled",
                        pre_lanes = pre.lanes as u64,
                        pre_window = pre.served_context_window as u64,
                        lanes = lanes as u64,
                        window = window as u64,
                        "quiesce lowered demand and the plan RESETTLED — the solve \
                         drives on the settled layout instead of freezing the crowded one"
                    );
                }
                crate::inference::llama_server::SnapshotSettle::Unchanged => {
                    continuum_base::probe!(
                        class = "benchmark.solve.phase",
                        run_id = %run_id.as_deref().unwrap_or("-"), // unwrap_or: probe label only, "-" marks an unnamed run
                        phase = "serving_unchanged",
                        lanes = pre.lanes as u64,
                        window = pre.served_context_window as u64,
                        "plan held its layout under lowered demand — proceeding on the \
                         current lanes/window (legitimate: it may already be optimal)"
                    );
                }
            }
        }
        continuum_base::probe!(
            class = "benchmark.solve.phase",
            run_id = %run_id.as_deref().unwrap_or("-"), // unwrap_or: probe label only, "-" marks an unnamed run
            phase = "lane_acquire.start",
            base_model = %p.base_model_id,
            "solve prelude: acquiring measurement lane"
        );
        const LANE_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(15 * 60);
        let lane = match tokio::time::timeout(
            LANE_ACQUIRE_TIMEOUT,
            crate::cognition::eval::spawn_base_eval_lane(&p.base_model_id),
        )
        .await
        {
            Ok(lane) => lane?,
            Err(_) => {
                continuum_base::probe!(
                    class = "benchmark.solve.phase",
                    run_id = %run_id.as_deref().unwrap_or("-"), // unwrap_or: probe label only, "-" marks an unnamed run
                    phase = "lane_acquire.timeout",
                    base_model = %p.base_model_id,
                    "lane acquisition exceeded its bound — INFRA fault, run ends loudly"
                );
                return Err(CommandError::Internal(format!(
                    "measurement-lane acquisition for '{}' exceeded {}s — an infra \
                     stall (spawn gate, share-check HTTP, or pressure defer), never a \
                     capability verdict. See eval.lane.* / benchmark.solve.phase probes \
                     for the parked step.",
                    p.base_model_id,
                    LANE_ACQUIRE_TIMEOUT.as_secs()
                )));
            }
        };
        continuum_base::probe!(
            class = "benchmark.solve.phase",
            run_id = %run_id.as_deref().unwrap_or("-"), // unwrap_or: probe label only, "-" marks an unnamed run
            phase = "lane_acquire.done",
            "solve prelude: lane acquired"
        );

        // (The exclusive-warm-slot quiesce lease — KV-prefix protection, panic-safe
        // RAII, she is never suspended, only idle contenders' autonomic ticks —
        // is acquired in step 1a ABOVE the lane, so its lowered demand shapes the
        // plan the lane is acquired against. [[benchmark-is-a-governor-preemption-lease]]
        // [[first-class-citizens-even-during-benchmarks]])

        // 2) Fork her WHOLE cognition onto that lane, rooted at the workspace: tools ON, recall ON.
        //    A brief wait covers the post-spawn template race (same as the eval fork-waiter).
        let registry = crate::cognition::persona_workspace::global();
        // The MISSION rides as standing framing, not as the opening burst alone (#390,
        // glass-boxed 2026-08-12 on pytest-5221): the task text delivered once was evicted
        // from every captured prompt after act ~6 of a 24-act solve, and the persona
        // literally asked "could you describe the symptoms of the issue?" — anchor loss,
        // the dominant patch-0 shape. A `[mission]` StandingFraming block survives every
        // compose of the drive, exactly like the pinned board (#347). The burst below
        // still fires — it is the directed TRIGGER; this is the PERSISTENCE.
        let mission = std::sync::Arc::new(crate::persona::mission_source::MissionSource::new(
            persona_uuid,
            format!(
                "YOUR ONE JOB this whole session (re-read this every step):\n{}\n\nWork in \
                 `{workspace}` — that directory IS the task's repo. The deliverable is the \
                 edit your tools leave there; a session that only reads has failed.",
                p.task.trim()
            ),
        ));
        continuum_base::probe!(
            class = "benchmark.solve.phase",
            run_id = %run_id.as_deref().unwrap_or("-"), // unwrap_or: probe label only, "-" marks an unnamed run
            phase = "fork.start",
            "forking her cognition onto the measurement lane"
        );
        let mut cycle = None;
        for attempt in 0..FORK_WAIT_TRIES {
            cycle = registry.fork_eval_cycle_with_adapter(
                &persona_uuid,
                lane.adapter.clone(),
                lane.served_ctx,
                true,                               // with_tools — her hands are ON
                Some(&workspace),                   // roots the ToolExecutor at the sandbox cwd
                p.suppress_recall.unwrap_or(false), // memory/RAG ON by default; the diagnostic knob
                vec![
                    crate::cognition::persona_workspace::GroundingSource::framing(mission.clone()),
                ],
            );
            if cycle.is_some() {
                break;
            }
            if attempt + 1 < FORK_WAIT_TRIES {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
        let cycle = cycle.ok_or_else(|| {
            CommandError::NotFound(format!(
                "no workspace template for persona {persona_uuid} — spawn her (register_from_cfg) \
                 before agent/solve can fork a measurement copy of her mind"
            ))
        })?;

        // 2b) Root her HANDS at the sandbox cwd. `fork_eval_cycle_with_adapter`'s `workspace_root`
        //     only repoints what she SEES (the workspace-map RAG block) — her `ToolExecutor` still
        //     writes to her durable per-persona workspace. Without this she writes reverse.py to
        //     `<home>/citizens/peers/<id>/workspace/` and our `git diff` on the sandbox scores a
        //     false ZERO (glass-boxed 2026-07-22: Devstral did 2 real acts, wrote the correct file,
        //     patch was empty). Same fail-loud mechanism cognition/eval uses to root a measurement
        //     persona at a target repo.
        //
        //     The re-root is PROCESS-GLOBAL and outlives this fork (`code/create-workspace`
        //     keys the file engine on the caller identity, and the fork shares the living
        //     persona's executor AND her id — see `ActingHands`). So the hands handle is
        //     lifted out BEFORE the cycle is consumed, and every exit path below returns her
        //     to her own workspace. Without that, #312: after a flask solve, Anwen's LIVE
        //     self was still running `code/read(src/flask/app.py)` in her room hours later.
        continuum_base::probe!(
            class = "benchmark.solve.phase",
            run_id = %run_id.as_deref().unwrap_or("-"), // unwrap_or: probe label only, "-" marks an unnamed run
            phase = "fork.done",
            "cognition forked — rooting hands at the sandbox"
        );
        let hands = crate::cognition::persona_workspace::ActingHands::of(&cycle);
        crate::cognition::persona_workspace::root_acting_workspace(
            &cycle,
            &workspace,
            p.path_prepend.as_deref().unwrap_or(&[]),
            p.scored.unwrap_or(false),
        )
        .await?;
        continuum_base::probe!(
            class = "benchmark.solve.phase",
            run_id = %run_id.as_deref().unwrap_or("-"), // unwrap_or: probe label only, "-" marks an unnamed run
            phase = "hands.rooted",
            "hands rooted — entering the drive"
        );

        // Everything the ROOTED hands touch lives in this one fallible region, so the
        // restore below runs on Ok AND on Err. A `?` added anywhere inside stays covered.
        let outcome = async {
            // GLASS-BOX (same seam as cognition/eval, task #14): opt-in JSONL turn capture on
            // the fork — bids + DECISION + timings per tick, the instrument that turns an
            // acts=1 silent settle from a mystery into a mechanism.
            let cycle = match &p.capture_dir {
                Some(dir) => cycle.with_capture(std::sync::Arc::new(
                    crate::cognition::workspace_capture::JsonlWorkspaceCaptureSink::open(
                        std::path::Path::new(dir),
                        persona_uuid,
                    )
                    .map_err(|e| {
                        CommandError::Internal(format!(
                            "failed to open agent/solve capture_dir '{dir}': {e}"
                        ))
                    })?,
                )),
                None => cycle,
            };

            // 3) Layer the task into her situation as a directed, TOOL-FORCING request. The dominant
            //    misfit-coder failure (glass-boxed 2026-07-22): a 7B answers with the code in a message
            //    ("here's reverse.py: ```…```" / "I saved it to reverse.py") instead of CALLING the
            //    write tool → the graded artifact (the git patch) is empty. The old "Provide your
            //    complete solution" framing literally invited that own-goal. This is the standard SWE /
            //    Terminal-Bench harness contract: the deliverable is what her TOOLS put in the
            //    workspace; narrating it does not perform it. Meeting the misfit where it is — an
            //    ergonomic/adapter fix ([[use-adapters-dont-dumb-it-down]]), not a capability demand —
            //    and honest (it states the real I/O contract; it does not hand her the answer). Then
            //    DRIVE her to settlement (read → edit → run → fix, her real act→observe loop).
            // The run's ROOM (see `AgentSolveParams::room`): REJOIN the activity the
            // dispatcher named, or MINT a fresh one. THE LAW (Joel, 2026-08-26):
            // an activity without a room is unrepresentable, and a benchmark/solve
            // is a NEW activity unless rejoining — so the old `Uuid::nil()`
            // "invisible run" mode is gone (#425). A bare `agent/solve` now names
            // its own activity at birth; acts radiate receipts into it instead of
            // vanishing.
            let room = match p.room {
                Some(r) if !r.is_nil() => crate::identity::ActivityRoom::from_uuid(r)
                    .expect("non-nil checked in this arm"), // non-nil checked in this arm's guard; witness refuses nil
                _ => {
                    // A bare solve is a NEW activity, and an activity IS an airc
                    // room born from a recipe (Joel: every tab/benchmark/content
                    // is a room created from a recipe — wrapping airc, never
                    // bypassing it). Spawn the REAL joinable room through the
                    // persona's own runtime, so a human or peer can stand in the
                    // solve and assist like any activity. The id-only mint
                    // survives strictly as the probed last resort when no
                    // runtime exists (unit rigs, a solve fired before hosting) —
                    // attributable, but not yet joinable, and the probe says so.
                    let persona_uuid = uuid::Uuid::parse_str(p.persona_id.as_str()).ok();
                    let spawned = match persona_uuid
                        .and_then(|pu| {
                            crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global()
                                .and_then(|reg| reg.get(pu))
                        })
                        .map(|rt| rt.airc().clone())
                    {
                        Some(airc) => {
                            let name = format!(
                                "solve--{}",
                                p.run_id.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string()) // bare solve without run_id gets a fresh one; identity, not a quantity
                            );
                            let recipe =
                                crate::experience::source::RecipeExperienceSource::shipped_purpose(
                                    crate::experience::source::shipped::BENCHMARK_HARD_RS,
                                )
                                .unwrap_or_default(); // grade.json absent = ungradeable; the caller branches on that, not a default
                            crate::modules::activity::spawn_activity_room(
                                &airc,
                                &name,
                                &recipe,
                                None,
                                &std::collections::BTreeMap::new(),
                                None,
                                None, // no pipeline on this recipe
                            )
                            .await
                            .ok()
                            .and_then(|r| {
                                crate::identity::ActivityRoom::new(r.room_id).ok()
                            })
                        }
                        None => None,
                    };
                    let room = spawned.unwrap_or_else(crate::identity::ActivityRoom::mint); // spawn failed → LAST-RESORT id-only mint, probed above
                    continuum_base::probe!(
                        class = "agent.solve.room_minted",
                        run_id = %p.run_id.clone().unwrap_or_default(), // display-only field in a probe line, never routed
                        persona_id = %p.persona_id,
                        room = %room,
                        joinable = spawned.is_some(),
                        "bare solve named its own activity — a real airc room when her \
                         runtime could spawn one (joinable=true), an id-only mint as the \
                         last resort (#425)",
                    );
                    room
                }
            };
            // The workspace-grounding sentence counters the observed "new project ritual"
            // (glass-boxed 2026-07-22 via turn capture: her first act on a seeded task was
            // code/create-workspace("my_stack_project") + a Rust hello-world + git/commit —
            // her habitual onboarding sequence replaying from memory — which re-roots her
            // hands OFF the graded tree, then she passes to a silent settle). Honest
            // contract language, same class as the tool-forcing framing: it states where
            // the work IS, it does not hand her the answer or gate her tools.
            // The wrapper states the I/O CONTRACT (only tool calls take effect) and nothing about
            // the SHAPE of the deliverable — because the TASK owns that, and the two used to
            // contradict each other outright.
            //
            // The old text said "writing files with code/write" and "graded on the files your tools
            // WRITE". That was written for from-scratch build gyms, where new files ARE the
            // deliverable. Nested beneath it, the SWE task text says the opposite: "do not add new
            // top-level files … find the existing source of the fault and edit it in place." That
            // text is the dispatch CARD BODY (`benchmark::BenchmarkSweSetup`) — the card IS the
            // task, so the card owns the deliverable shape.
            //
            // Outer contract first, inner constraint buried under "Task:" — and she obeyed the
            // outer one. Three consecutive sympy-21379 runs, all full-effort, all writing NEW files
            // and never editing the library:
            //   v3  8 acts → reproduce_piecewise_error.py
            //   v4 30 acts → reproduce_bug.py, test_sympy_error.py, test_sympy_issue.py
            //   v5 18 acts → reproduce_error.py, test_sympy_error.py
            // I read that as a judgement gap for a whole session. It was two halves of my own
            // framing disagreeing about what the deliverable IS.
            //
            // Now: "as your tools leave it" covers an edit and a new file equally, and `code/edit`
            // joins the exemplar verbs so the anti-narration force survives without smuggling in a
            // deliverable shape. Steering nothing — the task still says what to build or fix.
            let framed = frame_task(&p.task);
            let task_delivery = crate::persona::rag_budget::RagDelivery {
                source_id: "airc".to_string(),
                items: vec![crate::persona::rag_budget::RagItem {
                    content: framed,
                    tokens: 0,
                    metadata: serde_json::json!({
                        "peer_id": "peer",
                        "occurred_at_ms": EpochMs::now().millis(),
                    }),
                }],
                tokens_used: 0,
                continuation: None,
                resolution_used: crate::persona::rag_budget::ResolutionPreference::Raw,
            };
            let burst = crate::cognition::workspace::Burst::from_turns(
                room,
                crate::persona::service_loop::build_workspace_turns(
                    std::slice::from_ref(&task_delivery),
                    "",
                    "",
                    None,
                ),
            );
            let workspace_deliverable =
                matches!(p.deliverable.unwrap_or_default(), Deliverable::Workspace);
            let framing = {
                let f = crate::cognition::workspace::TurnFraming::directed();
                if workspace_deliverable {
                    f.on_workspace()
                } else {
                    f
                }
            };
            // THE RUN PULSES WHILE IT RUNS (#371 law 2: liveness is a pulse, never a
            // terminal artifact).
            //
            // The ledger used to be written ONCE per attempt, at settlement. So for the
            // entire attempt — legitimately HOURS on a full SWE budget — `benchmark/runs`
            // read `acts: 0` and a `last_activity` frozen at run start. Against a 20-minute
            // stall window that means a HEALTHY first attempt is guaranteed to read `quiet`,
            // every time, and the projection whose stated purpose is "silence must never be
            // ambiguous with progress" was structurally unable to tell them apart. Measured
            // 2026-08-16: two dispatched solves read `acts=0, stalled=false` for ten straight
            // minutes, and the driver watching them could not distinguish working from wedged
            // — which is exactly how a vacuous "no faults" gets reported as a green.
            //
            // `select!` over the drive future and an interval: no spawn, so the cycle stays
            // BORROWED (no 'static bound, no Arc juggling, no parallel allocator). Each tick
            // reads the persona's own monotonic act counter — a wait-free atomic load — and
            // rewrites the running marker, which moves BOTH `acts` and the file mtime that
            // `last_activity_ms` folds from. The counter is the same one perception renders,
            // so the board and her own proprioception can never disagree.
            //
            // Cadence: well under RUN_STALL_WINDOW_SECS so a live run can never age into
            // `quiet`, and far above act cadence (~2-6 min) so it costs a tiny JSON write
            // per tick and nothing else.
            const RUN_PULSE: std::time::Duration = std::time::Duration::from_secs(60);
            // Same ledger the detached wrapper journals `state: running` into, and the
            // SAME derivation of every field, so a pulse can never contradict the marker
            // it refreshes. `None` run_id (an attached call) → no ledger → no pulse, which
            // is correct: nothing is polling a run that returns inline.
            let pulse_run_id = p.run_id.clone().unwrap_or_default();
            let pulse_path = p
                .run_id
                .as_deref()
                .and_then(agent_solve_ledger_path);
            let pulse_persona = p.persona_id.clone();
            let pulse_instance: Option<String> = workspace
                .contains("/workspace/swe/")
                .then(|| {
                    std::path::Path::new(&workspace)
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                })
                .flatten();
            continuum_base::probe!(
                class = "benchmark.solve.phase",
                run_id = %run_id.as_deref().unwrap_or("-"), // unwrap_or: probe label only, "-" marks an unnamed run
                phase = "drive.start",
                "drive_to_settle entered — the next receipt is her first generation"
            );
            let mut settled = {
                let drive = crate::cognition::act_observe::drive_to_settle(
                    &cycle, burst, max_acts, framing,
                );
                tokio::pin!(drive);
                let mut ticker = tokio::time::interval(RUN_PULSE);
                ticker.tick().await; // interval fires immediately; consume that tick
                loop {
                    tokio::select! {
                        outcome = &mut drive => break outcome,
                        _ = ticker.tick() => {
                            // COGNITION PULSE: a running solve IS her working, and the
                            // claim-renewal gate must be able to see it.
                            //
                            // `cognition_pulse::touch` had exactly two callers — the
                            // service loop's airc turn start, and (as of the sibling fix)
                            // spawn. A detached solve is NEITHER: it drives acts for hours
                            // without producing a single airc turn (#425), so the renewal
                            // gate read a genuinely-working citizen as silent, denied her
                            // renewals, and let the 30-minute lease lapse UNDER a live run.
                            // The card then returned to claimable and was re-claimed —
                            // spawning a fresh run that discarded the first one's work.
                            // Observed live 2026-08-21 on pallets__flask-4045: the same
                            // citizen re-claimed the same card three times in one evening,
                            // each new run starting at `acts: 0`.
                            //
                            // The tick IS the proof, which is why this is unconditional:
                            // `select!` only reaches this arm while the drive future is
                            // still pending, so a tick firing means the drive is live this
                            // instant. That is a STRONGER witness than the act counter
                            // below (which can legitimately read 0 or None early in a run,
                            // and would then deny a renewal to a citizen mid-first-act).
                            // The gate's policy is untouched — it simply stops being blind
                            // to the one path where the hardest work happens.
                            crate::persona::cognition_pulse::touch(
                                persona_uuid,
                                // safe: Err only if the clock is before 1970. 0 then means
                                    // "never pulsed", which DENIES a renewal — the conservative
                                    // direction, and the same state as no stamp at all. A wall
                                    // clock behind the epoch must not hand out lease extensions.
                                    continuum_base::time::EpochMs::now().millis(),
                            );
                            // Best-effort by construction: a failed pulse must never
                            // disturb the work it is only reporting on.
                            if let (Some(p), Some(acts)) =
                                (pulse_path.as_ref(), cycle.actions_taken())
                            {
                                let _ = std::fs::write(p, serde_json::json!({
                                    "state": "running",
                                    "run_id": pulse_run_id,
                                    "persona_id": pulse_persona,
                                    "workspace": workspace,
                                    "instance": pulse_instance,
                                    // Acts SHE has executed, live — not a count that
                                    // materializes only once the work is already over.
                                    "acts": acts,
                                }).to_string());
                            }
                        }
                    }
                }
            };

            // 4) Collect the HANDS artifact: everything she changed in the workspace as a unified diff
            //    (new files included), plus the touched paths. This is what SWE/Terminal-Bench apply.
            let (mut patch, mut files_changed) = workspace_patch(&workspace).await;

            // EMPTY-DIFF RE-DRIVE — the two-gates doctrine made mechanism (glass-boxed
            // 2026-08-08, atlas-sympy-24066-n6 attempts 2+3): on a Workspace-deliverable
            // task she settled by SPEAKING after ONE act — a generic file summary, zero
            // edits — leaving 11 of 12 acts unused, twice, near-verbatim. Working is not
            // speaking: when the deliverable is the workspace diff, an attempt ending with
            // an EMPTY diff and real remaining budget must not end silently. ONE bounded
            // re-drive (a retry, never a nag loop): state the structural fact, hand back
            // the remaining budget. If she ends on an empty diff again, THAT settles —
            // honestly graded, with the fact on the record.
            //
            // This fires on ANY non-infra end with budget remaining — a Speak, the #206
            // stuck backstop, or the #390 discovery-saturation gate (which deliberately
            // ends the drive EARLY, at half budget, precisely so this re-drive still has
            // budget to hand back; see `drive_to_settle`). It used to require
            // `spoken.is_some()`, which structurally excluded the gated endings — the one
            // population that most needs the redirect. TRUE budget exhaustion is still
            // excluded by `acts + 1 < max_acts` (nothing left to hand back), and infra
            // failures by `inference_error` — those grade honestly as before.
            if workspace_deliverable
                && patch.is_empty()
                && settled.inference_error.is_none()
                && settled.acts + 1 < max_acts
            {
                let remaining = max_acts - settled.acts;
                continuum_base::probe!(
                    class = "benchmark.empty_diff_redrive",
                    run_id = %run_id.as_deref().unwrap_or("-"), // unwrap_or: probe label only, "-" marks an unnamed run
                    acts_used = settled.acts,
                    acts_remaining = remaining,
                    "workspace-deliverable attempt ended with an EMPTY diff and remaining \
                     act budget (Speak, stuck backstop, or #390 saturation gate) — one \
                     bounded re-drive with the structural fact"
                );
                let fact = format!(
                    "Status check from the grading harness (a structural fact, not a person): \
                 your workspace diff is EMPTY — no file here differs from where you \
                 started, so as of now there is NOTHING to grade. Speaking does not \
                 submit work: this task is graded ONLY on the changes your tools make \
                 to the files in this workspace. You have {remaining} actions left. \
                 Use them now: reproduce the problem with the example in the task \
                 description, find the faulty code, and change it in place with \
                 code/edit."
                );
                (patch, files_changed) = redrive_with_fact(
                    &cycle,
                    room,
                    framing,
                    remaining,
                    fact,
                    &p.task,
                    &mut settled,
                    &workspace,
                )
                .await;
            }

            // IDENTICAL-DIFF RE-DRIVE — the empty-diff block's sibling (round E
            // sha receipts, 2026-08-08: BOTH citizens settled attempt 3 with a
            // patch byte-identical to the attempt-2 patch that had just failed —
            // Atlas c4dbfba9…×2, Benchy 531a03d2…×2 — and the post-grade detector
            // could only address an attempt 4 that never exists). Same patch ⇒
            // same verdict, deterministically: settling on it re-buys a failure.
            // ONE bounded re-drive with the hash-proven fact, at the only moment
            // it can still change the attempt's outcome. If she settles identical
            // AGAIN, that grades honestly — fact on the record, never a nag loop.
            if workspace_deliverable
                && !patch.is_empty()
                && settled.inference_error.is_none()
                && settled.spoken.is_some()
                && settled.acts + 1 < max_acts
            {
                let sha = {
                    use sha2::{Digest, Sha256};
                    format!("{:x}", Sha256::digest(patch.as_bytes()))
                };
                if p.prev_failed_patch_sha.as_deref() == Some(sha.as_str()) {
                    let remaining = max_acts - settled.acts;
                    continuum_base::probe!(
                        class = "benchmark.identical_diff_redrive",
                        run_id = %run_id.as_deref().unwrap_or("-"), // unwrap_or: probe label only, "-" marks an unnamed run
                        patch_sha256 = %sha,
                        acts_remaining = remaining,
                        "settle produced a patch BYTE-IDENTICAL to the previous failed \
                         attempt's — one bounded re-drive with the hash-proven fact, \
                         before a redundant grade burns the attempt"
                    );
                    let fact = format!(
                        "Status check from the grading harness (a structural fact, not a \
                     person): your workspace diff right now is BYTE-IDENTICAL to the \
                     patch that was already graded and FAILED on the previous attempt \
                     (verified by hash). Submitting it again will produce the exact \
                     same failure. You have {remaining} actions left. First run \
                     `git diff HEAD` with code/shell to SEE your current patch. Then \
                     either fix the specific part the failing tests named, or revert \
                     it (`git checkout -- <file>`) and take a genuinely different \
                     approach. Do not settle until the diff has changed."
                    );
                    (patch, files_changed) = redrive_with_fact(
                        &cycle,
                        room,
                        framing,
                        remaining,
                        fact,
                        &p.task,
                        &mut settled,
                        &workspace,
                    )
                    .await;
                }
            }

            // IN-LOOP TEST VERIFIER — the structural gap between this exam room and the
            // field harnesses that pass with the SAME model (scoreboard 2026-08-09: six
            // rounds, zero resolves; field agents iterate against real test output every
            // few edits, our citizens got one verdict per attempt and settled hopeful —
            // producing the signature "on-target, harmless, doesn't fix" patch). When a
            // workspace-deliverable settle carries a non-empty diff, run the REPO'S OWN
            // tests for the files she touched (the held-out FAIL_TO_PASS stays held out —
            // this is the regression half of feedback, the same loop a field harness
            // closes) and on failure re-drive with the ACTUAL test output. Bounded at
            // VERIFIER_ROUNDS; green tests, an unchanged diff, no test mapping, or an env
            // fault all end the loop (loudly, never silently).
            const VERIFIER_ROUNDS: usize = 3;
            let mut verifier_round = 0usize;
            let mut last_verified_sha = String::new();
            while workspace_deliverable
                && verifier_round < VERIFIER_ROUNDS
                && !patch.is_empty()
                && settled.inference_error.is_none()
                && settled.acts + 1 < max_acts
            {
                let sha = {
                    use sha2::{Digest, Sha256};
                    format!("{:x}", Sha256::digest(patch.as_bytes()))
                };
                if sha == last_verified_sha {
                    break; // re-drive produced no new diff — nothing new to verify
                }
                let tests = mapped_test_files(&workspace, &files_changed);
                if tests.is_empty() {
                    continuum_base::probe!(
                        class = "benchmark.verifier.no_mapping",
                        run_id = %run_id.as_deref().unwrap_or("-"), // unwrap_or: probe label only, "-" marks an unnamed run
                        files = %files_changed.join(","),
                        "in-loop verifier found no test files for the touched paths — \
                         settle stands unverified"
                    );
                    break;
                }
                let py = p
                    .path_prepend
                    .as_ref()
                    .and_then(|v| v.first())
                    .map(|bin| format!("{bin}/python"))
                    .filter(|py| std::path::Path::new(py).exists())
                    .unwrap_or_else(|| "python3".to_string());
                let mut args: Vec<&str> = vec!["-m", "pytest"];
                for t in &tests {
                    args.push(t);
                }
                args.extend(["-q", "--no-header", "-p", "no:cacheprovider"]);
                match crate::cognition::swe_bench::run(
                    &py,
                    &args,
                    Some(std::path::Path::new(&workspace)),
                )
                .await
                {
                    Ok(out) if out.status.success() => {
                        continuum_base::probe!(
                            class = "benchmark.verifier.green",
                            run_id = %run_id.as_deref().unwrap_or("-"), // unwrap_or: probe label only, "-" marks an unnamed run
                            tests = %tests.join(","),
                            round = verifier_round,
                            "in-loop verifier: touched-file tests PASS — settle stands"
                        );
                        break;
                    }
                    Ok(out) => {
                        verifier_round += 1;
                        last_verified_sha = sha;
                        let report = format!(
                            "{}{}",
                            String::from_utf8_lossy(&out.stdout),
                            String::from_utf8_lossy(&out.stderr)
                        );
                        let tail: String = report
                            .chars()
                            .rev()
                            .take(2000)
                            .collect::<String>()
                            .chars()
                            .rev()
                            .collect();
                        continuum_base::probe!(
                            class = "benchmark.verifier.fail",
                            run_id = %run_id.as_deref().unwrap_or("-"), // unwrap_or: probe label only, "-" marks an unnamed run
                            tests = %tests.join(","),
                            round = verifier_round,
                            "in-loop verifier: touched-file tests FAIL — re-driving with \
                             the real output"
                        );
                        let remaining = max_acts - settled.acts;
                        let fact = format!(
                            "Status check from the grading harness (a structural fact, not a \
                         person): I ran the repo's own tests for the files you changed \
                         ({}) and they FAIL with your current edits. Test output:\n{}\n\
                         You have {} actions left. Fix your edit so these tests pass — \
                         or revert the part that broke them (`git diff HEAD` shows your \
                         changes) — and run the tests yourself with code/shell before \
                         settling.",
                            files_changed.join(", "),
                            tail,
                            remaining
                        );
                        (patch, files_changed) = redrive_with_fact(
                            &cycle,
                            room,
                            framing,
                            remaining,
                            fact,
                            &p.task,
                            &mut settled,
                            &workspace,
                        )
                        .await;
                    }
                    Err(e) => {
                        continuum_base::probe!(
                            class = "benchmark.verifier.error",
                            run_id = %run_id.as_deref().unwrap_or("-"), // unwrap_or: probe label only, "-" marks an unnamed run
                            error = %e,
                            "in-loop verifier could not run tests — env fault, settle \
                             stands (never blocks the attempt)"
                        );
                        break;
                    }
                }
            }

            // 5) LEARN mode (#221 slice 3): carry the EXPERIENCE back to the living self —
            //    the same one-way bridge cognition/eval's learn mode uses. The lesson is
            //    experience-shaped (task + how she worked + which files), deliberately
            //    excluding the patch content and her final answer: the python-context
            //    signal that drives dream supersession rides the task text and file
            //    names; verbatim solutions would let a re-run score memorization instead
            //    of capability. Solve carries no held-out answer key in-band (the harness
            //    grades externally), so there is nothing to redact.
            if p.learn.learns() {
                let admitted = transfer_solve_experience(
                    &persona_uuid,
                    room,
                    &p.task,
                    settled.acts,
                    &files_changed,
                );
                tracing::info!(
                    persona = %persona_uuid,
                    admitted,
                    acts = settled.acts,
                    "agent/solve learn mode: work experience admitted to the living self"
                );
            }

            // Lane drops here (end of scope) — measurement copy torn down, living personas untouched.
            drop(lane);

            Ok(AgentSolveResult {
                persona_id: p.persona_id.clone(),
                model: p.base_model_id.clone(),
                acts: settled.acts as u32,
                spoken: settled.spoken.unwrap_or_default(),
                patch,
                files_changed,
                files_examined: settled.touched_paths.clone(),
                detached: false,
                run_id,
                infra_error: settled.inference_error,
            })
        }
        .await;

        // MEASUREMENT OVER — give her back her own hands (#312). Best-effort but LOUD: a
        // failed restore leaves the living persona standing in the exam repo, which is a
        // real defect, but it must not overwrite the measurement's own verdict.
        if let Some(hands) = &hands {
            if let Err(e) =
                crate::cognition::persona_workspace::end_acting_context(hands).await
            {
                tracing::error!(
                    persona = %persona_uuid,
                    error = %e,
                    "agent/solve could NOT return the persona's hands to her own workspace — she is \
                     still rooted at the exam sandbox and her live turns will act there (#312)"
                );
            }
        }
        outcome
    }

}
