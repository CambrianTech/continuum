//! The `cognition/eval` command itself: the detached fire-and-poll launch, and the eval body that
//! resolves the persona and her exam room, loads the tasks, provisions the exam's world, stands
//! up or shares a lane, forks the measurement copy, and runs either the gene A/B or the single
//! pass (solo, help arm, or team) before writing the run's ledger row.

use super::*;

#[derive(Default)]
pub struct CognitionEval;

#[async_trait]
impl ActionCommand for CognitionEval {
    const NAME: &'static str = "cognition/eval";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Put a persona through a test-graded coder gym using her LIVE cognition (same model, \
         faculties, tools). Pass persona_id (must be spawned); tasks come from inline `tasks`, an \
         `eval_set` JSONL path, or the default coder-eval set. Returns a pass-rate — the objective \
         number for whether a change made her a better coder.";
    type Params = CognitionEvalParams;
    type Output = CognitionEvalResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        p: CognitionEvalParams,
    ) -> Result<CognitionEvalResult, CommandError> {
        // Fire-and-poll (#86): a long ACTING eval runs many minutes — far past any IPC client
        // timeout — so `detach` spawns it on the runtime (the body owns its params and reaches
        // cognition via the global workspace registry, needing neither `self` nor `ctx`),
        // returns a job handle NOW, and lets the real result land in the progress ledger. The
        // run survives the client disconnecting; the caller polls
        // `~/.continuum/progress/<persona>.jsonl` by `note`.
        if p.detached() {
            let persona_id = p.persona_id.clone();
            let note = p.note.clone().unwrap_or_default(); // unwrap_or_default: an omitted note is the empty label, on the log and the failed row alike
            let run_id = p.run_id_or_mint();
            let mut inner = p.clone();
            inner.detach = Some(false);
            inner.run_id = Some(run_id.clone());
            // Own copies for the spawned closure so `persona_id`/`run_id` remain available
            // for the ack's return value below (they're `String`, not `Copy`).
            let ledger_persona = persona_id.clone();
            let ledger_run = run_id.clone();
            DETACHED_IN_FLIGHT.lock().push((
                run_id.clone(),
                persona_id.as_str().to_string(),
                inner.eval_set_reference().to_string(),
            ));
            tokio::spawn(async move {
                let outcome = CognitionEval::run_eval(inner).await;
                DETACHED_IN_FLIGHT.lock().retain(|(rid, _, _)| rid != &ledger_run);
                clear_eval_cancel(&ledger_run); // after the ledger row stamped `cancelled` — flag consumed, next run of this id starts clean
                match outcome {
                    Ok(r) => tracing::info!(
                        note = %note,
                        score = r.score,
                        total = r.total,
                        "cognition/eval detached run complete — result in progress ledger"
                    ),
                    Err(e) => {
                        tracing::error!(note = %note, error = %e, "cognition/eval detached run failed");
                        // Fail loud on the POLL SURFACE, not only the log. A detached run
                        // that dies before `append_progress_ledger` leaves eval-status
                        // returning `complete:false, row:null` forever — indistinguishable
                        // from "still starting", so the poller waits on a corpse (cost me
                        // two cycles staring at `total:null` while the real error — a
                        // post-reboot "no workspace template" fork race — sat in the log).
                        // Write a FAILED row keyed on the SAME run_id so the poller sees the
                        // error and can retry. [[fallbacks-are-illegal-fail-loud]]
                        append_failed_ledger(&ledger_persona, &ledger_run, &note, &e.to_string());
                    }
                }
            });
            return Ok(CognitionEvalResult {
                detached: true,
                persona_id: persona_id.to_string(),
                run_id: Some(run_id),
                ..Default::default()
            });
        }
        CognitionEval::run_eval(p).await
    }
}

impl CognitionEval {
    /// The eval body — deliberately ctx-free (reaches the persona's live cognition through
    /// the global workspace registry, owns its params), so it runs inline from `run` OR is
    /// spawned detached for fire-and-poll (#86). One code path, two launch modes: the test
    /// and prod paths stay identical [[validate-via-pure-rust-not-npm-jtag]].
    ///
    /// The fork acts with its own hands in its own acting context
    /// (`persona_workspace::give_fork_its_own_hands`, docs/architecture/HER-HANDS.md), so the
    /// eval never moves her live hands and there is nothing of hers to restore afterward (#312).
    async fn run_eval(mut p: CognitionEvalParams) -> Result<CognitionEvalResult, CommandError> {
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
        let _fleet_lease = match crate::persona::PersonaAircRuntimeRegistry::try_global() {
            Some(r) => {
                let lease = r.quiesce_all();
                crate::probe!(
                    class = "eval.quiesce",
                    personas = lease.count(),
                    "eval-preemption lease: fleet quiesced for the measurement"
                );
                Some(lease)
            }
            None => {
                crate::probe!(
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
            p.eval_set_reference().to_string(),
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
        let eval_set_label = if p.tasks.is_some() { "inline" } else { p.eval_set_reference() }.to_string();

        // Task source: inline → a gym reference resolved through `gym::resolve_gym`
        // (an existing on-disk file wins for a custom set; otherwise a committed gym
        // resolves from the embedded registry — CWD- and deployment-independent;
        // a typo'd / vanished gym FAILS LOUD naming every candidate, no silent
        // degrade) → when no `eval_set` is given, the committed default through the
        // SAME resolver. One JSONL line = one task. A malformed line FAILS LOUD with
        // its line number — never silently dropped. A vanished task would shrink the
        // gym and report a clean score over fewer tasks than intended: the same
        // invisible-degraded-mode as a fallback, which the resolver's fail-loud kills.
        let mut tasks: Vec<EvalTask> = if let Some(inline) = p.tasks.take() {
            inline
        } else {
            let (origin, text) = crate::cognition::gym::resolve_gym(p.eval_set_reference())
                .map_err(CommandError::Invalid)?;
            crate::cognition::gym::parse_tasks(&text, &origin).map_err(CommandError::Invalid)?
        };
        // Every code task grades hands. See `require_hands_for_code` — applied HERE, after both
        // load paths converge, so an inline task from a command payload obeys the same rule as a
        // committed gym line. There is no loader path to a mouth-graded code task.
        for t in tasks.iter_mut() {
            if p.help_arm() {
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
            match provision_ephemeral_eval_root(&p.run_id_or_mint()) {
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
        let suppress_recall = p.suppress_recall();

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
        let mut placement_evidence: Option<PlacementEvidence> = None;
        let fork = ForkSpec {
            persona: persona_uuid,
            needs_tools,
            workspace_root: eval_workspace_root.as_deref(),
            suppress_recall,
        };
        let cycle = match (&p.gene, p.base_model_id.as_deref()) {
            // A gene names its own forged base → ephemeral lane + the gene as `--lora`.
            (Some(gene), _) => {
                let el = spawn_gene_eval_lane(gene).await?;
                placement_evidence = Some(el.placement.clone());
                let cycle = fork_onto_lane(&fork, &el).await?;
                _eval_lane = Some(el); // holds the lane + its VRAM lease alive for the run
                cycle
            }
            // A bare base_model_id → ephemeral lane for THAT model, no gene. The clean
            // same-model control: measure the full loop on a chosen model in its own
            // throwaway server, living persona untouched (#59).
            (None, Some(base_id)) => {
                let el = spawn_base_eval_lane(base_id).await?;
                placement_evidence = Some(el.placement.clone());
                let cycle = fork_onto_lane(&fork, &el).await?;
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
                        let cycle = fork_onto_lane(&fork, &el).await?;
                        _eval_lane = Some(el); // holds the lane + its VRAM lease alive for the run
                        cycle
                    }
                    None => {
                        // A dedicated lane won't fit / isn't available → co-tenant SHARE on her
                        // live lane, held steady for the run (the historical behavior).
                        _exam_serving = Some(acquire_exam_serving_context().await);
                        fork_onto_live_lane(&fork).await?
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

        // WARM-GATE — never measure a COLD model (see `await_warm_cycle`).
        await_warm_cycle(&cycle).await?;

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
            crate::probe!(
                class = "eval.hands_rooted",
                root = %root,
                explicit = %p.workspace_root.is_some(),
                "eval fork's hands rooted where the grader reads"
            );
        }

        let max_acts = p.max_acts();
        let max_retries = p.max_retries();
        let total = tasks.len() as u32;

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
                scale: gene.scale.unwrap_or(1.0), // unwrap_or: an omitted scale applies the gene at full strength
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
            // A/B runs on its OWN EphemeralServingLane (gene/base copy), which
            // decode-verifies at spawn — a different failure surface than the shared
            // live lane. Threading the infra-fault classification through the two-arm
            // LIFT assembly is a scoped follow-up; the shared-lane single-pass (the
            // fake-zero incident) is what Slice B makes trustworthy first.
            let mut result = CognitionEvalResult::graded(
                GradedRun { persona_id: persona_uuid, total, score: gene_score, results: gene_results },
                placement_evidence.as_ref(),
                None,
            );
            result.gene_id = Some(gene.name.clone());
            result.base_pass_rate = Some(pass_rate(base_score, total));
            result.lift = Some(result.pass_rate - pass_rate(base_score, total));
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
        let outcome = if p.wants_team() {
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
            // HELP ARM (see `spawn_help_arm`); the guard aborts the subscriber with the pass.
            let _help_guard = if p.help_arm() { spawn_help_arm(&cycle, room, persona_uuid) } else { None };
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
                    p.eval_set_reference(),
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

        let mut result = CognitionEvalResult::graded(
            GradedRun { persona_id: persona_uuid, total, score, results },
            placement_evidence.as_ref(),
            infra_unavailable,
        );
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

// Stateless → self-register onto the ONE registry (descriptor + runtime object).
crate::register_stateless_command!(CognitionEval);
