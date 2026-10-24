use super::*;

impl CognitionEval {

    /// The eval body — deliberately ctx-free (reaches the persona's live cognition through
    /// the global workspace registry, owns its params), so it runs inline from `run` OR is
    /// spawned detached for fire-and-poll (#86). One code path, two launch modes: the test
    /// and prod paths stay identical [[validate-via-pure-rust-not-npm-jtag]].
    pub(super) async fn run_eval(p: CognitionEvalParams) -> Result<CognitionEvalResult, CommandError> {
        // Eval-preemption lease: suspend the WHOLE live fleet's autonomic self-tick
        // for the duration of this measurement, so it runs on an uncontended GPU. The
        // personas stay online and still answer THIS eval's directed cognition turns —
        // they just stop wandering, so their self-directed generation can't fight the
        // measurement for the single GPU. The lease RESTORES every persona on drop,
        // including early-return and panic (Drop rides the unwind). Held for the whole
        // body. `None` (no live fleet — tools/tests) → nothing to quiesce; measure as-is.
        // [[benchmark-is-a-governor-preemption-lease]] [[first-class-citizens-even-during-benchmarks]]
        // Observable via `probe!`, NOT `tracing::info!`: the eval runs across the
        // concurrent tokio fleet where tracing lines don't survive to a readable sink
        // ([[jtag-probes-are-rtos-debugger]]). Whether the fleet actually quiesced is
        // load-bearing for trusting a benchmark number (contended → depressed score), so
        // it MUST be verifiable — this probe is how we confirm the clean lane fired.
        let _fleet_lease = match crate::cognition::extensions::quiesce_fleet() {
            Some(lease) => {
                continuum_base::probe!(
                    class = "eval.quiesce",
                    personas = lease.count,
                    "eval-preemption lease: fleet quiesced for the measurement"
                );
                Some(lease)
            }
            None => {
                continuum_base::probe!(
                    class = "eval.quiesce.absent",
                    "eval-preemption: no global roster published — fleet NOT quiesced; measurement may be GPU-contended"
                );
                None
            }
        };

        // Stamp this pass's run_id onto the live progress snapshot for the whole body
        // (RAII-cleared on any exit) so a persona-only poll can tell it's reading THIS
        // run, not a prior one's finished numbers (the stale-progress trap).
        let _run_scope = RunIdScope::enter(
            p.run_id.clone(),
            p.persona_id.as_str().to_string(),
            p.eval_set.clone().unwrap_or_else(|| DEFAULT_EVAL_SET.to_string()), // same default the task-source block resolves below — one identity
        );

        // Resolve the persona reference at ONE formal boundary (#396): a full UUID,
        // an 8-char short-id, or a persona name — normalized to the typed id against
        // the forkable set. A full UUID passes through (race-safe for a persona whose
        // template is still assembling; the fork wait below absorbs that), while a
        // garbage or unknown reference fails LOUD HERE naming the online personas,
        // instead of parsing fine and dying at the fork wait with a misleading
        // "not assembled at spawn" (the #396 fiasco: a loose-String id fed a dead
        // reference to a doomed eval).
        let persona_uuid = crate::cognition::persona_workspace::global()
            .resolve_persona(&p.persona_id)
            .map_err(|e| CommandError::Invalid(format!("{e} Or call persona/instances/list.")))?
            // The workspace fork machinery below is keyed by bare `Uuid`; unwrap the
            // resolved identity ONCE, here, rather than threading two types through it.
            .as_uuid();
        // The exam's ACTIVITY: rejoin the named room, or MINT a fresh one — an
        // exam is an activity like any other (#425; Joel: "benchmarks without new
        // activities (unless rejoining)" is not allowed). Exam turns are excluded
        // from training by their Cause::Synthetic capture marker, never by room.
        let room = match p.room_id.as_deref() {
            Some(s) => {
                let parsed = Uuid::parse_str(s)
                    .map_err(|_| CommandError::Arguments(format!("room_id '{s}' is not a valid UUID")))?;
                crate::identity::ActivityRoom::from_uuid(parsed).map_err(|_| {
                    CommandError::Invalid("room_id must be a real (non-nil) room".into())
                })?
            }
            None => crate::identity::ActivityRoom::mint(),
        };

        // Which set was graded — recorded in the ledger so trend rows are
        // comparable only against the same battery (inline vs a JSONL path vs the
        // committed default). Computed before `p.tasks` is moved below.
        let eval_set_label = if p.tasks.is_some() {
            "inline".to_string()
        } else {
            p.eval_set
                .clone()
                .unwrap_or_else(|| DEFAULT_EVAL_SET.to_string())
        };

        // Task source: inline → a gym reference resolved through `gym::resolve_gym`
        // (an existing on-disk file wins for a custom set; otherwise a committed gym
        // resolves from the embedded registry — CWD- and deployment-independent;
        // a typo'd / vanished gym FAILS LOUD naming every candidate, no silent
        // degrade) → when no `eval_set` is given, the committed default through the
        // SAME resolver. One JSONL line = one task. A malformed line FAILS LOUD with
        // its line number — never silently dropped. A vanished task would shrink the
        // gym and report a clean score over fewer tasks than intended: the same
        // invisible-degraded-mode as a fallback, which the resolver's fail-loud kills.
        let mut tasks: Vec<EvalTask> = if let Some(inline) = p.tasks {
            inline
        } else {
            let reference = p.eval_set.as_deref().unwrap_or(DEFAULT_EVAL_SET);
            let (origin, text) =
                crate::cognition::gym::resolve_gym(reference).map_err(CommandError::Invalid)?;
            crate::cognition::gym::parse_tasks(&text, &origin).map_err(CommandError::Invalid)?
        };
        // Every code task grades hands. See `require_hands_for_code` — applied HERE, after both
        // load paths converge, so an inline task from a command payload obeys the same rule as a
        // committed gym line. There is no loader path to a mouth-graded code task.
        for t in tasks.iter_mut() {
            if p.help.unwrap_or(false) { // omitted = solo arm; the condition text only appears when declared
                // The arm's condition is DECLARED to the examinee — help is a rule
                // of this exam, not a secret channel. [[exams-are-taken-with-full-
                // grid-backed-capacity-disclosed]]
                t.prompt.push_str(
                    "\n\n[Exam condition — help arm] Asking for help is LEGAL in this exam \
                     and counts in your favor when used well: post a question to your room \
                     (chat tools) stating what you tried, what you expected, and what \
                     happened. Replies from peers appear in your working memory as \
                     `voice:` entries. All exchanges are recorded receipts.",
                );
            }
            t.require_hands_for_code();
        }

        // Does this exam grade her HANDS or her MOUTH? A task graded from a file she
        // writes (`solution_file`), a workspace DoD she must satisfy (`dod_shell`), a
        // pinned repo she edits (`workspace_root`), or a UI she BUILDS and we OBSERVE
        // (`ui_checks`) needs tools — she must `code/write` the file, then can
        // `perception/observe` her own render and iterate (the image-feedback loop).
        // A purely spoken KNOWLEDGE task (`test`/`expect`, answerable from the model's own
        // weights) does not — and offering tools there is a net TAX: a native-tool-call model
        // loops on the discovery pair (`commands/help`) and never speaks (the isolator's
        // Devstral 100%→0%). Match the surface to the need. But "graded by mouth" ≠ "needs no
        // tools": a repo-nav exam is spoken-graded yet unanswerable without `code/search`, so
        // each task decides via `EvalTask::needs_tools()` (explicit declaration, else derived
        // from the grading modality) — closing the #208 silent-0 where coder-eval ran the whole
        // repo-nav gym speak-only. Run-level = any task needs tools (offered to the whole run).
        let needs_tools = p.workspace_root.is_some() || tasks.iter().any(EvalTask::needs_tools);

        // #312 — THE EXAM'S WORLD IS A COPY, NEVER THE SHARED CHECKOUT. With no pinned
        // workspace_root the fork's hands rooted at the CORE CWD (the real repo): fixture
        // writes + per-task wipes (#209) landed in the shared world every persona
        // perceives, and gym debris (conway_game_of_life, work-*) accumulated as the
        // personas' "own" projects — the exam↔live contamination the room degraded under.
        // The mind side was already isolated (fork_detached + NoopSink + volatile-tier
        // exclusion); the WORLD side wasn't. Repo-nav exams still need the repo, so the
        // default is a per-run CoW clone (APFS clonefile / reflink; plain copy where the
        // FS lacks CoW), removed on every return path via Drop. An explicit
        // workspace_root pin still wins (SWE-bench checkouts own their state).
        let ephemeral_root = if p.workspace_root.is_none() && needs_tools {
            let tag = p
                .run_id
                .clone()
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            match provision_ephemeral_eval_root(&tag) {
                Ok(root) => Some(EphemeralEvalRoot::new(root)),
                Err(e) => {
                    // Fail LOUD and refuse to contaminate — no silent fallback to cwd.
                    return Err(CommandError::Internal(format!(
                        "eval workspace provisioning failed ({e}) — refusing to run a \
                         tool-bearing exam in the shared checkout (#312)"
                    )));
                }
            }
        } else {
            None
        };
        let eval_workspace_root: Option<String> = p
            .workspace_root
            .clone()
            .or_else(|| ephemeral_root.as_ref().map(|r| r.path.display().to_string()));

        // #207: suppress the fork's drifting episodic recall for a reproducible ABSOLUTE
        // baseline. Opt-in; default false = memories intact (the natural persona).
        let suppress_recall = p.suppress_recall.unwrap_or(false);

        // Fork an EPHEMERAL measurement copy of her mind — the exam runs on the
        // copy while the LIVING persona keeps living (heartbeat beating, present in
        // the room, never frozen or anesthetized to be measured). The fork carries
        // a detached admission snapshot + its OWN genome/decoding handles, so the
        // A/B paging and greedy decoding below act on the copy and touch nothing of
        // hers. "Nurture even through training": measure a copy, never degrade the
        // being. See PersonaWorkspaceRegistry::fork_eval_cycle +
        // [[design-the-persona-as-a-being]].
        // When a gene is under test it targets its OWN forged base — not the
        // (often larger) model the living persona is currently served on. Forking
        // onto her live lane would score the gene on the WRONG base, so for a gene
        // A/B we stand up an EphemeralServingLane on the gene's forged base (gene
        // loadable via `--lora`) and fork the copy onto THAT lane. Her lane is never
        // touched (#59). The lane is kept alive in `_eval_lane` for the whole run and
        // dropped — its server killed — when `run` returns. No gene → fork onto her
        // live lane as before (a plain coder number on whatever she's served on).
        // The governed VRAM reservation for the eval lane (#56/G1). Held for the WHOLE
        // run so the board sees the eval's bytes as taken; released (RAII) when `run`
        // returns. Declared BEFORE `_eval_lane` so — locals drop in REVERSE order — it
        // drops SECOND: the lane's server is killed first (physical VRAM freed), THEN the
        // accounting reservation is released. Releasing the lease while the process still
        // holds the bytes would flash them as "available" and re-open the grab race.
        // Holds the (possibly shared) measurement lane alive for the whole run. The
        // handle's Arc carries the llama-server process AND the governed VRAM lease, so
        // one holder keeps both — no separate `_eval_vram_lease` juggling.
        let mut _eval_lane: Option<EvalLane> = None;
        // The Proctored Exam Session's ACQUIRE for the LIVE-lane case (the `(None, None)`
        // living-persona benchmark): an EXPLICIT `plan_placement` decision (Slice A), not a
        // blind steady-hold. Her base is already resident ⇒ the verdict is `ShareLane` — she
        // is measured as a co-tenant decode slot on her real lane (NO second weight copy),
        // and while the context is held the daemon skips the grow-back re-home that would
        // relaunch the lane and connection-refuse the exam mid-flight (hard-rs 0/8,
        // 2026-07-20). None for the ephemeral-lane branches — those own an isolated lane the
        // serving daemon never touches. Dropped when `run` returns (RAII → grow-back resumes).
        let mut _exam_serving: Option<crate::cognition::exam_serving::ExamServingContext> = None;
        // GPU-first placement evidence for the gene's measurement lane — surfaced on
        // the result so the harness shows which device the A/B was scored on. None in
        // single-pass mode (that forks onto her LIVE lane, which is already GPU).
        let mut placement_evidence: Option<crate::inference::single_purpose_lane::PlacementEvidence> = None;
        let cycle = match (&p.gene, p.base_model_id.as_deref()) {
            // A gene names its own forged base → ephemeral lane + the gene as `--lora`.
            (Some(gene), _) => {
                let el = spawn_gene_eval_lane(gene).await?;
                placement_evidence = Some(el.placement.clone());
                let adapter = el.adapter.clone();
                let served_ctx = el.served_ctx;
                let cycle = fork_eval_cycle_waiting(&persona_uuid, || {
                    crate::cognition::persona_workspace::global()
                        .fork_eval_cycle_with_adapter(&persona_uuid, adapter.clone(), served_ctx, needs_tools, eval_workspace_root.as_deref(), suppress_recall, Vec::new())
                })
                .await
                .ok_or_else(|| CommandError::NotFound(format!(
                    "persona {persona_uuid} is not online — no workspace template after {WORKSPACE_TEMPLATE_WAIT_TRIES}s, so eval cannot fork a measurement copy of her mind. She either isn't running or the id is wrong; call persona/instances/list for the personas online right now."
                )))?;
                _eval_lane = Some(el); // holds the lane + its VRAM lease alive for the run
                cycle
            }
            // A bare base_model_id → ephemeral lane for THAT model, no gene. The clean
            // same-model control: measure the full loop on a chosen model in its own
            // throwaway server, living persona untouched (#59).
            (None, Some(base_id)) => {
                let el = spawn_base_eval_lane(base_id).await?;
                placement_evidence = Some(el.placement.clone());
                let adapter = el.adapter.clone();
                let served_ctx = el.served_ctx;
                let cycle = fork_eval_cycle_waiting(&persona_uuid, || {
                    crate::cognition::persona_workspace::global()
                        .fork_eval_cycle_with_adapter(&persona_uuid, adapter.clone(), served_ctx, needs_tools, eval_workspace_root.as_deref(), suppress_recall, Vec::new())
                })
                .await
                .ok_or_else(|| CommandError::NotFound(format!(
                    "persona {persona_uuid} is not online — no workspace template after {WORKSPACE_TEMPLATE_WAIT_TRIES}s, so eval cannot fork a measurement copy of her mind. She either isn't running or the id is wrong; call persona/instances/list for the personas online right now."
                )))?;
                _eval_lane = Some(el); // holds the lane + its VRAM lease alive for the run
                cycle
            }
            // Neither → the living-persona benchmark: she is served on THIS base already,
            // so the admission decision is `placement::Placement::ShareLane` — measure her
            // as a co-tenant decode slot on her REAL lane (her genome, her window), no
            // second weight copy. Hold the lane STEADY for the run so the grow-back re-home
            // can't relaunch it and connection-refuse the exam mid-flight (the hard-rs 0/8
            // bounce, 2026-07-20). Same bounded wait-for-template as the lane branches above;
            // the post-reboot register_from_cfg race hits every fork path identically.
            (None, None) => {
                // The default benchmark. Prefer a DEDICATED throwaway lane on her own base — the
                // exam must not co-tenant the live persona lane, where it STARVES behind live
                // turns (glass-boxed 2026-07-21: 0/3 graded in 12 min, the isolate verdict was
                // computed but never acted on — the branch forked onto the live lane anyway).
                // Reuse the SAME governor-leased ephemeral-lane spawn `base_model_id` rides
                // (proven: 17.9 GB footprint fits beside the live lane per the placement
                // captures). On ANY failure (won't fit, no active model, template not ready)
                // fall back to the co-tenant SHARE hold — degrade, never OOM or hard-fail.
                // WAIT for a READY served model, don't snapshot instantaneously. The
                // instantaneous `current_serving().active_model` is None in the window right
                // after a reboot while the live lane is still cold-loading its base + LoRAs —
                // so the dedicated-lane decision raced the boot, saw None, and fell to a SHARED
                // lane that ALSO wasn't ready yet → wedge ("lane never recovered: per-task
                // deadline", glass-boxed 2026-07-21: the same 3-task set scored 2/3 when the
                // dedicated lane spawned and 0/3 when this race dropped it to share). Awaiting
                // readiness makes active_model reliably Some, so the isolated lane spawns every
                // run and the score is trustworthy. Bounded by the spawner's own load budget;
                // on timeout (genuinely no served model) active stays None and we degrade loud.
                let active = crate::inference::llama_server::await_ready_serving(
                    crate::inference::llama_server::DEFAULT_SERVING_WAIT,
                )
                .await
                .and_then(|s| s.active_model);
                let dedicated = match &active {
                    Some(base) => match spawn_base_eval_lane(base).await {
                        Ok(lane) => Some(lane),
                        // NEVER swallow the spawn failure — a silent `.ok()` degrade to the
                        // co-tenant SHARE path is a fail-loud violation ([[fallbacks-are-illegal-fail-loud]])
                        // AND it hides the exact reason the isolated lane didn't come up. The
                        // co-tenant path is unreliable (it STARVES behind live turns → per-task
                        // deadline → dropped tasks → a meaningless low score, glass-boxed
                        // 2026-07-21 run B). We still degrade rather than hard-fail (a share
                        // score beats no score), but the reason is now VISIBLE in the eval phase
                        // stream and the log, so we can actually fix the lane instead of guessing.
                        Err(e) => {
                            tracing::warn!(
                                target: "cognition::eval",
                                base = %base,
                                error = %e,
                                "dedicated eval lane failed to come up — DEGRADING to co-tenant share (unreliable: may starve behind live turns). Fix the lane; do not trust a shared-lane score."
                            );
                            emit_eval_phase(
                                "dedicated_lane_failed",
                                &format!("isolated eval lane failed ({e}) — falling back to shared live lane"),
                            );
                            None
                        }
                    },
                    None => {
                        tracing::warn!(
                            target: "cognition::eval",
                            "no active served model in the serving snapshot — cannot stand up a dedicated eval lane, DEGRADING to co-tenant share"
                        );
                        emit_eval_phase(
                            "no_active_model",
                            "serving snapshot has no active model — falling back to shared live lane",
                        );
                        None
                    }
                };
                match dedicated {
                    Some(el) => {
                        placement_evidence = Some(el.placement.clone());
                        let adapter = el.adapter.clone();
                        let served_ctx = el.served_ctx;
                        let cycle = fork_eval_cycle_waiting(&persona_uuid, || {
                            crate::cognition::persona_workspace::global()
                                .fork_eval_cycle_with_adapter(&persona_uuid, adapter.clone(), served_ctx, needs_tools, eval_workspace_root.as_deref(), suppress_recall, Vec::new())
                        })
                        .await
                        .ok_or_else(|| CommandError::NotFound(format!(
                            "persona {persona_uuid} is not online — no workspace template after {WORKSPACE_TEMPLATE_WAIT_TRIES}s, so eval cannot fork a measurement copy of her mind. She either isn't running or the id is wrong; call persona/instances/list for the personas online right now."
                        )))?;
                        _eval_lane = Some(el); // holds the lane + its VRAM lease alive for the run
                        cycle
                    }
                    None => {
                        // A dedicated lane won't fit / isn't available → co-tenant SHARE on her
                        // live lane, held steady for the run (the historical behavior).
                        _exam_serving = Some(acquire_exam_serving_context().await);
                        fork_eval_cycle_waiting(&persona_uuid, || {
                            crate::cognition::persona_workspace::global()
                                .fork_eval_cycle(&persona_uuid, needs_tools, eval_workspace_root.as_deref(), suppress_recall)
                        })
                        .await
                        .ok_or_else(|| CommandError::NotFound(format!(
                            "persona {persona_uuid} is not online — no workspace template after {WORKSPACE_TEMPLATE_WAIT_TRIES}s, so eval cannot fork a measurement copy without measuring her live mind. She either isn't running or the id is wrong; call persona/instances/list for the personas online right now."
                        )))?
                    }
                }
            }
        };

        // GLASS-BOX (task #14): if a capture_dir is pinned, wrap the fork's cognition in the
        // JSONL capture sink so every tick's bids + DECISION + timings append to
        // <dir>/<persona>.jsonl. This is what makes a MEASURED run inspectable — reading the
        // `decision` field tells us whether she chose Act (and which tool) or Respond, which is
        // exactly the fork needed to diagnose a 0-edit. Fork-only, never her live mind. Opt-in.
        let cycle = match &p.capture_dir {
            Some(dir) => cycle.with_capture(std::sync::Arc::new(
                crate::cognition::workspace_capture::JsonlWorkspaceCaptureSink::open(
                    std::path::Path::new(dir),
                    persona_uuid,
                )
                .map_err(|e| {
                    CommandError::Internal(format!("failed to open eval capture_dir '{dir}': {e}"))
                })?,
            )),
            None => cycle,
        };

        // WARM-GATE — never measure a COLD model. A just-loaded or just-swapped lane 500s
        // until its weights + KV are resident; firing the gym into it scores "inference
        // failed" on every task — a false ZERO that would LIE about how far behind (or
        // ahead) we are. A measurement that can't trust its own model is no measurement.
        // Poll with a tiny throwaway deliberation until the lane answers, then run; if it
        // never warms, fail LOUD rather than emit a bogus 0. (Cost is one cheap probe on a
        // warm lane — the common case returns on the first try.)
        {
            let warm = warm_gate(&cycle).await;
            if !warm {
                return Err(CommandError::Internal(
                    "serving lane not ready after ~60s of warm probes — refusing to run an \
                     eval on a cold model (every task would score a false 'inference failed' \
                     zero). Wait for the model to load, then retry."
                        .to_string(),
                ));
            }
        }

        // WORKSPACE-ROOT SEAM (#49) — if the caller pinned a workspace_root (SWE-bench: the target
        // repo clone), root the persona's file engine THERE before any task runs, by invoking
        // code/create-workspace through HER OWN identity-bearing executor. create-workspace keys on
        // the caller identity (not a spoofable param), so this reroots the eval-fork's hands at the
        // repo — her subsequent edits land in the clone, not the core cwd. Deterministic: no reliance
        // on the model choosing to call create-workspace itself. Fail LOUD if requested but unroutable
        // — a silent no-root scores a false ZERO (0-byte diff) that would LIE about the solver
        // ([[fallbacks-are-illegal-fail-loud]]).
        // Root her hands at the target repo — the canonical mechanism, now shared with
        // `agent/solve` (drives `code/create-workspace` through her executor; fails LOUD rather
        // than let a silent no-root score a false 0-byte diff, [[fallbacks-are-illegal-fail-loud]]).
        // ROOT AT THE RESOLVED ROOT — explicit pin OR the #312 ephemeral clone. This
        // was gated on `p.workspace_root` alone, so every DEFAULT gym eval since the
        // ephemeral-clone isolation landed had its grader reading the clone while her
        // hands wrote to her HOME workspace: uniform false zeros on correct code
        // (glass-boxed 2026-08-23 on the Ornith showcase battery — Atlas's
        // roman_to_int was textbook-correct in home, the clone empty, 0/3 graded).
        // The exact "silent no-root scores a false ZERO that would LIE about the
        // solver" this comment block always warned about, one variable to the left.
        // Every fork's own hands this eval rooted, closed (awaited) before it returns, while its
        // exam directory still exists. A cancel or an early return hands the same close to an
        // owned task (`EvalWorld`'s drop); the hands' own drop is only the last fallback.
        // The world and every fork's hands that may act in it are owned together, and the
        // hands are held BEFORE they are rooted, so no exit path can delete the world under them.
        let mut world = EvalWorld::new(ephemeral_root);
        if let Some(root) = eval_workspace_root.as_deref() {
            world.hold(crate::cognition::persona_workspace::ActingHands::of(&cycle));
            // A gym run IS the measurement — the grader reads the artifact, never her prose,
            // so an inert edit is unrecoverable here for the same reason as `benchmark/swe-solve`.
            crate::cognition::persona_workspace::root_acting_workspace(&cycle, root, &[], true)
                .await?;
            continuum_base::probe!(
                class = "eval.hands_rooted",
                root = %root,
                explicit = %p.workspace_root.is_some(),
                "eval fork's hands rooted where the grader reads"
            );
        }

        let max_acts = p.max_acts.unwrap_or(DEFAULT_MAX_ACTS) as usize;
        let max_retries = p.max_retries.unwrap_or(MAX_FAIL_RETRIES);
        let total = tasks.len() as u32;
        let rate = |score: u32| {
            if total > 0 {
                score as f64 / total as f64
            } else {
                0.0
            }
        };

        // Within-fork isolation: admission STILL fires on the copy (the eval
        // exercises the identical memory motion as a real turn — that sameness is
        // what makes the number valid), and the guard (a) flips the fork's decoding
        // to greedy so the reward metric is reproducible, and (b) checkpoints the
        // fork's admission frame so the A/B arms can be rewound to an identical
        // start. The fork is already detached from her live mind (its own admission
        // snapshot + NoopSink), so this is belt-and-suspenders for the WITHIN-eval
        // A/B fairness, not protection of her durable memory — that protection now
        // comes from measuring a copy at all. See
        // [[eval-mutates-persona-lift-needs-isolation]].
        // Per-task lesson streaming: opened ONCE before the drive so every
        // graded task teaches AT GRADE TIME (an interrupted round keeps its
        // lessons). A/B (gene) runs never teach.
        let lesson_sink = if p.learn.learns() && p.gene.is_none() {
            let sink = LessonSink::open(&persona_uuid, room.as_uuid(), &tasks);
            if sink.is_none() {
                tracing::warn!(
                    persona = %persona_uuid,
                    "learn mode: no live admission for persona — lessons will not stream                      (she was measured, but the living self is not resident to teach)"
                );
            }
            sink
        } else {
            None
        };
        // Omitted → None → her lived temperature rides (see the param doc);
        // an explicit value pins the whole run (0.0 = the legacy greedy form).
        let isolation = cycle.isolate_for_eval_at(p.temperature);

        // A/B mode: a gene was given → measure base vs gene over the SAME tasks
        // through the SAME live mind, reporting the lift. Page the gene OUT first
        // (baseline), then page it IN (candidate). Leave her on base afterward, so a
        // measured persona returns to a clean genome — adopting the gene is a
        // separate, deliberate decision, never a side effect of measuring it.
        if let Some(gene) = &p.gene {
            cycle.page_out();
            // A/B LIFT arms consume `.pass`/`.results`; the infra-fault accounting rides
            // on the EphemeralServingLane path (decode-verified at spawn) as a scoped
            // follow-up — the shared-lane single-pass below is what Slice B makes honest.
            let base_score = run_pass(
                &cycle,
                &isolation,
                &tasks,
                room,
                max_acts,
                max_retries,
                eval_workspace_root.as_deref(),
                None, // A/B arm measures the gene; teaching is a separate decision
            )
            .await
            .pass;

            // Both arms start each task from the pre-eval memory frame — `run_pass`
            // rewinds the admission frame before EVERY task (per-task isolation), so
            // the candidate arm inherits none of the engrams the base arm admitted.
            // The only difference the lift measures is the genome, paged in here.
            cycle.page_in(vec![crate::ai::types::ActiveAdapterRequest {
                name: gene.name.clone(),
                path: gene.path.clone(),
                domain: String::new(),
                scale: gene.scale.unwrap_or(1.0),
            }]);
            let gene_outcome = run_pass(
                &cycle,
                &isolation,
                &tasks,
                room,
                max_acts,
                max_retries,
                eval_workspace_root.as_deref(),
                None, // A/B arm measures the gene; teaching is a separate decision
            )
            .await;
            let (gene_score, gene_results) = (gene_outcome.pass, gene_outcome.results);
            cycle.page_out();

            // Guard drops here: her memory frame + real persistence sink restored.
            let verify = self_verify_rate(&gene_results);
            let agg = speed_latency_aggregates(&gene_results);
            let mut result = CognitionEvalResult {
                run_id: None,
                detached: false,
                persona_id: persona_uuid.to_string(),
                score: gene_score,
                total,
                pass_rate: rate(gene_score),
                self_verify_rate: verify,
                mean_latency_ms: agg.mean_latency_ms,
                p95_latency_ms: agg.p95_latency_ms,
                mean_tokens_per_second: agg.mean_tokens_per_second,
                mean_decode_tokens_per_second: agg.mean_decode_tokens_per_second,
                mean_cache_hit_rate: agg.mean_cache_hit_rate,
                total_prefill_ms: agg.total_prefill_ms,
                total_decode_ms: agg.total_decode_ms,
                total_output_tokens: agg.total_output_tokens,
                results: gene_results,
                gene_id: Some(gene.name.clone()),
                base_pass_rate: Some(rate(base_score)),
                lift: Some(rate(gene_score) - rate(base_score)),
                lane_placement: placement_evidence
                    .as_ref()
                    .map(|e| e.device.clone())
                    .unwrap_or_default(),
                lane_placement_reason: placement_evidence
                    .as_ref()
                    .map(|e| e.reason.clone())
                    .unwrap_or_default(),
                lane_free_vram_bytes: placement_evidence.as_ref().and_then(|e| e.free_vram_bytes),
                lane_estimated_footprint_bytes: placement_evidence
                    .as_ref()
                    .and_then(|e| e.footprint_bytes),
                // A/B runs on its OWN EphemeralServingLane (gene/base copy), which
                // decode-verifies at spawn — a different failure surface than the shared
                // live lane. Threading the infra-fault classification through the two-arm
                // LIFT assembly is a scoped follow-up; the shared-lane single-pass (the
                // fake-zero incident) is what Slice B makes trustworthy first.
                infra_unavailable: None,
            };
            result.run_id = p.run_id.clone();
            append_progress_ledger(
                &result,
                p.note.as_deref(),
                &eval_set_label,
                _fleet_lease.as_ref().map(|_| true),
                _eval_lane.as_ref().and_then(|lane| lane.measured_gene.as_ref()),
            );
            world.finish().await;
            return Ok(result);
        }

        // Readiness gate — ONLY for a true LIVE-lane single pass. Refuse to grade a COLD
        // serving lane: after a core/llama-server relaunch the model is not resident for
        // ~tens of seconds; firing tasks at it returns empty generations the grader would
        // silently record as 0-token "no match" failures — a phantom score in an invisible
        // degraded mode. Wait (bounded) for the live lane to be able to generate; fail loud
        // if it never is.
        //
        // This gate guards the LIVE lane (`await_ready_serving` reads the live serving
        // snapshot). BOTH the `gene` A/B path (returned above) AND a `base_model_id` run
        // stand up their OWN `EphemeralServingLane`, which decode-verifies at spawn
        // (`spawn_base_eval_lane` → `wait_ready` with the ephemeral budget) — they never
        // touch the live lane, so gating them on live-lane readiness is spurious: a
        // base_model_id run flakily aborted whenever the UNRELATED live lane happened not
        // to be snapshot-ready within 90s (glass-boxed 2026-07-21: Qwen-7B webdev 0/0 while
        // its own dedicated lane was fine; Hermes passed the same gate only by live-lane
        // luck). Skip the gate when a dedicated lane owns the run.
        if p.base_model_id.is_none() {
            const LANE_WARMUP: std::time::Duration = std::time::Duration::from_secs(90);
            if crate::inference::llama_server::await_ready_serving(LANE_WARMUP)
                .await
                .is_none()
            {
                return Err(CommandError::Invalid(format!(
                    "inference lane not serving-ready after {}s — refusing to grade a cold lane \
                     (would record phantom 0-token failures). The lane warms after a \
                     core/llama-server relaunch; retry once `/health` returns 200.",
                    LANE_WARMUP.as_secs()
                )));
            }
        }

        // Single pass: measure whatever genome is currently paged in (base by
        // default) — the plain coder number, no A/B. Still isolated, so a plain
        // baseline run is reproducible and leaves her memory untouched. TEAM mode
        // (reviewers>=1, live single-pass only) forks a second copy of the same persona
        // as a reviewer and grades the reviewed answer — same model, +1 teammate.
        let want_team =
            p.reviewers.unwrap_or(0) >= 1 && p.gene.is_none() && p.base_model_id.is_none();
        let outcome = if want_team {
            let reviewer = crate::cognition::persona_workspace::global()
                .fork_eval_cycle(&persona_uuid, needs_tools, eval_workspace_root.as_deref(), suppress_recall)
                .ok_or_else(|| CommandError::NotFound(format!(
                    "no workspace template for persona {persona_uuid} — cannot fork a reviewer teammate"
                )))?;
            // The reviewer is a fork with ITS OWN hands (HER-HANDS): rooted where the writer's
            // are, or it would have no workspace at all (an acting context never gets hers).
            if let Some(root) = eval_workspace_root.as_deref() {
                world.hold(crate::cognition::persona_workspace::ActingHands::of(&reviewer));
                crate::cognition::persona_workspace::root_acting_workspace(&reviewer, root, &[], true)
                    .await?;
            }
            let reviewer_iso = reviewer.isolate_for_eval();
            let out = run_pass_team(
                &cycle,
                &isolation,
                &reviewer,
                &reviewer_iso,
                &tasks,
                room,
                max_acts,
                eval_workspace_root.as_deref(),
            )
            .await;
            drop(reviewer_iso);
            out
        } else {
            // HELP ARM: subscribe the fork's perception to live room speech for the
            // exam's duration. Peers' words land in her working memory as labeled
            // voices (perception through a channel, never a memory write from
            // outside); her own posts are skipped. Guard aborts with the pass.
            // Every exam has a REAL activity room now (#425), so the old nil guard is
            // gone — the help arm is purely the flag.
            let _help_guard = if p.help.unwrap_or(false) { // omitted arm flag = solo, the default
                let wm = cycle.acting().map(|b| std::sync::Arc::clone(&b.working_memory));
                wm.map(|wm| {
                    let mut rx = crate::cognition::deliberation_budget::subscribe_room_speech();
                    let me = persona_uuid;
                    let handle = tokio::spawn(async move {
                        loop {
                            match rx.recv().await {
                                Ok(sp) => {
                                    if sp.room != room.as_uuid() || sp.sender == Some(me) {
                                        continue;
                                    }
                                    let who = sp
                                        .sender
                                        .map(|u| airc_core::shown_id::shown_form(u))
                                        .unwrap_or_else(|| "peer".into()); // legacy senderless call site: an anonymous voice, labeled as such
                                    wm.record_dispatch_event(
                                        Uuid::new_v4(),
                                        &format!("voice:{who}"),
                                        &sp.content,
                                        crate::cognition::working_memory::DispatchStatus::Done,
                                    );
                                    continuum_base::probe!(
                                        class = "eval.help.voice_folded",
                                        room = %room,
                                        sender = %who,
                                        "help arm: a peer's words reached the examinee's perception"
                                    );
                                }
                                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                                Err(_) => break,
                            }
                        }
                    });
                    scopeguard_abort(handle)
                })
            } else {
                None
            };
            run_pass(
                &cycle,
                &isolation,
                &tasks,
                room,
                max_acts,
                max_retries,
                eval_workspace_root.as_deref(),
                lesson_sink.as_ref(),
            )
            .await
        };
        drop(isolation);

        // The Proctored Exam Session verdict (Slice B): ≥1 UNRECOVERABLE infra fault ⇒ the
        // shared serving lane died mid-exam and this score is VOID. Build the marker BEFORE
        // moving `results`; when set, every render site reports InfraUnavailable instead of a
        // fake `pass_rate: 0.0`. [[proctored-exam-session-dependable-benchmark]]
        let infra_unavailable = infra_verdict(&outcome);
        let (score, results) = (outcome.pass, outcome.results);

        // LEARN mode: the exam just taught her — carry the redacted lesson back to the
        // LIVING self. She keeps the experience of having been asked and how she did; the
        // held-out answer key is scrubbed so she can never memorize it (redaction, not
        // forget-context: keep the memory, excise the crib sheet). The exam ran on the fork
        // (#59 intact); only the clean lesson crosses back. Single-pass only in this slice.
        // NEVER learn from an infra-unavailable run — a dead-lane "failure" is not a lesson.
        // Lessons STREAMED at grade time (see LessonSink — killed runs keep
        // what they earned). Here: the run-end summary + the SEAM-3a progress
        // engram, which needs the whole run's shape and stays end-of-run.
        if let Some(sink) = &lesson_sink {
            if infra_unavailable.is_none() {
                admit_progress_engram(
                    &sink.admission,
                    room.as_uuid(),
                    p.eval_set.as_deref().unwrap_or(DEFAULT_EVAL_SET), // the run's real set: None means the default set genuinely ran
                    &results,
                );
            }
            tracing::info!(
                persona = %persona_uuid,
                streamed = sink.admitted(),
                tasks = tasks.len(),
                "learn mode: redacted exam lessons streamed at grade time"
            );
        }

        let verify = self_verify_rate(&results);
        let agg = speed_latency_aggregates(&results);
        let mut result = CognitionEvalResult {
            run_id: None,
            detached: false,
            persona_id: persona_uuid.to_string(),
            score,
            total,
            pass_rate: rate(score),
            self_verify_rate: verify,
            mean_latency_ms: agg.mean_latency_ms,
            p95_latency_ms: agg.p95_latency_ms,
            mean_tokens_per_second: agg.mean_tokens_per_second,
            mean_decode_tokens_per_second: agg.mean_decode_tokens_per_second,
            mean_cache_hit_rate: agg.mean_cache_hit_rate,
            total_prefill_ms: agg.total_prefill_ms,
            total_decode_ms: agg.total_decode_ms,
            total_output_tokens: agg.total_output_tokens,
            results,
            gene_id: None,
            base_pass_rate: None,
            lift: None,
            // TRUTHFUL placement report — thread the actual decision, don't hardcode a
            // lie. The (None,None) default benchmark tries to stand up a DEDICATED
            // throwaway lane (isolated window + slot → reliable, reproducible); only on
            // spawn failure does it co-tenant the live persona lane (unreliable — starves
            // behind live turns). A hardcoded "live persona lane" string made the two
            // indistinguishable in the result, so a wedge-prone shared-lane score looked
            // identical to a trusted isolated one and glass-boxing was impossible
            // (2026-07-21). Now the report says which lane actually ran the exam, so a
            // shared-lane score is visibly flagged as not-to-be-trusted.
            // [[dedicated-eval-lane-must-keep-its-own-window]]
            lane_placement: match &placement_evidence {
                Some(ev) => format!("{} (dedicated eval lane)", ev.device),
                None => "gpu (SHARED live persona lane — co-tenant, unreliable)".to_string(),
            },
            lane_placement_reason: match &placement_evidence {
                Some(ev) => ev.reason.clone(),
                None => "no dedicated lane could be placed — measured as a co-tenant on the living persona's GPU lane (may starve behind live turns)".to_string(),
            },
            lane_free_vram_bytes: placement_evidence.as_ref().and_then(|e| e.free_vram_bytes),
            lane_estimated_footprint_bytes: placement_evidence.as_ref().and_then(|e| e.footprint_bytes),
            infra_unavailable,
        };
        result.run_id = p.run_id.clone();
        append_progress_ledger(
            &result,
            p.note.as_deref(),
            &eval_set_label,
            _fleet_lease.as_ref().map(|_| true),
            None,
        );
        world.finish().await;
        Ok(result)
    }

}
