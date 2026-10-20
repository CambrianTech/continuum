use super::*;

#[async_trait]
impl ActionCommand for BenchmarkDispatch {
    const NAME: &'static str = "benchmark/dispatch";
    // Operator/curator surface, like work/create: seeding the board is
    // curation, not a citizen verb.
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Post a benchmark's tasks onto the work board as claimable cards (one card per task, \
         title marked `[bench <name>]`). Citizens claim and solve them through the normal \
         kanban loop with their own hands; scoring reads the artifacts they produce. \
         `limit` caps how many tasks are posted; `repo` defaults to the board's existing repo.";
    type Params = BenchmarkDispatchParams;
    type Output = BenchmarkDispatchResult;

    async fn run(
        &self,
        ctx: &Ctx,
        p: BenchmarkDispatchParams,
    ) -> Result<BenchmarkDispatchResult, CommandError> {
        use crate::modules::work::persona_airc;
        use airc_lib::{CreateWorkCard, Priority, RepoId};

        // ── RECIPE PATH: the whole experiment by name ──────────────────────
        // `--recipe X` loads a `benchmark_recipes` row (data, not code), pins
        // the model it names (fit-gated by serving/pin), awaits lane readiness,
        // then fires every dispatch entry through THIS SAME verb. The design
        // from the start: two primitives, commands composing commands.
        if let Some(recipe_name) = p.recipe.clone() {
            if p.name.is_some() || p.instances.is_some() {
                return Err(CommandError::Invalid(
                    "pass either --recipe OR --name/--instances — the recipe row carries                      its own dispatches"
                        .into(),
                ));
            }
            let recipe = self
                .load_recipe(&recipe_name, &p.params.clone().unwrap_or_default()) // unwrap_or: absent params = empty map, placeholders then fail loud by name
                .await?;
            self.ensure_recipe_model(&recipe).await?;
            let mut agg: Option<BenchmarkDispatchResult> = None;
            for d in &recipe.dispatches {
                // ROLE → NAMES, at dispatch (recipe = content-type + RULES, #371):
                // `reviewers: N` resolves against the LIVE resident roster here, so
                // the recipe ships without citizen names. Merged with any explicit
                // `teammates`; a reviewer who lands as a card's assignee is skipped
                // per-card downstream (never their own reviewer). Short rosters
                // degrade loudly, never silently.
                let mut team: Vec<String> = d.teammates.clone();
                if let Some(n) = d.reviewers.filter(|n| *n > 0) {
                    let picked: Vec<String> = match crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global() {
                        Some(reg) => reg
                            .resident_snapshot()
                            .await
                            .into_iter()
                            .map(|(name, _)| name)
                            .filter(|name| !team.contains(name))
                            .take(n as usize)
                            .collect(),
                        None => Vec::new(),
                    };
                    if (picked.len() as u32) < n {
                        crate::probe!(
                            class = "recipe.reviewers.short",
                            benchmark = %d.benchmark,
                            wanted = n as u64,
                            resolved = picked.len() as u64,
                            "recipe asked for more reviewers than the roster holds — team degrades, loudly"
                        );
                    }
                    team.extend(picked);
                }
                let sub = BenchmarkDispatchParams {
                    name: Some(d.benchmark.clone()),
                    recipe: None,
                    doctrine: d.doctrine.clone().or_else(|| p.doctrine.clone()),
                    review_gate: d.review_gate.or(p.review_gate),
                    teammates: if team.is_empty() {
                        p.teammates.clone()
                    } else {
                        Some(team)
                    },
                    instances: if d.instances.is_empty() {
                        None
                    } else {
                        Some(d.instances.clone())
                    },
                    limit: d.limit.or(p.limit),
                    ..p.clone()
                };
                let r = Box::pin(self.run(ctx, sub)).await?;
                agg = Some(match agg.take() {
                    None => r,
                    Some(mut a) => {
                        a.dispatched += r.dispatched;
                        a.card_ids.extend(r.card_ids);
                        a.skipped_needs_setup += r.skipped_needs_setup;
                        a.skipped_known_red += r.skipped_known_red;
                        a.skipped_already_on_board += r.skipped_already_on_board;
                        a.skipped_already_resolved += r.skipped_already_resolved;
                        a.pruned_duplicates += r.pruned_duplicates;
                        a.contended_tasks += r.contended_tasks;
                        a.kickoffs += r.kickoffs;
                        a.solves_fired += r.solves_fired;
                        a.kickoff_errors.extend(r.kickoff_errors);
                        a.benchmark = format!("recipe:{recipe_name}");
                        a
                    }
                });
            }
            return agg.ok_or_else(|| {
                CommandError::Invalid(format!(
                    "recipe '{recipe_name}' has no dispatches — author at least one"
                ))
            });
        }
        let name = p.name.clone().ok_or_else(|| {
            CommandError::Invalid(
                "pass --name <benchmark> (see benchmark/list) or --recipe <name>".into(),
            )
        })?;

        let spec = known_benchmarks()
            .iter()
            .find(|b| b.name == name)
            .ok_or_else(|| {
                CommandError::Invalid(format!(
                    "unknown benchmark '{}' — see benchmark/list",
                    name
                ))
            })?;
        // Two shapes of dispatchable benchmark, ONE card loop below. A `PreparedCard`
        // normalizes both so create + kickoff is source-agnostic:
        //  • gym collections resolve a committed JSONL of `EvalTask`s (write→compile→test);
        //  • SWE-bench-INSTANCE collections pull real GitHub-issue PROJECTS via the proven
        //    `swe_bench` loader — the real-project tier "the frontier models fight over"
        //    (Joel, 2026-08-10). Each instance becomes a clone-and-fix card.
        let prepared = prepare_cards(
            spec,
            &CardSelection {
                instances: p.instances.clone(),
                sample: p.sample,
                seed: p.seed,
                language: None,
            },
        )
        .await?;
        prewarm_swe_envs(&prepared);

        let requested = p.assignees.clone().unwrap_or_default();  // unwrap_or: unreadable = empty, the report shows the tracker's view
        if requested.iter().any(|a| a.trim().is_empty()) {
            return Err(CommandError::Invalid(
                "assignees contains an empty name — every kickoff must address a real citizen"
                    .to_string(),
            ));
        }
        // #442 (roster half) + #412 + #455: a dispatch fired inside the post-boot resume
        // window used to find an EMPTY roster and refuse instantly — so the operator
        // hand-rolled a sleep-loop around dispatch (run by hand twice on 2026-08-17; a
        // runbook line is a design defect). The serving half of #442 already parks
        // (`await_ready_serving` below); the roster half parks the same way, bounded.
        //
        // #455 is what this loop keys on NOW: RESIDENCY, not registration. Waiting for the
        // roster to be non-empty released the wait ~10 minutes too early (#412) — citizens
        // are registered, presence-pumping and renewing claims long before the supervisor
        // attaches a service loop, so the old condition cleared while nobody could hear a
        // thing. Measured 2026-08-18: roster listed 2, `subscribe_opened` was 0, a whole
        // round went onto the board and produced zero turns.
        //
        // An unknown NAME against a RESIDENT roster still fails fast — that error means a
        // typo, never a resume in progress.
        const ROSTER_RESUME_WAIT: std::time::Duration = std::time::Duration::from_secs(180);
        const ROSTER_RESUME_POLL: std::time::Duration = std::time::Duration::from_secs(5);
        let wait_started = std::time::Instant::now();
        let mut resident = self.registry.resident_snapshot().await;
        while resident.is_empty() && wait_started.elapsed() < ROSTER_RESUME_WAIT {
            tracing::info!(
                waited_s = wait_started.elapsed().as_secs(),
                registered = self.registry.roster_snapshot().len(),
                probe_class = "benchmark.dispatch.awaiting_residency",
                "dispatch: no RESIDENT citizen yet (registered != in the room, #412/#455) \
                 — waiting for a service loop rather than staging into an empty room"
            );
            tokio::time::sleep(ROSTER_RESUME_POLL).await;
            resident = self.registry.resident_snapshot().await;
        }

        // Curator seed — resolved AFTER the residency park on purpose. It authors
        // through a live citizen when the operator has no self-peer (#27), and it
        // used to run BEFORE the park, so a dispatch fired inside the post-boot
        // window refused instantly with "none are online" while the 180s wait that
        // exists precisely for that window sat unreachable 30 lines below
        // (measured live 2026-08-26). Order: wait for a citizen, then author.
        let airc = persona_airc(&self.registry, ctx, "benchmark/dispatch")?;

        // Resolve the dispatch roster against THIS machine's live citizens (never our
        // names): empty request → the whole live roster; explicit names → validated or
        // fail-loud. This is the generalization for all repo users — dispatch targets the
        // citizens they actually spawned, whoever those are.
        //
        // Resolved BEFORE the room exists because the roster decides WHO gets moved into
        // it: a run room nobody is standing in is the other half of the bug this verb is
        // fixing ("old rooms flooded, or ones with nothing").
        let roster =
            resolve_dispatch_roster(&resident, &self.registry.roster_snapshot(), &requested)?;
        // TEAM roster (#team-proof gap 1): resolved against the SAME resident
        // snapshot, but LENIENTLY — a non-resident teammate shrinks the team
        // (probed), it never refuses the dispatch the way a bad assignee does:
        // the claimer's solve is the accountable unit, the team is the amplifier.
        let teammates: Vec<crate::identity::PeerId> = p
            .teammates
            .clone()
            .unwrap_or_default() // unwrap_or: no teammates named = solo dispatch, the default
            .iter()
            .filter_map(|name| {
                let hit = resident
                    .iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case(name))
                    .map(|(_, id)| crate::identity::PeerId::from_uuid(*id));
                if hit.is_none() {
                    crate::probe!(
                        class = "benchmark.team.mate_not_resident",
                        mate = %name,
                        "named teammate not resident — team shrinks, dispatch proceeds"
                    );
                }
                hit
            })
            .collect();

        // Repo hint, read from the board the curator is standing in RIGHT NOW — before we
        // move her, and ONLY when the caller named no repo. The run room is fresh, so its
        // board is empty and cannot answer "what repo key do cards use here"; the room she
        // came from can. Keeps `repo` optional exactly as before, and is the ONLY thing the
        // old room still contributes to a run.
        let repo_hint = match &p.repo {
            Some(_) => None,
            None => airc
                .work_board_complete(airc_lib::WORK_BOARD_PROJECTION_PAGE_SIZE)
                .await
                .ok()
                .and_then(|b| {
                    b.snapshot()
                        .cards
                        .first()
                        .map(|c| c.repo.as_str().to_string())
                }),
        };

        // ── THE RUN'S OWN ROOM ────────────────────────────────────────────────────
        // A benchmark run is an ACTIVITY, and an activity is a ROOM. Spawned through the
        // same `activity/spawn` path a citizen uses, so the room carries its recipe binding
        // and projects as a benchmark rather than a plain chat — a hand-made room would
        // carry neither ([[benchmarks-must-be-positronic-activities-not-a-parallel-subsystem]]).
        //
        // The `join` inside also MOVES the curator's current-room pointer, which is exactly
        // what makes the rest of this function land in the run room: the board read, every
        // `create_work_card`, and every kickoff `say` are all current-room operations. That
        // pointer move is a documented gap for other callers (activity.rs) and the mechanism
        // for this one.
        // WE WORK ACTIVITIES. A dispatch ROOTS its own benchmark activity — always.
        //
        // This used to accept a room NAME, and that one optional string was enough to
        // throw away the entire outcome: `--room continuum` put the cards on a plain
        // CHAT room's board, where they carry no benchmark binding, so `review_gate`
        // ("only the reviewer's done closes the card and fires its grade") has nothing
        // to hang off and nothing ever grades them. Measured 2026-09-09: three correct,
        // compiling citizen solutions produced exactly that way, `benchmark/runs` = 0,
        // scoreboard `attempted=0 resolved=0`, and the round reported `settled=1` beside
        // `graded_at_ms=None` without complaint.
        //
        // The knob is gone rather than validated. A verb that CAN post benchmark cards
        // onto a non-benchmark board will eventually be used to do it — it is locally
        // the shortest path to "a number" every single time, which is the parallel
        // runner `BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md` exists to forbid. Removing
        // the option makes the wrong thing unreachable instead of merely discouraged.
        //
        // To add work to a run that already exists, dispatch again: the round tracker
        // converges duplicates (`skipped_already_on_board`, `pruned_duplicates`) rather
        // than needing a caller to aim at a board by hand.
        let room_name = default_run_room_name(spec.name, crate::time::EpochMs::now().secs());
        // The room binds to the SHIPPED benchmark recipe's declared purpose,
        // resolved from its constant — never a re-typed string. The old literal
        // here was "benchmark" while the recipe declares "benchmark/hard-rs",
        // so every run room resolved to no manifest and rendered as plain chat
        // (#431 — the scoreboard region was the whole point of the recipe).
        let bench_recipe = crate::experience::source::RecipeExperienceSource::shipped_purpose(
            crate::experience::source::shipped::BENCHMARK_HARD_RS,
        )
        .ok_or_else(|| {
            CommandError::Internal(
                "shipped benchmark recipe missing from the embedded set — \
                 build-time authoring bug"
                    .into(),
            )
        })?;
        // The run's REAL targeting rides the binding (#433): suite is the
        // resolved spec's name, instances the caller's explicit selection,
        // team the resolved roster. Anything not set here (budget) stays at
        // the recipe's declared default — the binding is the room's honest
        // self-description, readable through the same pipe as everything else.
        let mut run_params = std::collections::BTreeMap::new();
        run_params.insert("suite".to_string(), serde_json::json!(spec.name));
        if let Some(instances) = &p.instances {
            run_params.insert("instances".to_string(), serde_json::json!(instances));
        }
        run_params.insert(
            "team".to_string(),
            serde_json::json!(roster.iter().map(|(who, _)| who).collect::<Vec<_>>()),
        );
        run_params.insert("driver".to_string(), serde_json::json!(p.drive.unwrap_or_default()));  // unwrap_or: no driver named = the recipe default (citizen)
        if let Some(doctrine) = &p.doctrine {
            run_params.insert("doctrine".to_string(), serde_json::json!(doctrine));
        }
        run_params.insert(
            "review_gate".to_string(),
            serde_json::json!(p.review_gate.unwrap_or(false)),  // unwrap_or: gate not named = off, the control arm
        );
        // WHERE THE ROUND ROOTS is the spawn seam's decision, for every recipe: the
        // benchmark recipe declares `base: academy` (a round is learning, by what it is),
        // so no parent is named here.
        let room = crate::modules::activity::spawn_activity_room(
            &airc,
            &room_name,
            &bench_recipe,
            None,
            &run_params,
            self.executor().ok(),
            None, // hard-rs recipes declare no pipeline; nothing acts here
        )
        .await?;
        // The round's standing rules, published as the run room's operating doctrine
        // RIGHT HERE while the curator's current room is still the freshly spawned run
        // (spawn_activity_room leaves the pointer there). Rendered verbatim into every
        // resident's grounding as `[Room operating doctrine]` — the reminder that
        // survives the window, unlike a kickoff message.
        if let Some(doctrine) = &p.doctrine {
            airc.publish_room_doctrine(
                doctrine.clone(),
                format!("round-{}", room.room_id.as_uuid().simple()),
            )
            .await
            .map_err(|e| CommandError::Internal(format!("doctrine publish: {e}")))?;
            crate::probe!(
                class = "bench.round.doctrine_published",
                room = %room.room_id.as_uuid(),
                chars = doctrine.len() as u64,
                "the round's standing rules are on the run room's doctrine — every \
                 resident grounds on them each turn"
            );
        }

        // Move every assignee INTO the run — a citizen who is not subscribed never sees the
        // board, the kickoff, or the peers working beside her. This is the members[] half
        // of #274, done for the one activity that needs it most.
        let mut room_join_errors: Vec<String> = Vec::new();
        for (who, peer) in &roster {
            match self.registry.get(*peer) {
                Some(rt) => {
                    // join_room, not airc().join: it bumps the membership epoch so
                    // her LIVE perception stream re-opens with the new room in its
                    // channel snapshot. A bare join grants durable membership to a
                    // room she structurally cannot hear (P0 20b44763 — three rounds
                    // of kickoffs into deaf run rooms, zero turns).
                    if let Err(e) = rt.join_room(&room_name).await {
                        room_join_errors.push(format!("{who}: {e}"));
                    }
                }
                None => room_join_errors.push(format!("{who}: no live airc runtime")),
            }
        }

        // The RUN ROOM's board — fresh, so the idempotence gate and prune below reason
        // about THIS run and nothing else. That is the point: a run's board is finally its
        // own honest denominator instead of a shared pile 136 cards deep.
        let board = airc
            .work_board_complete(airc_lib::WORK_BOARD_PROJECTION_PAGE_SIZE)
            .await
            .map_err(|e| CommandError::Internal(format!("board read: {e}")))?
            .snapshot();

        // Tasks that ALREADY have a live card. A card in a terminal state (Closed /
        // Merged) is finished work and must NOT block a re-dispatch — that is how a
        // benchmark gets legitimately re-run. Everything else (Open, Claimed,
        // InProgress, Blocked, Review) is live work; posting a second card for it
        // just splits effort across duplicates.
        // Grouped by task key, because the same map answers BOTH questions: "does
        // this task already have a card?" (the idempotence gate) and "which of its
        // cards are redundant?" (the optional prune below).
        let mut live_by_task: std::collections::HashMap<&str, Vec<&airc_lib::WorkCard>> =
            std::collections::HashMap::new();
        for c in board.cards.iter().filter(|c| {
            !matches!(
                c.state,
                airc_lib::CardState::Closed | airc_lib::CardState::Merged
            )
        }) {
            if let Some(k) = bench_card_key(&c.title) {
                live_by_task.entry(k).or_default().push(c);
            }
        }

        // Repo: caller-supplied, else the repo the board already uses. No
        // baked-in default — an empty board with no repo argument is a real
        // question only the operator can answer.
        // Repo key, most-explicit-first:
        //   1. what the caller named;
        //   2. what the room they came from already uses (unchanged legacy behaviour —
        //      matches existing cards so a re-dispatch never splits a live board);
        //   3. THE CHECKOUT'S OWN `origin` — the repo key is a fact about this clone,
        //      not a string to retype.
        //
        // (3) exists because per-run rooms removed (2)'s source by construction: the
        // FIRST dispatch leaves the curator standing in a fresh empty bench room, so the
        // SECOND one had nothing to infer from and failed asking for `--repo`. Deriving
        // it from `origin` fixes that at the root and is right for any repo user on a
        // fresh clone, the same way `resolve_dispatch_roster` refuses to bake in names.
        let repo_key = match p.repo.clone() {
            Some(r) => r,
            None => match repo_hint {
                Some(hint) => Some(hint),
                // Our recorded checkout's origin, never the process cwd: a launchd core
                // runs in its data dir (card 2a2cd6f1). No recorded checkout returns None
                // and the caller is asked for `repo`, never a wrong board key.
                None => match crate::modules::repo_registry::own_checkout().await {
                    Ok(checkout) => crate::code::git_bridge::origin_repo_slug(&checkout).await,
                    Err(_) => None,
                },
            }
            .ok_or_else(|| {
                    CommandError::Invalid(
                        "no `repo` was given, the room you dispatched from has no cards to \
                         infer one from, and this checkout has no `origin` remote to derive \
                         one from — pass repo=<owner/name> so the cards land under a real \
                         board key"
                            .to_string(),
                    )
                })?,
        };
        let repo = RepoId::new(repo_key)
            .map_err(|e| CommandError::Invalid(format!("invalid repo: {e:?}")))?;

        let take = p.limit.map(|l| l as usize).unwrap_or(prepared.len());
        // Every task key THIS benchmark owns — captured before the loop consumes
        // `prepared`, and deliberately NOT limited by `take`: a prune must be able
        // to clean duplicates for tasks outside the current dispatch window, which
        // is what makes `limit=0 prune=true` a pure board-cleanup call.
        let planned_keys: Vec<String> = prepared
            .iter()
            .filter_map(|pc| bench_card_key(&pc.title).map(str::to_string))
            .collect();
        // Continuum home under which each citizen's workspace lives (for SWE staging).
        let stage_home = crate::paths::continuum_home().ok();
        let mut card_ids = Vec::new();
        // FULL card uuids for the round tracker (#371) — the bus event carries the full
        // hyphenated uuid, so the round's membership set must too (the 8-char `card_ids`
        // shorts are the human/CLI handle, not the identity).
        let mut card_uuids: Vec<uuid::Uuid> = Vec::new();
        let skipped_needs_setup = 0u32; // setup runs at the claim edge now (card_staging)
        let mut skipped_known_red = 0u32;
        let mut skipped_already_on_board = 0u32;
        let mut kickoffs = 0u32;
        let mut solves_fired = 0u32;
        let mut kickoff_errors = Vec::new();
        // Auto-fire is capped at the live serving LANE count. A directed dispatch must never
        // fire more concurrent scored solves than the box can hold (glass-boxed 2026-08-11:
        // over-firing solves onto a 2-lane box thrashed the llama-server lane to a
        // Connection-refused death mid-solve).
        //
        // WAIT for the boot-gate, don't guard against it (Joel 2026-08-11: "persona should boot
        // beforehand … dispatch requires live is dumb"). Personas boot event-driven the moment
        // the serving lane proves it can decode (ipc/mod.rs spawn_all), so a dispatch that lands
        // in that ~10-15s window must PARK on serving readiness, not skip every solve and post
        // silent cards. `await_ready_serving` returns as soon as the lane is decode-verified, or
        // None after the deadline (a genuinely dead lane — then we stage + kick off but don't
        // launch a solve into a corpse, self-healing on the next dispatch). NOTE: this caps THIS
        // dispatch call; the global in-flight-solve admission gate shared with work/claim
        // (#385/#386) is the broader fix.
        //
        // #442, and the correction that makes it a GATE: the probe below was already here and
        // already correct — its answer was simply advisory. On a dead lane this set the cap to
        // zero and STAGED THE CARDS ANYWAY, posting a full round of work to a board with
        // nothing on the box able to decode a token (#455: "we stage work into the gap"). A
        // not-ready lane is now a STATE the round stops at, not a parameter that silently
        // degrades it into an empty round.
        //
        // The wait budget is DERIVED, never invented: `DEFAULT_SERVING_WAIT` is
        // `READY_TIMEOUT + margin` — the spawner's own load budget — so this gate can never
        // declare failure before a legitimate cold load has had its full window. The flat 30s
        // that used to be here was exactly that bug in miniature.
        let awaited = crate::inference::llama_server::await_ready_serving(
            crate::inference::llama_server::DEFAULT_SERVING_WAIT,
        )
        .await;
        let solve_cap: u32 = {
            use crate::cognition::round_readiness::{decide, RoundReadiness};
            let current = crate::inference::llama_server::current_serving();
            match decide(awaited.as_ref(), Some(&current)) {
                RoundReadiness::Ready { lanes } => lanes,
                RoundReadiness::Blocked(reason) => {
                    let why = reason.explain();
                    crate::probe!(
                        class = "bench.round.staging_blocked",
                        benchmark = spec.name,
                        forced = p.force.unwrap_or(false),
                        reason = why.as_str(),
                        "STAGING → READY refused: serving cannot work this round (#442)",
                    );
                    // Refuse, with the override announcing itself — same contract as
                    // `start --force` (#420). A gate with no escape gets worked around;
                    // a silent escape is worse than no gate.
                    if !p.force.unwrap_or(false) {
                        return Err(CommandError::Invalid(format!(
                            "benchmark/dispatch refused to stage `{}`: {why}\n\
                             (pass --force to stage anyway — it will post cards that cannot be \
                             worked until a lane comes up)",
                            spec.name
                        )));
                    }
                    tracing::warn!(
                        benchmark = %spec.name,
                        reason = %why,
                        "--force: staging a round into a lane that is NOT decode-verified"
                    );
                    0
                }
            }
        };
        // OPEN the round BEFORE the first card exists. Kickoffs go out inside the loop, so
        // a citizen can claim card 1 while card 2 is still being posted — and `work/claim`
        // asks the round who drives. Registering after the loop (as this did) left that
        // window answering with the default, which would silently fire the detached solver
        // on the first card of a citizen-driven round.
        let driver = p.drive.unwrap_or_default();  // unwrap_or: unreadable = empty, the report shows the tracker's view
        crate::cognition::bench_round::open_round(room.room_id.as_uuid(), spec.name, driver);
        crate::cognition::bench_round::set_run_room_name(room.room_id.as_uuid(), &room_name);
        if p.review_gate.unwrap_or(false) {  // unwrap_or: gate not named = off, the control arm
            crate::cognition::bench_round::set_review_gate(room.room_id.as_uuid(), true);
        }
        // The round REMEMBERS its team: driver edges (settle-advance, non-settling
        // advance, boot resume) dispatch cards that were never initially fired, and
        // without a round-level record they went out team-less (2026-08-30).
        if !teammates.is_empty() {
            crate::cognition::bench_round::set_round_team(
                room.room_id.as_uuid(),
                teammates.iter().map(|t| t.as_uuid()).collect(),
            );
        }
        let mut skipped_already_resolved = 0u32;
        for pc in prepared.into_iter().take(take) {
            // A gym setup_shell card is prepared in the CLAIMER's workspace at claim
            // time (`card_staging`, 2026-09-03) — the early unconditional skip that
            // used to live here reported the entire ds-1000 maiden dispatch as
            // `skipped_needs_setup: 4` (2026-08-22): the eval path always ran
            // setup_shell, but no card-dispatch orchestration did.

            // IDEMPOTENCE: this exact task already has a live card. Re-dispatching
            // would post a duplicate, and duplicates are not free — two citizens
            // claiming two cards for one instance burn two of a 2-4 lane box on the
            // same problem, and the resulting board has no honest denominator to
            // compute a pass rate from. The key comes from `pc.title` itself, so the
            // string we match on is the string that would have been posted.
            //
            // Skipping is REPORTED (`skipped_already_on_board`), never silent — same
            // contract as `skipped_needs_setup` above: a partial dispatch that reads
            // as full coverage is the lie these counters exist to prevent.
            if bench_card_key(&pc.title).is_some_and(|k| live_by_task.contains_key(k)) {
                skipped_already_on_board += 1;
                continue;
            }

            // A CITIZEN ALREADY SOLVED THIS ONE. The seeded sample is the replication
            // contract, so the instance stays IN the round and in its score — but it is
            // not offered as work again. Re-offering cost us twice on 2026-09-07:
            // astropy-13236 (resolved 08-26) was re-staged onto its August checkout, and
            // sympy-22456's second attempt overwrote the first citizen's outcome. Neither
            // taught anyone anything; both burned lanes we do not have.
            if let CardWork::Swe { instance } = &pc.work {
                if crate::cognition::swe_bench::read_verdict(&instance.instance_id)
                    .is_some_and(|v| v.resolved)
                {
                    skipped_already_resolved += 1;
                    crate::probe!(
                        class = "bench.round.already_resolved",
                        instance = %instance.instance_id,
                        "a citizen's card already resolved this instance — it stays in the round's sample and its score, and is not offered as work again"
                    );
                    continue;
                }
            }

            // The directed assignee (round-robin over the RESOLVED live roster). Always a
            // real online citizen — resolve_dispatch_roster guaranteed a non-empty roster
            // or errored. Her peer_id rides along, so SWE staging needs no second name
            // lookup (and can never silently no-stage on an unknown name). Under a
            // DETACHED-SOLVE round she is staged and pre-claimed below and her solve
            // fires (#346); under a CITIZEN round she is only the kickoff's voice
            // exclusion — the card posts OPEN and whoever pulls it is staged at claim.
            let (who, who_peer) = &roster[card_ids.len() % roster.len()];

            // STAGE the SWE checkout into the assignee's workspace/swe/<instance> BEFORE the
            // card is claimable, so `work/claim` finds it and launches the solve. Reuses the
            // proven swe_bench::clone_at (fast from the local mirror). Best-effort: a stage
            // failure is REPORTED and the card still posts — the loop never half-breaks.
            // THE COVERAGE GATE (the cheap half of `benchmark/validate`). If THIS
            // box has already PROVEN this instance's (repo, era) env class red,
            // do not spend a citizen's hours and a grader's slot discovering the
            // same wall a third time — name it and skip. Measured 2026-08-28:
            // astropy-6938 was dispatched into the numpy-2 wall while a validate
            // run minutes away already knew astropy/2017 was red.
            //
            // A DICTIONARY LOOKUP, never a build: dispatch must stay fast for a
            // repo user who has never run validate. Fail-open by construction —
            // no map, a green class, or a map earned on a different machine class
            // all return None and DISPATCH (see known_red_wall). Reported like
            // every other skip; a partial dispatch reading as full coverage is
            // the lie these counters exist to prevent.
            if let CardWork::Swe { instance } = &pc.work {
                if let Some(wall) =
                    known_red_wall(spec.name, &instance.repo, instance.year())
                {
                    skipped_known_red += 1;
                    kickoff_errors.push(format!(
                        "skipped {} — its env class ({} {}) is PROVEN RED on this box by \
                         benchmark/validate, so a solve here would burn hours on a known \
                         wall: {wall}",
                        instance.instance_id,
                        instance.repo,
                        instance.year()
                    ));
                    continue;
                }
            }

            // STAGING MOVED TO THE CLAIM EDGE (2026-09-03, the pull inversion). A
            // citizen-driven round stages NOTHING here: whoever PULLS the card is
            // staged for it by `work/claim` → `card_staging::stage_for_claimer`, so any
            // resident can work any Open card and a rebooted citizen who re-reads the
            // board is staged exactly like a first claimer. Only a DETACHED-SOLVE round
            // still stages its directed assignee here — the SAME function — because
            // the solve below fires without a claim from her.
            let staged_ok = if driver == crate::cognition::bench_round::WorkDriver::DetachedSolve
            {
                match stage_home.as_ref() {
                    Some(home) => match crate::modules::card_staging::stage_for_claimer(
                        home,
                        *who_peer,
                        &pc.title,
                    )
                    .await
                    {
                        crate::modules::card_staging::Staging::Ready { .. }
                        | crate::modules::card_staging::Staging::Ordinary => true,
                        crate::modules::card_staging::Staging::Failed { stage, error } => {
                            kickoff_errors.push(format!("stage {}: {stage}: {error}", pc.title));
                            false
                        }
                    },
                    None => false,
                }
            } else {
                false
            };
            let mut req = CreateWorkCard::new(repo.clone(), pc.title, Priority::P2);
            req.body = Some(pc.body);
            let card_id = airc
                .create_work_card(req)
                .await
                .map_err(|e| CommandError::Internal(e.to_string()))?;
            let full = card_id.as_uuid().simple().to_string();
            let short = full[..8].to_string();

            // The card joins the round the moment it exists — BEFORE the pre-claim and
            // the kickoff below, either of which can put it into someone's hands. From
            // here `work/claim` can read who drives it (see `open_round`).
            crate::cognition::bench_round::add_card(room.room_id.as_uuid(), card_id.as_uuid());
            // WHO works this card, recorded at staging (before any solve fires) —
            // the follow-on driver and the boot resume read it (plan A4/A5).
            if driver == crate::cognition::bench_round::WorkDriver::DetachedSolve {
                crate::cognition::bench_round::record_card_assignee(card_id.as_uuid(), *who_peer);
            }
            // WHAT it tests, same moment — the roll-call names the instance
            // and in-flight runs join back to their card by it.
            if let CardWork::Swe { instance } = &pc.work {
                crate::cognition::bench_round::record_card_instance(
                    card_id.as_uuid(),
                    &instance.instance_id,
                );
            }

            // Directed gym card: CLAIM IT FOR HER at dispatch, under her own airc
            // identity. The detached-solve SWE arm below fires her scored solve directly
            // (dispatch_staged_swe_solve — "we don't wait on her to re-derive a
            // work/claim from the kickoff"); gym cards never got the same cut, so
            // every round spent its first multi-minute turn per card on claim
            // ceremony the dispatcher had already decided (Joel 2026-08-15:
            // "taking 30 minutes to start coding sure is a flawed design").
            // #425-compatible: the claim is administrative — the WORK stays hers,
            // in-room, through her own cognition. Best-effort: a failed pre-claim
            // is REPORTED and the card stays claimable by hand.
            //
            // A CITIZEN-driven SWE card takes the same cut, and for the same reason it
            // was written: nothing detached will fire for it, so the ONLY thing standing
            // between the card and her work turn is the kickoff→claim hop that stalls
            // rounds. Pre-claiming removes that hop without moving the work — she still
            // does it herself, in her own loop, on the held-work turn. (This goes through
            // airc directly, not the `work/claim` verb, so it cannot re-enter the
            // detached-solve dispatcher from here.)
            let mut pre_claimed = false;
            //
            // `staged_ok` gates the SWE arm for the SAME reason it gates the detached
            // solve above ("why run broken code knowing she's gonna struggle and fall"):
            // an unbuildable env can only void, and pre-claiming would put her hands in it
            // for a full turn. The card still posts, claimable by hand once the env heals.
            // BOTH drivers pre-claim a staged SWE card now. DetachedSolve was
            // excluded, which left its cards Open forever: the solve never touches
            // card state and the lapse sweeper refuses unclaimed cards, so a
            // detached round could never reach Done and every boot reaped it
            // (mapped 2026-08-26). Claimed-by-the-assignee is what lets the
            // grade path close the card and the round complete.
            // PRE-CLAIM only for a detached-solve round: its solve fires for the
            // directed assignee below, so the claim must already be hers. A
            // citizen-driven round posts its cards OPEN — that is what makes the deck
            // a deck. (Before 2026-09-03 every card, both drivers, was pre-claimed for
            // its round-robin assignee: a push wearing a pull's clothes.)
            let pre_claim_this = driver == crate::cognition::bench_round::WorkDriver::DetachedSolve
                && staged_ok
                && matches!(pc.work, CardWork::Swe { .. });
            if pre_claim_this {
                match self.registry.get(*who_peer) {
                    Some(rt) => {
                        let ttl_ms = crate::modules::work::DEFAULT_CLAIM_TTL_MS;
                        let mut attempt = rt
                            .airc()
                            .claim_work_card_with_origin(
                                airc_lib::ClaimWorkCard { card_id, ttl_ms },
                                airc_work::ClaimOrigin::Automatic,
                            )
                            .await;
                        // Follow the card to its room, same as work/claim (#328).
                        // Sequential dispatch mints a solve room per card, so the
                        // assignee's current-room pointer trails one card behind and
                        // every pre-claim after the first bounced on wrong-room
                        // (observed on the first team round, 2026-08-30: 3 of 8).
                        if matches!(
                            attempt,
                            Err(airc_lib::AircError::WorkCardNotInCurrentRoom { .. })
                        ) {
                            if let Some(retry) = crate::modules::work::claim_following_card_room(
                                rt.airc(),
                                card_id,
                                ttl_ms,
                                airc_work::ClaimOrigin::Automatic,
                            )
                            .await
                            {
                                attempt = retry;
                            }
                        }
                        match attempt {
                            Ok(_) => pre_claimed = true,
                            Err(e) => kickoff_errors.push(format!("pre-claim {short}: {e}")),
                        }
                    }
                    None => kickoff_errors
                        .push(format!("pre-claim {short}: {who} has no live airc runtime")),
                }
            }

            // Directed dispatch: round-robin an addressed kickoff per card. An addressed
            // imperative in its OWN message block is what actually starts work (measured
            // 2026-08-07: coalesced mid-burst it was ignored). airc.say is one event = one
            // block, so the structural condition holds by construction.
            // Per-card, ADDRESSED kickoffs are a detached-solve round's activation path
            // (the assignee is named, her solve is already running). A citizen-driven
            // round gets ONE room-addressed kickoff after the loop instead — twelve
            // per-card kickoffs cost every resident twelve inbound turns of near-
            // duplicate noise per dispatch (measured 2026-09-03: `deferred_loop_filler`
            // ×9 per citizen), for a deck the substrate pulls from deterministically.
            if driver == crate::cognition::bench_round::WorkDriver::DetachedSolve {
                let kickoff = match &pc.work {
                    CardWork::Gym { solution_file } if pre_claimed => format!(
                        "@{who} (to you): card {short} is CLAIMED FOR YOU on this board — \
                         no claim step needed. Read its body, write your solution to \
                         `{solution_file}` in your workspace NOW, then mark it done \
                         (work/state {short} done). Your artifact gets graded against \
                         held-out tests."
                    ),
                    CardWork::Gym { solution_file } => format!(
                        "@{who} (to you): card {short} on this board is yours. Claim it \
                         (claim_task {short}), read its body, write your solution to \
                         `{solution_file}` in your workspace, then mark it done \
                         (work/state {short} done). Your artifact gets graded against \
                         held-out tests."
                    ),
                    CardWork::Swe { instance } => {
                        let staged = if staged_ok {
                            format!(
                                " The repo is STAGED in your workspace at `swe/{}/`.",
                                instance.instance_id
                            )
                        } else {
                            String::new()
                        };
                        // The close coaching is NOT optional decoration: the gym arm
                        // always carried it, this arm said "watch the room for the
                        // verdict" — passive voice — and citizens did exactly that:
                        // wrote real patches, never said done, and every settle came
                        // from the lapse sweeper confiscating a partial (both misses
                        // of 2026-08-31). The verb that fires her grade is HERS.
                        format!(
                            "@{who} (to you): card {short} is a REAL {} issue (SWE-bench, a full \
                             project).{staged} I've STARTED your scored solve on it — fix the bug \
                             in `swe/{}/` (do not edit the tests); your diff is graded against the \
                             repo's held-out tests, and you get a few attempts to investigate your \
                             own failures. When your fix is ready, YOU close it: `work/state \
                             {short} done` — that is what fires your grade; nobody fires it for \
                             you. Teammates each hold their OWN card in this room — a kickoff \
                             naming someone else is not yours; {who}, card {short} is the one and \
                             only card you work.",
                            instance.repo, instance.instance_id
                        )
                    }
                };
                // AUTHOR IT AS SOMEONE ELSE. A citizen's inbound stream skips messages
                // she is recorded as having said (correct — nobody answers their own
                // speech), so a kickoff addressed to her and AUTHORED by her is dropped
                // silently and she never takes a turn.
                //
                // That is not hypothetical: `curator_airc` falls back to
                // `any_live_citizen()` for the operator (no self-peer, #27), which picks
                // the lexicographically-lowest name. With the roster at `Atlas` + `Benchy`
                // that is ALWAYS Atlas — so a round directed at Atlas sent her three
                // kickoffs she authored herself, reported `kickoff_errors: []`, and
                // produced ZERO turns while a detached solver did the work beside her
                // (measured 2026-08-17). A bigger roster usually picked someone else,
                // which is why it read as intermittent (#417) rather than structural.
                let voice_rt = match self.registry.any_live_citizen_other_than(Some(*who_peer)) {
                    Some(rt) => rt,
                    None => {
                        // The only live citizen IS the addressee. Refuse loudly instead of
                        // sending a message that cannot be heard — the card stays on the
                        // board, and the operator learns the roster is too small to direct
                        // work at all ([[fallbacks-are-illegal-fail-loud]]).
                        kickoff_errors.push(format!(
                            "{short}: {who} is the only live citizen, so nobody else can \
                             voice a kickoff addressed to her — she would skip her own \
                             message and never take a turn. Spawn a second citizen \
                             (persona/spawn), then re-dispatch."
                        ));
                        continue;
                    }
                };
                // SAY IT IN THE ROUND'S ROOM — never `say()`. Plain `say` posts to the
                // VOICE's current room, and the voice is by construction a different
                // citizen than the addressee, standing wherever she happens to stand.
                // Measured 2026-08-21 (the third deaf-kickoff variant): two dispatches
                // reported `kickoffs: 1`, both messages landed in the voice's own room,
                // the addressee's stream never carried them — and Atlas spent the
                // afternoon solving a gym exercise she COULD see while her staged SWE
                // card sat silent. The voice also JOINS the round room first (idempotent,
                // epoch-bumping), because membership is what makes say_in deliverable —
                // the same rule the assignee join-loop above encodes for hearing.
                if let Err(e) = voice_rt.join_room(&room_name).await {
                    kickoff_errors.push(format!("{short}: voice join: {e}"));
                    continue;
                }
                match crate::persona::airc_citizen::publish_text_in_room(
                    voice_rt.airc(),
                    room.room_id.as_uuid(),
                    &kickoff,
                )
                .await
                {
                    Ok(_) => kickoffs += 1,
                    // The card stays claimable — a lost kickoff is REPORTED (never unwound
                    // or hidden); the citizen can still find and claim it off the board.
                    Err(e) => kickoff_errors.push(format!("{short}: {e}")),
                }
            }

            // DIRECTED SWE dispatch FIRES her scored solve directly — the repo is staged in
            // her workspace and the card is addressed to her, so we don't wait on her to
            // re-derive a work/claim from the kickoff (the hop that stalls under warm-slot
            // starvation — glass-boxed 2026-08-11: cards staged + assigned, zero claims,
            // zero solves). Her WHOLE cognition solves it with an exclusive warm slot; the
            // work/claim path stays the trigger for undirected / human-claimed cards. Only a
            // STAGED SWE card has a solve to fire here (a gym card self-grades differently).
            if staged_ok
                && solves_fired < solve_cap
                && driver == crate::cognition::bench_round::WorkDriver::DetachedSolve
            {
                if let CardWork::Swe { .. } = &pc.work {
                    // The run room goes WITH the solve: her acts radiate receipts
                    // into the room this dispatch just spawned, so the round's work
                    // is visible where the round lives (#243/#329) instead of only
                    // in a ledger file that lands when it is already over.
                    crate::modules::work::dispatch_staged_swe_solve(
                        ctx,
                        &airc,
                        crate::modules::work::StagedSolveDispatch {
                            // The roster still carries bare `(String, Uuid)` tuples — a
                            // loose-id smell of its own (#396). Typed at THIS boundary so
                            // the dispatch cannot confuse a peer with a room; typing the
                            // roster itself is that card's work, not a silent widening here.
                            claimer: crate::identity::PeerId::from_uuid(*who_peer),
                            card: card_id,
                            teammates: teammates
                                .iter()
                                .filter(|m| m.as_uuid() != *who_peer) // the claimer is not her own teammate
                                .cloned()
                                .collect(),
                            room: room.room_id,
                        },
                    )
                    .await;
                    solves_fired += 1;
                }
            }
            card_uuids.push(card_id.as_uuid());
            card_ids.push(short);
        }

        // THE ROUND BECOMES AN ENTITY (#371): register the dispatched card set under the
        // run room's own id (dispatch already mints one id per run — the room; never a
        // second one). From here the round has a lifecycle somebody owns: the
        // `work.card.state_changed` subscriber settles cards as they reach terminal
        // states and announces the END — instead of the round's fate being probe
        // archaeology ("random and directed by agent, not an ecosystem", Joel 8/16).
        // Announce the open deck as the actual dispatch caller. Borrowing a citizen's
        // identity makes a singleton recipient drop the announcement as her own echo.
        // This offers work; claiming remains a citizen act, never automatic assignment.
        if driver == crate::cognition::bench_round::WorkDriver::Citizen && !card_ids.is_empty() {
            match publish_citizen_deck(&airc, room.room_id.as_uuid(), spec.name, &card_ids).await {
                Ok(_) => kickoffs += 1,
                Err(e) => kickoff_errors.push(format!("deck kickoff: {e}")),
            }
        }
        crate::cognition::bench_round::seal_round(room.room_id.as_uuid());

        // PRUNE (opt-in): converge the board to one live card per task for THIS
        // benchmark. Scoped to the keys this dispatch planned, so pruning one
        // benchmark can never touch another's cards — or a hand-written one.
        //
        // Runs AFTER dispatch so a card just posted for a task is already in the
        // group and cannot be orphaned by a prune that ran against a stale read.
        let mut pruned_duplicates = 0u32;
        let mut contended_tasks = 0u32;
        if p.prune.unwrap_or(false) {
            let now_ms = crate::time::EpochMs::now().millis();
            for key in planned_keys.iter() {
                let Some(group) = live_by_task.get(key.as_str()) else {
                    continue;
                };
                let holds: Vec<_> = group
                    .iter()
                    .map(|c| crate::persona::card_holder::hold_of(c, now_ms))
                    .collect();
                let (to_close, contended) = duplicates_to_close(&holds);
                if contended {
                    contended_tasks += 1;
                }
                for idx in to_close {
                    let card_id = group[idx].card_id;
                    match airc
                        .change_work_card_state(airc_lib::ChangeWorkCardState {
                            card_id,
                            state: airc_lib::CardState::Closed,
                        })
                        .await
                    {
                        Ok(_) => pruned_duplicates += 1,
                        // A close that fails is REPORTED, never silently dropped —
                        // the caller must not read a partial prune as a clean board.
                        Err(e) => kickoff_errors.push(format!(
                            "prune {}: {e}",
                            card_id.as_uuid().simple().to_string()[..8].to_string()
                        )),
                    }
                }
            }
        }

        // Room-join failures ride the SAME reported channel as kickoff failures — a
        // citizen who never made it into the run is exactly as invisible as a kickoff
        // that never landed, and neither may be silent.
        kickoff_errors.extend(room_join_errors.into_iter().map(|e| format!("join room: {e}")));

        Ok(BenchmarkDispatchResult {
            benchmark: spec.name.to_string(),
            room: room.name,
            room_id: room.room_id,
            binding_post_id: room.binding_post_id,
            dispatched: card_ids.len() as u32,
            card_ids,
            skipped_needs_setup,
            skipped_known_red,
            skipped_already_on_board,
            skipped_already_resolved,
            pruned_duplicates,
            contended_tasks,
            kickoffs,
            solves_fired,
            kickoff_errors,
        })
    }
}
