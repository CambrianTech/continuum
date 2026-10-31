use super::*;

impl WorkspaceCycle {
    pub fn new(faculties: Vec<Arc<dyn Faculty>>, arbiter: Arc<dyn Arbiter>) -> Self {
        Self {
            faculties,
            arbiter,
            capture: Arc::new(NoopWorkspaceCaptureSink),
            acting: None,
            genome: empty_genome(),
            decoding: relaxed_decoding(),
            model_binding: None,
            experience: None,
            cycle_counter: std::sync::atomic::AtomicU64::new(0),
            token_sink: std::sync::Mutex::new(None),
            faculty_pulse: Arc::new(super::super::faculty_pulse::FacultyPulse::new()),
            kv_cached_tokens: std::sync::atomic::AtomicU64::new(0),
            kv_prefill_tokens: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// The live cognition-compass accumulator (#186). The tick seam + acting seam bump
    /// it; the vitals radiator samples [`FacultyPulse::levels`]. `Arc` clone is cheap.
    /// #266 Fold ONE generation's prefill accounting into this persona's lifetime
    /// KV totals, and emit the per-generation probe. Called once per live turn from
    /// the settle seam — the single place a completed generation's [`TurnMetrics`]
    /// is known — so there is exactly one writer and no parallel accumulator.
    ///
    /// A generation the lane reported no timings for (cloud provider, older
    /// endpoint) folds in as 0/0 and moves neither counter: absent stays absent
    /// rather than being counted as a miss, which would slander a lane that simply
    /// never told us.
    pub fn note_generation(&self, m: &TurnMetrics) {
        use std::sync::atomic::Ordering;
        if m.cached_tokens == 0 && m.prefill_tokens == 0 {
            return; // no timings reported — an absence, never a 0% datum
        }
        let cached = self
            .kv_cached_tokens
            .fetch_add(u64::from(m.cached_tokens), Ordering::Relaxed)
            + u64::from(m.cached_tokens);
        let prefill = self
            .kv_prefill_tokens
            .fetch_add(u64::from(m.prefill_tokens), Ordering::Relaxed)
            + u64::from(m.prefill_tokens);
        let lifetime = cached + prefill;
        // The hour's prefix reuse, on the citizen-health line (card c119ace7): the same
        // two numbers, folded once here — the one writer — so the receipt a human reads
        // in the org room carries "prefix reuse NN%" without a probe query.
        crate::cognition::health_ledger::note_generation(m.cached_tokens, m.prefill_tokens);
        continuum_base::probe!(
            class = "serving.kv.reuse",
            turn_cached = m.cached_tokens,
            turn_prefill = m.prefill_tokens,
            turn_rate = m.cache_hit_rate(),
            lifetime_cached = cached,
            lifetime_prefill = prefill,
            lifetime_rate = if lifetime == 0 {
                0.0
            } else {
                cached as f64 / lifetime as f64
            },
            prefill_ms = m.prefill_ms,
            decode_ms = m.decode_ms,
            "KV reuse: {} of {} prompt tokens served from cache this generation",
            m.cached_tokens,
            m.cached_tokens.saturating_add(m.prefill_tokens)
        );
    }

    /// #266 This persona's lifetime KV-prefix reuse — `cached / (cached + prefilled)`
    /// across every generation she has run. `None` until at least one generation
    /// reported timings, so a citizen who has not yet spoken radiates NOTHING rather
    /// than a fabricated 0% (the honest-empty rule the vitals meters already follow
    /// for a dark cognition axis and an un-paged genome).
    ///
    /// The ratio is derived from totals on every read, never stored and never
    /// averaged across turns — averaging rates lies, the same reason
    /// [`TurnMetrics::accumulate`] re-derives tok/s from totals.
    pub fn kv_reuse(&self) -> Option<f64> {
        use std::sync::atomic::Ordering;
        let cached = self.kv_cached_tokens.load(Ordering::Relaxed);
        let prefill = self.kv_prefill_tokens.load(Ordering::Relaxed);
        let total = cached + prefill;
        (total > 0).then(|| cached as f64 / total as f64)
    }

    pub fn faculty_pulse(&self) -> Arc<super::super::faculty_pulse::FacultyPulse> {
        self.faculty_pulse.clone()
    }

    /// #186 glass-box tap: record that a faculty CONTRIBUTED this tick on the live
    /// compass, brightness scaled by its salience (a strong recall lights brighter than
    /// a weak one) but floored so any real firing is visibly lit. Maps the internal
    /// faculty → its display axis; a faculty with no compass home (Affect/Volition/
    /// Salience) is a silent no-op. PURE OBSERVABILITY — called from the tick after a
    /// bid, never reads back into a decision.
    pub(super) fn note_faculty_firing(&self, faculty: &FacultyId, salience: f32) {
        if let Some(axis) = super::super::faculty_pulse::CognitionAxis::of(faculty) {
            // 35..=100: a bid always lights (≥35), salience carries the rest.
            let level = (35.0 + salience.clamp(0.0, 1.0) * 65.0).round() as u8;
            self.faculty_pulse.note(axis, level);
        }
    }

    /// #186 glass-box: fire the Act axis on the live compass — the hands moving, a tool
    /// actually executing. Called from the acting seam ([`super::act_observe::apply_act`]);
    /// Act has no `FacultyId` (the hands run AFTER deliberation, not as a workspace
    /// faculty), so it is bumped explicitly rather than through the tick map.
    pub fn note_acting(&self) {
        self.faculty_pulse
            .fire(super::super::faculty_pulse::CognitionAxis::Act);
    }

    /// Share the persona's decoding handle — call with the SAME [`DecodingHandle`]
    /// passed to [`LlmDeliberationFaculty::with_decoding`] so the eval window's
    /// greedy flip takes effect on the faculty's next generation.
    pub fn with_decoding(mut self, decoding: DecodingHandle) -> Self {
        self.decoding = decoding;
        self
    }

    /// Share the genome handle the deliberation faculty reads — call with the SAME
    /// [`GenomeHandle`] passed to [`LlmDeliberationFaculty::with_genome`] so a
    /// page-in on the cycle takes effect on the faculty's next generation.
    pub fn with_genome(mut self, genome: GenomeHandle) -> Self {
        self.genome = genome;
        self
    }

    /// Share the model binding the deliberation faculty reads — call with the SAME
    /// [`ModelBindingHandle`] passed to
    /// [`LlmDeliberationFaculty::with_model_binding`](super::llm_deliberation_faculty::LlmDeliberationFaculty::with_model_binding)
    /// so a [`rebind_model`](Self::rebind_model) on the cycle takes effect on the
    /// faculty's next generation. Without this call the cycle has no binding to
    /// re-home (a pure-cognition / test cycle), and `rebind_model` is a no-op.
    pub fn with_model_binding(mut self, binding: ModelBindingHandle) -> Self {
        self.model_binding = Some(binding);
        self
    }

    /// Share the node's experience source so each tick can read the room's recipe
    /// and stamp its affordances onto the turn (S1). `None` keeps the pre-S1
    /// behaviour: no stamp, global surface.
    pub fn with_experience_source(
        mut self,
        experience: Option<crate::experience::source::SharedExperienceSource>,
    ) -> Self {
        self.experience = experience;
        self
    }

    /// The command names the room's recipe authorizes — `affordances[].command` of
    /// the resolved manifest. Empty when no source is shared in, when the source has
    /// no recipe for the room (never a fabricated manifest), or when the recipe
    /// declares none: all three mean "the surface is not room-selected this turn".
    pub(super) fn room_affordances_for(&self, room: Uuid) -> Vec<String> {
        self.experience
            .as_ref()
            .and_then(|source| source.experience_for(room))
            .map(|manifest| manifest.affordances.into_iter().map(|a| a.command).collect())
            .unwrap_or_default()
    }

    /// Re-home the persona's deliberation onto a newly served model — swap the
    /// {adapter, model, context window} triple ATOMICALLY into the shared binding
    /// the faculty reads. The genome, working memory, admission, and hippocampus are
    /// UNTOUCHED (no cycle rebuild): the SAME continuous mind now deliberates through
    /// the new model, so this is model-load-as-paging (frequent, on grid demand —
    /// the base-model sibling of [`page_in`](Self::page_in)'s LoRA paging), not a
    /// teardown. Wait-free swap; the faculty's next generation sees it. No-op for a
    /// cycle with no deliberation faculty (no binding was shared in). See
    /// [`ModelBinding`] + [[seamless-persona-failover-model-and-genome]].
    pub fn rebind_model(&self, binding: ModelBinding) {
        if let Some(handle) = &self.model_binding {
            handle.store(Arc::new(binding));
        }
    }

    /// #169: set the per-turn STREAMING sink for the NEXT turn(s). The live caller
    /// (service_loop) sets `Some(tx)` just before a streamed turn and `None` after,
    /// so `run_in_room_inner` reads it into that turn's [`Workspace::token_sink`].
    /// `&self` (interior-mutable, like [`rebind_model`](Self::rebind_model)) — the
    /// living mind is shared, not owned per tick.
    pub fn set_token_sink(
        &self,
        sink: Option<tokio::sync::mpsc::UnboundedSender<crate::ai::adapter::GenerationChunk>>,
    ) {
        *self.token_sink.lock().unwrap() = sink;
    }

    /// The per-turn streaming sink to hand this tick's Workspace, or `None`.
    pub(super) fn current_token_sink(
        &self,
    ) -> Option<tokio::sync::mpsc::UnboundedSender<crate::ai::adapter::GenerationChunk>> {
        self.token_sink.lock().unwrap().clone()
    }

    /// The inference adapter this mind CURRENTLY deliberates through — read from
    /// the same shared binding [`rebind_model`](Self::rebind_model) writes, so it
    /// tracks re-homes. `None` for a cycle with no deliberation faculty. The seam
    /// background regions (dream consolidation, wanderers) resolve their adapter
    /// from, instead of a spawn-time snapshot that would go stale on failover.
    pub fn current_adapter(&self) -> Option<Arc<dyn crate::ai::adapter::AIProviderAdapter>> {
        self.model_binding
            .as_ref()
            .map(|handle| handle.load().adapter.clone())
    }

    /// The whole live binding {adapter, model, window} as ONE read — what the re-home
    /// owner compares against the served snapshot. `None` for a cycle with no
    /// deliberation faculty.
    pub fn current_binding(&self) -> Option<Arc<ModelBinding>> {
        self.model_binding.as_ref().map(|handle| handle.load_full())
    }

    /// The adapter AND the served-model id it must be asked for, as one read of
    /// the shared binding. Background regions must send BOTH: the model id
    /// selects the route/template on the serving side, and omitting it produced
    /// degenerate role-token runaway on the dream's first live pass
    /// (2026-07-12) while normal turns — which send `binding.model` — were
    /// clean through the same adapter.
    pub fn current_model_route(
        &self,
    ) -> Option<(
        Arc<dyn crate::ai::adapter::AIProviderAdapter>,
        Option<String>,
    )> {
        self.model_binding.as_ref().map(|handle| {
            let b = handle.load();
            (b.adapter.clone(), b.model.clone())
        })
    }

    /// The served-model id + EFFECTIVE context window, as ONE atomic read of the
    /// shared binding — the display inputs a roster LOADOUT projects (`model ·
    /// ctx`). `None` for a cycle with no deliberation faculty (no binding to
    /// report). The parameter COUNT is deliberately NOT here: it isn't on the
    /// binding, and the caller resolves it from the model registry by this id
    /// (honest-absent when the row is unhydrated) — the cognition layer must not
    /// take a `model_registry` dependency to hand a display fact to the projector.
    /// Reading both in one `load()` avoids a torn `(model, window)` across a
    /// concurrent [`rebind_model`](Self::rebind_model).
    ///
    /// The model id reported is the **EFFECTIVE** one: the binding's explicit id
    /// when set, else the adapter's own `default_model()` — which is exactly the
    /// id the next generation asks for (`ModelBinding.model` doc: `None` → the
    /// adapter's default). This resolution matters live: the boot upstart binds
    /// `model: None` (the shared served adapter already carries the active model
    /// as its default), and a binding stored without an explicit id keeps it
    /// `None` until a re-home names one. Reporting the raw field left every
    /// live roster LOADOUT as "ctx only, no model name" (the tile regression this
    /// fixes). An adapter with an empty default resolves to honest-absent.
    pub fn model_loadout(&self) -> Option<(Option<String>, u32)> {
        self.model_binding.as_ref().map(|handle| {
            let b = handle.load();
            let model = b.model.clone().or_else(|| {
                let default = b.adapter.default_model();
                (!default.is_empty()).then(|| default.to_string())
            });
            (model, b.context_window)
        })
    }

    /// Page a gene (set of LoRA layers) into the persona's genome — the next
    /// generation runs the base model adapted by these layers. Wait-free swap.
    /// This is the measured page-in: the genome loop pages in a freshly forged
    /// gene here and `cognition/eval` measures the lift it produced.
    pub fn page_in(&self, adapters: Vec<ActiveAdapterRequest>) {
        self.genome.store(Arc::new(adapters));
    }

    /// Page out all genes — the persona reverts to the base model (no LoRA). The
    /// baseline arm of an A/B, and the clean state to leave a persona in.
    pub fn page_out(&self) {
        self.genome.store(Arc::new(Vec::new()));
    }

    /// The persona's currently paged-in genome (a snapshot).
    /// Mirror `live`'s genome, decoding and model binding into THIS cycle's own handles, so
    /// her mind cycle (`PersonaWorkspaceRegistry::mind_cycle`) thinks with the same genes on
    /// the same served model as her live cycle. Values are copied, not the handles: the two
    /// cycles keep separate working memories and never share a buffer.
    pub(crate) fn mirror_mind_from(&self, live: &WorkspaceCycle) {
        self.genome.store(live.genome.load_full());
        self.decoding.store(live.decoding.load_full());
        if let (Some(mine), Some(theirs)) = (&self.model_binding, &live.model_binding) {
            mine.store(theirs.load_full());
        }
    }

    pub fn genome(&self) -> Vec<ActiveAdapterRequest> {
        self.genome.load().as_ref().clone()
    }

    /// This mind's monotonic service-tick count — how many `Workspace::cycle`
    /// ticks it has serviced since spawn. The DELTA of this over an interval is
    /// the persona's live "thinking tempo" (the roster **ACT** vital): a rising
    /// count = actively servicing concerns, a flat count = idle. Read wait-free.
    pub fn cycle_count(&self) -> u64 {
        self.cycle_counter
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Clear the volatile working-memory scratch (the act/reasoning proprioception
    /// buffer), if this cycle has hands. Called at the boundary between disjoint
    /// concerns — e.g. each independent task in a `cognition/eval` pass — so one
    /// concern's proprioception does not bleed into the next. No-op for a
    /// pure-cognition cycle. See [`super::working_memory::WorkingMemory::clear`].
    pub fn reset_working_memory(&self) {
        if let Some(acting) = &self.acting {
            acting.working_memory.clear();
        }
    }

    /// How many acts these hands have executed, EVER — the monotonic counter, not
    /// the capacity-bounded receipt ring ([`super::working_memory::WorkingMemory::actions_taken`]).
    /// `None` for a pure-cognition cycle (no hands, so the question has no answer —
    /// distinct from `Some(0)`, which is hands that have not acted yet).
    ///
    /// Exists so a LONG-RUNNING drive can report its own liveness WHILE it runs.
    /// `drive_to_settle` returns its act count only at settlement, and a benchmark
    /// attempt legitimately runs for hours — so every liveness surface downstream
    /// (`benchmark/runs`, the bench board, the run-room panel) was reading a number
    /// that could not move until the work was already over. Measured 2026-08-16:
    /// two dispatched solves read `acts=0, stalled=false` for ten straight minutes
    /// while their citizens were mid-turn, and the projection whose stated purpose
    /// is "silence must never be ambiguous with progress" could not tell the two
    /// apart. A wait-free atomic load, safe to poll on a heartbeat.
    pub fn actions_taken(&self) -> Option<u64> {
        self.acting
            .as_ref()
            .map(|a| a.working_memory.actions_taken())
    }

    /// Begin a memory-isolated measurement window over this cycle's hippocampus.
    ///
    /// `cognition/eval` drives the persona's REAL admission as it grades her, so
    /// the act-observations a run admits would otherwise (1) drift her absolute
    /// score run-to-run, (2) order-bias a paired A/B (the second arm inherits the
    /// first arm's writes), and (3) pollute her durable sqlite. While the returned
    /// guard is alive, admission STILL fires — the measured memory motion is
    /// identical to a real turn, which is what keeps the measurement valid — but
    /// the persistence sink is muted (nothing reaches disk) and the in-memory
    /// admission frame is checkpointed. Call [`EvalIsolation::rewind`] between A/B
    /// arms so both start from identical memory; dropping the guard restores the
    /// memory and the real sink. A pure-cognition cycle (no hands → nothing is
    /// admitted) yields a no-op guard. See
    /// [[eval-mutates-persona-lift-needs-isolation]].
    pub fn isolate_for_eval(&self) -> EvalIsolation {
        self.isolate_for_eval_at(Some(0.0))
    }

    /// [`Self::isolate_for_eval`] with an explicit decoding pin — the
    /// temperature A/B seam (2026-08-23). Greedy (0.0) buys byte-reproducible
    /// rewards but measured pathological on long-context agentic work: think
    /// emissions spiraled 6.9k → 16,384 tokens (the reply ceiling, hit
    /// exactly) at 4+ min decode per act, while every SOTA agentic harness
    /// samples ~0.6-0.8 for exactly this reason. `None` = her lived
    /// temperature. Behavior-before-perplexity applied to our own harness:
    /// the pin is a MEASURED choice per run, not an axiom.
    pub fn isolate_for_eval_at(&self, decoding: Option<f32>) -> EvalIsolation {
        // Force greedy decoding for the WHOLE eval window — before the no-hands
        // early return, because a reproducible metric needs deterministic
        // generation even for a pure-speak (no-tools) eval. Restored on drop.
        self.decoding.store(Arc::new(decoding));
        let decoding = Some(Arc::clone(&self.decoding));

        let Some(acting) = &self.acting else {
            return EvalIsolation {
                admission: None,
                checkpoint: None,
                real_sink: None,
                decoding,
            };
        };
        let admission = acting.admission.clone();
        let checkpoint = admission.checkpoint();
        let real_sink =
            admission.swap_persistence(crate::cognition::admission_persistence::NoopSink::arc());
        EvalIsolation {
            admission: Some(admission),
            checkpoint: Some(checkpoint),
            real_sink: Some(real_sink),
            decoding,
        }
    }

    /// The faculty ids composed into this cycle, in order — the observable a test uses to
    /// assert a faculty was included or (as with #207's recall suppression) deliberately
    /// omitted. Test-only: production cognition never introspects its own faculty roster.
    #[cfg(test)]
    pub(crate) fn faculty_ids(&self) -> Vec<FacultyId> {
        self.faculties.iter().map(|f| f.id()).collect()
    }

    /// Install a capture sink (recording / on-disk replay / in-flight inspection).
    pub fn with_capture(mut self, capture: Arc<dyn WorkspaceCaptureSink>) -> Self {
        self.capture = capture;
        self
    }

    /// Give this mind a body — the hands + hippocampus + identity the act→observe
    /// driver uses to execute an [`Decision::Act`], remember its result, and
    /// re-perceive. Without it the cycle is pure cognition (no acting). See
    /// [`ActingBody`].
    pub fn with_acting(mut self, body: Arc<ActingBody>) -> Self {
        self.acting = Some(body);
        self
    }

    /// The persona's body, if it has hands. The act→observe driver reads this;
    /// `None` → pure-cognition mind (no tools were offered, so no `Act` arises).
    pub fn acting(&self) -> Option<&Arc<ActingBody>> {
        self.acting.as_ref()
    }

    /// Run one cognition tick over the consolidated `world_state`, recording the
    /// full trace (every bid incl. losers, what won, the decision) for replay.
    ///
    /// **Staged assembly** (cbar lines→planes / GWT broadcast-then-rebid):
    /// 1. **Perception phase** — faculties with `reacts_to_broadcast() == false`
    ///    bid in parallel over the raw world-state (broadcast still empty). The
    ///    arbiter routes the salient subset into the broadcast: this is the
    ///    *assembled context* (the "RAG" the decider will read).
    /// 2. **Deliberation phase** — faculties with `reacts_to_broadcast() == true`
    ///    bid in parallel over the workspace whose broadcast now holds the
    ///    assembled context, so the [`Decision`] is conditioned on what recall /
    ///    world-model / affect actually surfaced. Their bids append to the
    ///    broadcast.
    ///
    /// This is what makes "pull relevant memory, *then* decide" real: the decider
    /// is never blind to recall. Still one tick over the consolidated burst, still
    /// `O(bids)` for the context — no per-event slowdown.
    /// TEST-ONLY ambient shorthand: the burst names its room (a raw string
    /// converts with [`ActivityRoom::test_room`](crate::identity::ActivityRoom::test_room)
    /// under `#[cfg(test)]`), so even faculty-isolation ticks run in a
    /// real-shaped room. Production has no roomless ticks (#425) — live callers
    /// use [`run_framed`](Self::run_framed) / [`run_situated`](Self::run_situated).
    #[cfg(any(test, feature = "test-fixtures"))]
    pub async fn run(&self, burst: impl Into<Burst>) -> Workspace {
        self.run_inner(
            burst.into(),
            TurnFraming::ambient(),
            Situation::FreshContext,
        )
        .await
    }

    /// TEST-ONLY compat: ambient tick with an explicit room override.
    #[cfg(test)]
    pub async fn run_in_room(&self, burst: impl Into<Burst>, room_id: Uuid) -> Workspace {
        let mut b: Burst = burst.into();
        if let Ok(r) = crate::identity::ActivityRoom::from_uuid(room_id) {
            b.room = r;
        }
        self.run_inner(b, TurnFraming::ambient(), Situation::FreshContext)
            .await
    }

    /// The live persona path passes `attention`/`self_initiated` here so the
    /// deliberation faculty's system prompt reflects whether a question was put
    /// TO her (suppress the silence escape) and whether this is her own
    /// heartbeat. The room rides ON the burst — one statement of the activity,
    /// no separate parameter to drift from the rendered header (#425).
    pub async fn run_framed(&self, burst: Burst, framing: TurnFraming) -> Workspace {
        // Fresh-context default: a bare framed tick is a fresh ask (fuller
        // grounding). The act→observe driver calls [`run_situated`] with
        // `PostAction` on re-perception ticks.
        self.run_inner(burst, framing, Situation::FreshContext)
            .await
    }

    /// Same as [`run_framed`](Self::run_framed) but with the tick's [`Situation`]
    /// threaded explicitly — the seam the act→observe loop uses to tell the focuser
    /// "this tick re-perceives a tool result" (`PostAction`) vs "fresh ask"
    /// (`FreshContext`). The situation is a TYPED signal carried from the loop,
    /// never inferred from the burst text ([[no-hardcoded-heuristics-to-steer-
    /// cognition]]).
    pub async fn run_situated(
        &self,
        burst: Burst,
        framing: TurnFraming,
        situation: Situation,
    ) -> Workspace {
        self.run_inner(burst, framing, situation).await
    }
}
