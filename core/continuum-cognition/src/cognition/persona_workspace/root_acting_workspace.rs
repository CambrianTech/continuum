use super::*;

/// Root a forked cycle's file-engine (its `ToolExecutor`) at `root` by driving the
/// `code/create-workspace` act through her hands — the SAME mechanism `cognition/eval`
/// uses to point a measurement persona at a target repo, and now `agent/solve` uses to
/// point her at a benchmark sandbox. This is the counterpart to
/// [`repoint_workspace_map_if_pinned`]: that fixes what she *sees* (the workspace-map RAG
/// block); THIS moves where her hands *write*. Without it a forked persona writes to her
/// durable per-persona workspace (`<home>/citizens/peers/<id>/workspace/`) and the caller's
/// `git diff` on the sandbox scores a false ZERO — a lie about the solver
/// ([[fallbacks-are-illegal-fail-loud]]). Fails LOUD if the cycle has no hands or the
/// executor rejects the root; never silently no-ops.
///
/// Callers root an acting context they opened (a fork's own hands); her own hands are
/// never rooted here.
pub async fn root_acting_workspace(
    cycle: &WorkspaceCycle,
    root: &str,
    path_prepend: &[String],
    refuse_inert_edits: bool,
) -> Result<(), crate::sdk_codegen::CommandError> {
    let hands = ActingHands::of(cycle).ok_or_else(|| {
        crate::sdk_codegen::CommandError::Internal(
            "workspace_root requested but this cycle has no acting body (no hands) — cannot root \
             a workspace for a pure-cognition persona"
                .to_string(),
        )
    })?;
    // HER OWN HANDS NEVER MOVE (card fc9e9fd7): a measurement, solve or replay roots the
    // hands of the fork it opened, never hers.
    if hands.context.is_none() {
        return Err(crate::sdk_codegen::CommandError::Internal(
            "refusing to root her default hands: a measurement, solve or replay roots its own fork's hands".to_string(),
        ));
    }
    drive_create_workspace(
        &hands,
        root,
        path_prepend,
        "root-acting-workspace",
        refuse_inert_edits,
    )
    .await?;
    note_acting_root_in(hands.persona_id, hands.context, Some(std::path::PathBuf::from(root)));
    if let Some(body) = cycle.acting() {
        body.working_memory.set_scope(Some(root.to_string()));
    }
    continuum_base::probe!(
        class = "workspace.rooted",
        persona = %hands.persona_name,
        root = %root,
        "forked persona's file engine rooted at the target directory before her cycle"
    );
    Ok(())
}

/// Forget where an acting context stood: what a context's release on drop (the abnormal-exit
/// fallback) does once its release CONFIRMED, so no bookkeeping outlives it.
pub(crate) fn forget_acting_context(persona_id: Uuid, context: crate::identity::hands_context::HandsContext) {
    note_acting_root_in(persona_id, Some(context), None);
}

/// Why an acting context is NOT closed: its handles and place are kept for a retry.
#[derive(Debug)]
pub struct ContextNotClosed {
    pub message: String,
    /// The pids not confirmed gone, or `None` when the release couldn't report them.
    pub unconfirmed: Option<Vec<u32>>,
}

impl ContextNotClosed {
    pub(super) fn unknown(message: String) -> Self {
        Self { message, unconfirmed: None }
    }
}

impl std::fmt::Display for ContextNotClosed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl From<ContextNotClosed> for crate::sdk_codegen::CommandError {
    fn from(not_closed: ContextNotClosed) -> Self {
        crate::sdk_codegen::CommandError::Internal(not_closed.message)
    }
}

/// End an acting context and confirm it: `code/release-hands` through these hands (so it
/// ends THIS context and no other), awaited. Only a CONFIRMED release forgets where the
/// context stood; a release that couldn't confirm every kill keeps both the handles and the
/// record, and is returned as an error naming what's unresolved, so the caller keeps what
/// the context stood in and a retry finds it (Astra's review of #5005). This is the normal
/// teardown of an eval, a solve or a replay; the release sent when its hands drop is only
/// the fallback for an abnormal exit.
pub async fn close_acting_context(hands: &ActingHands) -> Result<(), ContextNotClosed> {
    let Some(context) = hands.context else {
        return Ok(()); // her live hands are never ended
    };
    // THE RELEASE IS THE KERNEL'S, run through the core as a RELEASE of these hands (never as
    // work admitted to them): it closes them to new work, waits out what is admitted, then
    // ends what still runs (Astra on #5026). Its typed result decides; nothing is clipped.
    let core = hands.executor.command_executor().ok_or_else(|| {
        ContextNotClosed::unknown(format!("closing acting context {}: these hands have no core to release through", context.id()))
    })?;
    let releasing = crate::routing::CallerIdentity::local_persona(crate::identity::PeerId::from_uuid(hands.persona_id))
        .in_hands(context)
        .releasing_hands();
    let release: crate::code::hands::ReleaseHandsResult = match core
        .execute_with_caller("code/release-hands", serde_json::json!({}), Some(releasing))
        .await
    {
        Ok(crate::runtime::CommandResult::Json(value)) => serde_json::from_value(value).map_err(|e| { // BOUNDARY: code/release-hands answers through the command surface's JSON result
            ContextNotClosed::unknown(format!("closing acting context {}: the release's result isn't its own shape ({e}), so nothing is confirmed", context.id()))
        })?,
        Ok(_) => return Err(ContextNotClosed::unknown(format!("closing acting context {}: the release answered in a shape other than its own JSON", context.id()))),
        Err(e) => return Err(ContextNotClosed::unknown(format!("closing acting context {}: {e}", context.id()))),
    };
    if !release.released {
        return Err(ContextNotClosed {
            message: format!(
                "acting context {} is NOT closed: pids {:?} not confirmed gone ({}); its handles and place are kept for a retry",
                context.id(),
                release.unconfirmed,
                release.errors.join("; ")
            ),
            unconfirmed: Some(release.unconfirmed),
        });
    }
    note_acting_root_in(hands.persona_id, hands.context, None);
    hands.working_memory.set_scope(None);
    continuum_base::probe!(
        class = "workspace.context_closed",
        persona = %hands.persona_name,
        context = %context.id(),
        "an acting context ended: its engine and shells released, its place forgotten"
    );
    Ok(())
}

impl PersonaWorkspaceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up a persona's mind. `None` if it hasn't been registered/built yet.
    pub fn get(&self, persona_id: &Uuid) -> Option<Arc<WorkspaceCycle>> {
        self.cycles
            .lock()
            .get(persona_id)
            .map(|entry| Arc::clone(&entry.cycle))
    }

    /// The cycle a turn of `persona_id` in `room` runs on: her mind cycle in her mind room,
    /// her live cycle everywhere else. THE selection the service loop asks (one rule, two
    /// call sites: a message turn and a self/continuation turn).
    pub fn cycle_for_room(&self, persona_id: &Uuid, room: Uuid) -> Option<Arc<WorkspaceCycle>> {
        if crate::cognition::mind_room::is_private_room(*persona_id, room) {
            self.mind_cycle(persona_id)
        } else {
            self.get(persona_id)
        }
    }

    /// Her mind cycle, built on first use from her template with its own working memory,
    /// no captures, and synchronous recall (a deferred recall worker would fold dispatch
    /// completions into it as well as her live memory). Before every use it mirrors her live
    /// cycle's genome, decoding and model binding, so a page-in or a re-home reaches her
    /// private turns at the next turn boundary. `None` when she has no live cycle or template.
    pub fn mind_cycle(&self, persona_id: &Uuid) -> Option<Arc<WorkspaceCycle>> {
        let live = self.get(persona_id)?;
        let cached = self.mind_cycles.lock().get(persona_id).map(|entry| Arc::clone(&entry.cycle));
        let mind = match cached {
            Some(mind) => mind,
            None => {
                let _assembly = self.assembly.lock();
                // Bound to its own statement: a guard in an `if let` scrutinee would live into
                // the `else` and deadlock on the insert below (non-reentrant lock).
                let built_meanwhile = self.mind_cycles.lock().get(persona_id).map(|entry| Arc::clone(&entry.cycle));
                if let Some(mind) = built_meanwhile {
                    mind
                } else {
                    let mut cfg = self.templates.lock().get(persona_id)?.clone();
                    cfg.defer_recall = false;
                    let (cycle, working_memory) = assemble_workspace_cycle(cfg, None, Capture::Withheld);
                    let cycle = Arc::new(cycle);
                    self.mind_cycles
                        .lock()
                        .insert(*persona_id, ResidentWorkspace { cycle: Arc::clone(&cycle), working_memory });
                    cycle
                }
            }
        };
        mind.mirror_mind_from(&live);
        Some(mind)
    }

    /// The personas that have a fork-template right now — i.e. the set
    /// `cognition/eval` can fork a measurement copy of. Keyed by the template map
    /// (populated by [`register_from_cfg`](Self::register_from_cfg) at spawn).
    pub fn template_ids(&self) -> Vec<Uuid> {
        self.templates.lock().keys().copied().collect()
    }

    /// THE ONE formal boundary (#396) that turns an operator-supplied persona
    /// reference — a full UUID, an 8-char short-id prefix, or a case-insensitive
    /// persona NAME — into the typed persona `Uuid`. Callers never sniff strings
    /// themselves; a name resolves INTO the id here (Joel's "come from name, but be
    /// more formal").
    ///
    /// A well-formed full UUID passes through unchanged, WITHOUT a membership check,
    /// so a persona whose template is still assembling (the post-reboot
    /// `register_from_cfg` race the fork wait absorbs) is never false-rejected —
    /// race safety lives in the caller's existing wait, not here. Short-id and name
    /// resolve against the forkable set that exists at call time. Fails LOUD naming
    /// the online personas — never a silent guess (the loose-`String` id boundary is
    /// exactly the defect class that fed a dead id to a doomed eval).
    /// The ONE door from a caller's [`PersonaRef`] to a real [`PeerId`]. Taking the
    /// newtype rather than `&str` is what makes resolution unskippable: a param that
    /// holds a reference cannot reach a subsystem that wants an identity without
    /// coming through here.
    pub fn resolve_persona(&self, reference: &PersonaRef) -> Result<PeerId, String> {
        let id_or_name = reference.as_str();
        // Snapshot (id, name) once, then drop the lock before resolving.
        let roster: Vec<(Uuid, String)> = {
            let templates = self.templates.lock();
            templates
                .iter()
                .map(|(id, cfg)| (*id, cfg.persona_name.clone()))
                .collect()
        };
        let ids: Vec<Uuid> = roster.iter().map(|(id, _)| *id).collect();

        // 1. Full UUID (race-safe passthrough) or short-id prefix against the
        //    forkable set — the shared id normalization primitive.
        if let Ok(id) = continuum_base::id_resolve::resolve(id_or_name, &ids, "persona") {
            return Ok(PeerId::from_uuid(id));
        }

        // 2. Case-insensitive persona NAME.
        let want = id_or_name.trim();
        let name_matches: Vec<Uuid> = roster
            .iter()
            .filter(|(_, name)| name.eq_ignore_ascii_case(want))
            .map(|(id, _)| *id)
            .collect();
        match name_matches.as_slice() {
            [one] => Ok(PeerId::from_uuid(*one)),
            [] => Err(format!(
                "no persona matches '{id_or_name}' (not a UUID, an 8-char short-id, or a name). {}",
                Self::roster_hint(&roster)
            )),
            many => Err(format!(
                "'{id_or_name}' is ambiguous — {} online personas share that name; pass a UUID. {}",
                many.len(),
                Self::roster_hint(&roster)
            )),
        }
    }

    /// "Online now: Asha (90e758b2), Atlas (e5f4141d)" — the actionable tail every
    /// resolution failure carries so an operator can fix the reference without
    /// grepping. Empty roster reads "no personas are online".
    pub(super) fn roster_hint(roster: &[(Uuid, String)]) -> String {
        if roster.is_empty() {
            return "No personas are online right now — call persona/instances/list.".to_string();
        }
        let mut listed: Vec<String> = roster
            .iter()
            .map(|(id, name)| format!("{name} ({})", airc_core::shown_id::shown_form(*id)))
            .collect();
        listed.sort();
        format!("Online now: {}.", listed.join(", "))
    }

    /// Build + register a persona's mind from its `cfg`, retaining a clone of the
    /// cfg as a fork-template so `cognition/eval` can later fork a measurement
    /// copy ([`fork_eval_cycle`](Self::fork_eval_cycle)). Overwrites any existing
    /// cycle + template: a persona can respawn in the same process (node
    /// resilience), and the fresh admission + adapter must replace the prior
    /// lifetime's. This is the production spawn path (see `supervisor.rs`).
    /// Checkpoint lock/read/schema failures refuse this registration and leave
    /// an existing resident unchanged. Call from a blocking worker during boot.
    pub fn register_from_cfg(
        &self,
        cfg: PersonaBrainConfig,
    ) -> std::io::Result<Arc<WorkspaceCycle>> {
        let _assembly = self.assembly.lock();
        let persona_id = cfg.persona_id;
        let template = cfg.clone();
        // Assembly restores disk state; do it outside the lookup lock.
        let restored = load_volatile(persona_id)?;
        let (cycle, working_memory) = assemble_workspace_cycle(cfg, restored, Capture::Recorded);
        // A resident keeps her learned tool head in her own store; a fork never does.
        if let Some(store) = template.admission.tool_heads() {
            working_memory
                .tool_head()
                .attach(crate::cognition::tool_head::entity::ToolHeadStore::new(Arc::clone(store), persona_id));
        }
        let cycle = Arc::new(cycle);
        // cycles THEN templates (the one canonical lock order).
        let mut cycles = self.cycles.lock();
        self.templates.lock().insert(persona_id, template);
        cycles.insert(
            persona_id,
            ResidentWorkspace {
                cycle: Arc::clone(&cycle),
                working_memory,
            },
        );
        Ok(cycle)
    }

    /// Get the persona's mind, building + caching it from `cfg` on first access.
    /// Lazy-init so a persona's cycle is assembled once and reused across every
    /// room it services (the "one soul" invariant). Also retains the fork-template
    /// (same as [`register_from_cfg`](Self::register_from_cfg)).
    pub fn get_or_build(&self, cfg: PersonaBrainConfig) -> std::io::Result<Arc<WorkspaceCycle>> {
        let persona_id = cfg.persona_id;
        if let Some(existing) = self.get(&persona_id) {
            return Ok(existing);
        }
        let _assembly = self.assembly.lock();
        if let Some(existing) = self.get(&persona_id) {
            return Ok(existing);
        }
        let template = cfg.clone();
        let restored = load_volatile(persona_id)?;
        let (cycle, working_memory) = assemble_workspace_cycle(cfg, restored, Capture::Recorded);
        // A resident keeps her learned tool head in her own store; a fork never does.
        if let Some(store) = template.admission.tool_heads() {
            working_memory
                .tool_head()
                .attach(crate::cognition::tool_head::entity::ToolHeadStore::new(Arc::clone(store), persona_id));
        }
        let cycle = Arc::new(cycle);
        // cycles THEN templates (the one canonical lock order).
        let mut cycles = self.cycles.lock();
        self.templates.lock().insert(persona_id, template);
        cycles.insert(
            persona_id,
            ResidentWorkspace {
                cycle: Arc::clone(&cycle),
                working_memory,
            },
        );
        Ok(cycle)
    }

    /// Periodic crash checkpoint. A busy writer coalesces this tick; no queue
    /// of blocking jobs accumulates behind slow storage.
    pub fn checkpoint_volatile_all(&self) -> Vec<(Uuid, std::io::Result<()>)> {
        let Some(stopped) = self.checkpoint_stopped.try_lock() else {
            return Vec::new();
        };
        if *stopped {
            return Vec::new();
        }
        self.write_volatile_all(SaveReason::Periodic)
    }

    /// Explicit save is repeatable and leaves periodic persistence running.
    pub fn flush_volatile_all(&self) -> Vec<(Uuid, std::io::Result<()>)> {
        let _checkpoint = self.checkpoint_stopped.lock();
        self.write_volatile_all(SaveReason::Seam)
    }

    /// Shutdown boundary: drain the admitted checkpoint, close periodic writes,
    /// then snapshot CURRENT residents. The caller must quiesce mutation first.
    /// Blocking work can outlive an async timeout; callers must receipt that as
    /// uncertain, never successful. No later periodic write can overtake this one.
    pub fn stop_volatile_checkpoints(&self) -> Vec<(Uuid, std::io::Result<()>)> {
        let mut stopped = self.checkpoint_stopped.lock();
        *stopped = true;
        self.write_volatile_all(SaveReason::Seam)
    }

    // Only called under checkpoint_stopped. The cycle lookup lock is released
    // before snapshot/serialization/IO, keeping room turns and roster reads free.
    pub(super) fn write_volatile_all(&self, reason: SaveReason) -> Vec<(Uuid, std::io::Result<()>)> {
        // ONE staged-read budget for the whole seam, shared by every resident, so the save
        // phase is bounded by the phase and not by resident count; a periodic write has none.
        let staged_deadline = matches!(reason, SaveReason::Seam)
            .then(|| std::time::Instant::now() + crate::cognition::handoff::STAGED_READ_BOUND);
        let residents: Vec<_> = self
            .cycles
            .lock()
            .iter()
            .map(|(id, entry)| (*id, Arc::clone(&entry.working_memory)))
            .collect();
        #[cfg(all(test, feature = "stress-tests"))]
        {
            let pause = self.checkpoint_pause.lock().take();
            if let Some(pause) = pause {
                assert!(pause.admitted.send(()).is_ok(), "test awaits admission");
                assert!(
                    pause
                        .resume
                        .recv_timeout(std::time::Duration::from_secs(30))
                        .is_ok(),
                    "test releases checkpoint"
                );
            }
        }
        let save_one = |id: Uuid, memory: Arc<WorkingMemory>| {
            let started = std::time::Instant::now();
            let result = save_volatile(id, reason, staged_deadline, &memory);
            continuum_base::probe!(
                class = "persona.volatile.checkpoint",
                persona_id = %id,
                outcome = if result.is_ok() { "ok" } else { "error" },
                ms = started.elapsed().as_millis() as u64,
                "resident volatile checkpoint completed"
            );
            (id, result)
        };
        match reason {
            // A periodic checkpoint reads no tree and writes one small file per resident:
            // sequential on the checkpoint worker, no threads per tick.
            SaveReason::Periodic => residents
                .into_iter()
                .map(|(id, memory)| save_one(id, memory))
                .collect(),
            // THE SEAM RUNS EVERY RESIDENT AT ONCE (card 1e4d8b3b). Each resident's record
            // reads her tree (two bounded git children); sequential under one shared
            // deadline meant the first cold `git status` spent the budget and every later
            // resident wrote `Unmeasured` or, past the save phase, only a Periodic record
            // (M5, first deploy seam 2026-09-26 16:37Z: five Deploy records, two Periodic).
            // Fork/join like the runtime's own save phase: the seam's wall time is the
            // slowest resident, never the sum, and one deadline is enough for all.
            SaveReason::Seam => {
                let started = std::time::Instant::now();
                let count = residents.len();
                let save_one = &save_one;
                // A test's native-home fixture is thread-local; each seam thread carries it
                // (a fixture not carried is a real home written from a test).
                #[cfg(test)]
                let test_home = crate::paths::NativeHomeOverride::current();
                #[cfg(test)]
                let test_home = &test_home;
                let results: Vec<(Uuid, std::io::Result<()>)> = std::thread::scope(|scope| {
                    let handles: Vec<_> = residents
                        .into_iter()
                        .map(|(id, memory)| {
                            let handle = scope.spawn(move || {
                                #[cfg(test)]
                                let _home = test_home
                                    .as_deref()
                                    .map(crate::paths::NativeHomeOverride::install);
                                save_one(id, memory)
                            });
                            (id, handle)
                        })
                        .collect();
                    handles
                        .into_iter()
                        .map(|(id, handle)| {
                            handle.join().unwrap_or_else(|_| { // unwrap_or_else: a resident's save thread panicked — her record is the one thing lost, named; the seam goes on
                                (
                                    id,
                                    Err(std::io::Error::other(
                                        "the resident's seam checkpoint thread panicked",
                                    )),
                                )
                            })
                        })
                        .collect()
                });
                continuum_base::probe!(
                    class = "persona.volatile.seam",
                    residents = count,
                    failed = results.iter().filter(|(_, r)| r.is_err()).count(),
                    ms = started.elapsed().as_millis() as u64,
                    staged_budget_ms =
                        crate::cognition::handoff::STAGED_READ_BOUND.as_millis() as u64,
                    "every resident's seam record was written concurrently under one \
                     phase-derived deadline"
                );
                results
            }
        }
    }

    /// Fork an EPHEMERAL measurement cycle for `cognition/eval`: a faithful copy
    /// of the persona's mind that takes the exam while the LIVING persona keeps
    /// living. Clones the retained fork-template, swaps `admission` for a fully
    /// detached copy ([`AdmissionState::fork_detached`]) — fresh metadata registry
    /// + NoopSink, so the eval's recall-hits, decay, and admissions touch NOTHING
    /// of hers — and rebuilds via [`build_workspace_cycle`], which gives the fork
    /// its OWN genome + decoding handles. So A/B paging and greedy decoding act on
    /// the copy; her live genome never flickers and her heartbeat keeps beating on
    /// the original, undisturbed. `None` if the persona has no retained template
    /// (never spawned through `register_from_cfg`/`get_or_build`) — the caller
    /// fails loud rather than measuring her live mind. See
    /// [[design-the-persona-as-a-being]] + [[eval-mutates-persona-lift-needs-isolation]].
    pub fn fork_eval_cycle(
        &self,
        persona_id: &Uuid,
        with_tools: bool,
        workspace_root: Option<&str>,
        suppress_recall: bool,
    ) -> Option<WorkspaceCycle> {
        let mut cfg = self.templates.lock().get(persona_id)?.clone();
        cfg.admission = Arc::new(cfg.admission.fork_detached());
        cfg.suppress_recall = suppress_recall;
        repoint_workspace_map_if_pinned(&mut cfg, persona_id, workspace_root);
        // The eval fork runs recall + grounding SYNCHRONOUSLY: drive_to_settle's
        // tight loop never yields to a background prefetch worker, so deferral would
        // measure a starved copy. Faithful eval = synchronous perception here.
        cfg.defer_recall = false;
        cfg.defer_grounding = false;
        // It acts beside her, so it never shares her page or waits on her act.
        cfg.forked = true;
        // Match the tool surface to the exam's grading MODALITY. A spoken-graded task
        // (`test`/`expect`, answer read from her mouth) needs NO hands — offering the
        // discovery-pair tool surface only lets a native-tool-call model loop on
        // `commands/help` and never SPEAK (Devstral 100%→0% through the loop; the
        // system-lift isolator's finding). Grade her mouth → don't hand her hands to
        // fumble; grade her hands (`solution_file`/`dod_shell`/`workspace_root`) → keep
        // them. This is exam hygiene, the same family as the greedy/directed controls —
        // [[adaptive-tool-surface-meets-you-in-the-middle]], [[eval-is-an-exam-not-a-life]].
        if !with_tools {
            cfg.tool_executor = None;
            // Capability consistency: grounding that DESCRIBES her hands
            // (workspace-map's tool paths + "drill in with code/list…") must not
            // be delivered into a hands-stripped cycle — a perception surface
            // never describes affordances that don't exist this cycle.
            cfg.grounding_sources.retain(|g| !g.requires_hands);
        } else {
            give_fork_its_own_hands(&mut cfg, *persona_id);
        }
        Some(build_workspace_cycle(cfg))
    }

    /// Like [`fork_eval_cycle`] but ROUTES the ephemeral copy's deliberation at a
    /// caller-supplied `adapter` instead of the live persona's. Everything else —
    /// admission snapshot (forked detached), grounding, recall, tools — is the
    /// persona's real cognition; only the model backend is swapped.
    ///
    /// The genome A/B (`cognition/eval`) uses this to point the measurement fork
    /// at an [`crate::inference::llama_server::EphemeralServingLane`] serving the
    /// gene's forged base, so a candidate is scored on a COPY, on its own base,
    /// without re-homing the model the living persona is currently thinking with
    /// (the humane-eval invariant, #59). The faculty's own `model` stays `None`,
    /// so the request omits the model id and the override adapter's default model
    /// (set via `with_default_model`) is authoritative.
    ///
    /// `context_window` is the window the override lane actually serves (`-c` per
    /// slot). It REPLACES the live persona's served window in the fork so the
    /// deliberation faculty budgets its prompt against what THIS lane serves —
    /// otherwise a fork carrying the 14B's larger window could build a prompt the
    /// ephemeral 4B lane can't hold and overflow it (the Asha-mute failure class).
    /// `extra_grounding` joins the persona's own grounding sources for THIS fork only —
    /// the seam a measurement drive uses to pin its task brief as `[mission]` standing
    /// framing (#390: the brief delivered once as a burst was evicted by act ~6 on a
    /// 24-act solve, and the persona literally asked what the issue was; a
    /// StandingFraming source cannot be evicted). Empty for plain evals.
    pub fn fork_eval_cycle_with_adapter(
        &self,
        persona_id: &Uuid,
        adapter: Arc<dyn AIProviderAdapter>,
        context_window: u32,
        with_tools: bool,
        workspace_root: Option<&str>,
        suppress_recall: bool,
        extra_grounding: Vec<GroundingSource>,
    ) -> Option<WorkspaceCycle> {
        let mut cfg = self.templates.lock().get(persona_id)?.clone();
        cfg.admission = Arc::new(cfg.admission.fork_detached());
        cfg.adapter = adapter;
        cfg.context_window = context_window;
        cfg.suppress_recall = suppress_recall;
        cfg.grounding_sources.extend(extra_grounding);
        repoint_workspace_map_if_pinned(&mut cfg, persona_id, workspace_root);
        // Synchronous perception on the eval copy (see `fork_eval_cycle`).
        cfg.defer_recall = false;
        cfg.defer_grounding = false;
        // It acts beside her, so it never shares her page or waits on her act.
        cfg.forked = true;
        // Speak-only for spoken-graded exams (see `fork_eval_cycle` for why).
        if !with_tools {
            cfg.tool_executor = None;
            // Capability consistency: grounding that DESCRIBES her hands
            // (workspace-map's tool paths + "drill in with code/list…") must not
            // be delivered into a hands-stripped cycle — a perception surface
            // never describes affordances that don't exist this cycle.
            cfg.grounding_sources.retain(|g| !g.requires_hands);
        } else {
            give_fork_its_own_hands(&mut cfg, *persona_id);
        }
        Some(build_workspace_cycle(cfg))
    }

    /// Enumerate the resident minds: `(persona_id, persona_name)` for every
    /// registered cycle. The name is read from the cycle's `ActingBody`
    /// (`None` for a pure-cognition persona with no hands). The seam the
    /// `cognition/personas` introspection command lists from — you can't score
    /// "every persona" without first discovering who has a live `WorkspaceCycle`.
    pub fn roster(&self) -> Vec<(Uuid, Option<String>)> {
        self.cycles
            .lock()
            .iter()
            .map(|(id, entry)| (*id, entry.cycle.acting().map(|b| b.persona_name.clone())))
            .collect()
    }

    /// The reflective handles for one persona's mind: its hippocampus + the
    /// inference adapter it currently deliberates through. The seam the
    /// `DreamConsolidationRegion`'s production `PersonaReflectionSource` reads —
    /// admission from the retained fork-template (the SAME `Arc` the live
    /// cycle's recall shares), adapter from the live cycle's model binding so a
    /// re-home is tracked (falling back to the template's spawn adapter for a
    /// pure-cognition cycle with no binding). Resolved per call, never cached —
    /// no parallel persona→adapter map ([[rag-as-persistent-cache]]).
    pub fn reflector_handles(
        &self,
        persona_id: &Uuid,
    ) -> Option<(
        Arc<AdmissionState>,
        Arc<dyn AIProviderAdapter>,
        Option<String>,
    )> {
        // Lock order contract: `cycles` THEN `templates` (see struct docs).
        // `get` takes + releases the cycles lock before we touch templates.
        let cycle = self.get(persona_id)?;
        let templates = self.templates.lock();
        let cfg = templates.get(persona_id)?;
        // Adapter AND served-model id from the live binding: a request without
        // the model id degenerated on the dream's first live pass (role-token
        // runaway) while turns — which send it — were clean.
        let (adapter, model) = cycle
            .current_model_route()
            .unwrap_or_else(|| (cfg.adapter.clone(), None));
        Some((Arc::clone(&cfg.admission), adapter, model))
    }

    /// How many persona minds are resident.
    pub fn len(&self) -> usize {
        self.cycles.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Process-global persona-workspace registry. One per process; persona minds are
/// assembled into it at spawn (`supervisor::materialize_adapters`) and resolved
/// from it by the `ai/should-respond` ServiceModule. Same pattern as
/// `ai::adapter::global_registry()` — the shared seam between the spawn
/// path that builds minds and the command path that runs them.
pub fn global() -> Arc<PersonaWorkspaceRegistry> {
    static GLOBAL: OnceLock<Arc<PersonaWorkspaceRegistry>> = OnceLock::new();
    GLOBAL
        .get_or_init(|| Arc::new(PersonaWorkspaceRegistry::new()))
        .clone()
}

// ── MEMENTO FIX (#138): volatile-tier persistence across deploy reboots ──
// "They wake up blank like Memento — an engineering failure; the flywheel
// falls apart" (Joel 2026-07-12; nine reboots that day = nine blank wakes).
// The persisted file is the SAME serialization grid-sync will ship — one
// format, one seam ([[persona-mind-persists-across-shutdowns]]).
/// On-disk shape: the working-memory snapshot plus the own-speech ring (the
/// ring is process-global in `deliberation_budget`, so it dies with the
/// process unless carried here — the repetition detectors were re-blinded by
/// every reboot until this).
#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct PersistedVolatile {
    pub(super) wm: super::super::working_memory::VolatileSnapshot,
    pub(super) own_speech: OwnSpeechPersisted,
}
