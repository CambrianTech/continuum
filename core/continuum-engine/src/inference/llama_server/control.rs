//! Control responsibility of the llama-server process owner.
use super::*;
/// Typed serving-control failures. The only string is a display-only leaf
/// detail — never parsed back into control flow ([[protocols-prevent-pain]]).
#[derive(Debug, thiserror::Error)]
pub enum LlamaServerError {
    #[error("serving lifecycle request was superseded before admission")]
    Superseded,
    /// The server isn't answering — not yet spawned, or down. The reconcile
    /// loop treats this as "no active model" and (re)spawns, rather than failing.
    #[error("llama-server unreachable: {0}")]
    Unreachable(String),
    /// Spawn itself failed (binary missing, bad args, OS refused). Loud.
    #[error("llama-server spawn failed: {0}")]
    Spawn(String),
    /// The model GGUF could not be resolved on disk for the requested id.
    #[error("no local GGUF for model '{0}'")]
    ModelNotFound(String),
    /// The server spawned but never became ready within the timeout.
    #[error("llama-server not ready after {0:?}: {1}")]
    NotReady(Duration, String),
    /// A target named a LoRA genome whose GGUF is not on disk. Loud — we never
    /// silently skip a missing `--lora` (that would serve a different genome than
    /// the plan asked for, the wrong kind of degrade)
    /// ([[fallbacks-are-illegal-fail-loud]]).
    #[error("LoRA adapter file not found: {0}")]
    AdapterNotFound(String),
    /// The incumbent engine could not be retired safely, so it was not signalled and still
    /// runs: its residents' checkpoint custody refused, or its identity could not be verified
    /// (card 931cfd22). Never a spawn failure: it neither condemns the incumbent nor rolls the
    /// engine binary back.
    #[error("retiring the incumbent engine was refused, nothing was signalled: {0}")]
    RetirementRefused(String),
    /// Retirement has been signalled but exact-process exit is not yet proven.
    /// Keep admission suspended and resume observation; never blame the candidate binary.
    #[error("incumbent retirement remains unverified; endpoint suspended: {0}")]
    RetirementPending(String),
}


/// Outcome of one reconcile pass. The daemon logs/publishes against this; it is
/// exhaustive so a new state can't be silently dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnsureOutcome {
    /// Guarded request lost its intent before lifecycle admission; no mutation.
    Superseded,
    /// The desired model was already being served — no relaunch.
    AlreadyServing,
    /// We (re)spawned and the server became ready serving `model`.
    Spawned { model: String },
    /// We could not bring the desired model up; `reason` says why (display-only).
    Degraded { reason: String },
}


/// Opaque identity of an owned engine, captured before requesting retirement.
/// It cannot authorize stopping an adopted process or a replacement generation.
#[derive(Clone)]
pub struct OwnedEngineIdentity {
    pub(super) owner: std::sync::Weak<StdMutex<Option<OwnedEngine>>>,
    pub(super) generation: crate::inference::slots::EngineGeneration,
}


impl PartialEq for OwnedEngineIdentity {
    fn eq(&self, other: &Self) -> bool {
        self.owner.ptr_eq(&other.owner) && self.generation.same_engine(&other.generation)
    }
}

impl Eq for OwnedEngineIdentity {}


/// Original target and observed geometry recorded only after local launch readiness.
/// This is launch provenance, not a fresh health check or resource reservation.
#[derive(Clone)]
pub struct OwnedServingTarget {
    // Original application/model input evidence stays private and optional.
    pub(super) prepared: Option<Arc<PreparedLocalLaunch>>,
    pub identity: OwnedEngineIdentity,
    pub target: ServingTarget,
    /// Selected program spelling; a PATH-only name is not an immutable binary identity.
    pub engine_program: String,
    /// Public diagnostic projection of explicit arguments and managed environment.
    /// Only the private original prepared receipt can authorize exact replay.
    pub invocation: crate::inference::lane_args::LaneInvocation,
    pub observed_context_window: u32,
    pub observed_lanes: u32,
    pub launched_host_prompt_cache_mib: u32,
}


/// Checked at the endpoint's drained lifecycle-admission boundary. Once admitted,
/// the operation may settle even if newer intent arrives; its result keeps its epoch.
pub type ServingLifecycleCheck<'a> = dyn Fn() -> bool + Send + Sync + 'a;


/// Evidence handle, not a capacity lease. Dropping it does not abandon the child.
#[derive(Clone)]
pub struct EngineRetirementReceipt {
    pub(super) identity: OwnedEngineIdentity,
}


/// Retry authority minted from one original owned launch. Only the controller's
/// shared spawn path can advance the expected generation; callers cannot relabel
/// an ordinary or foreign successor as part of this restoration.
pub struct OwnedRestoreSession {
    pub(super) original: OwnedServingTarget,
    pub(super) expected: parking_lot::Mutex<crate::inference::slots::EngineGeneration>,
    pub(super) _pages: PageDirReservation,
    pub(super) suspension: parking_lot::Mutex<Option<crate::inference::slots::EndpointSuspension>>,
}


impl OwnedRestoreSession {
    pub fn was_suspended(&self) -> bool {
        self.suspension.lock().is_some()
    }

    pub fn attempt_identity(&self) -> OwnedEngineIdentity {
        OwnedEngineIdentity {
            owner: self.original.identity.owner.clone(),
            generation: self.expected.lock().clone(),
        }
    }

    pub fn attempt_retirement(&self) -> EngineRetirementReceipt {
        EngineRetirementReceipt {
            identity: self.attempt_identity(),
        }
    }
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineRetirementStatus {
    /// Includes failed signalling and unknown process status; ownership is retained.
    Pending,
    /// The actual owned child reported exit. Resource admission remains separate.
    Exited,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EngineRetirementError {
    #[error("owned engine retirement is unsupported by this controller")]
    Unsupported,
    #[error("this controller does not own the requested engine")]
    NotOwned,
    #[error("the requested engine generation has been replaced")]
    StaleGeneration,
}


/// The serving-control seam. `LlamaServerProcess` is the real impl; tests use a
/// fake to exercise the pure reconcile decision without a live process.
#[async_trait]
pub trait LlamaServerControl: Send + Sync {
    /// Scoped physical paging uncertainty requires the normal owned-engine
    /// replacement path even when model identity and HTTP health are unchanged.
    fn paging_recovery_required(&self) -> bool {
        false
    }

    /// The model id the running server reports serving, or `None` if nothing is
    /// up. `Unreachable` = no server answering (a normal pre-spawn state).
    async fn active_model(&self) -> Result<Option<String>, LlamaServerError>;

    /// The LoRA genome layers the running server has loaded (sorted paths — the
    /// load identity, matching [`ServingTarget::adapter_paths`]). Empty when
    /// nothing is up or no genes are loaded. [`ensure_model_serving`] compares
    /// this against the desired set to detect a genome change (a new gene
    /// trained, or one retired) that requires a relaunch — page-in/out *within* a
    /// loaded set is per-request scale and never reaches here.
    async fn active_adapters(&self) -> Result<Vec<String>, LlamaServerError>;

    /// Converge the running engine's adapter set onto `target`'s WITHOUT a relaunch:
    /// load each new gene (dormant until a request gives it a scale) and retire each one
    /// the target dropped (charter S1, card 49b5e806). The default cannot, so the genome
    /// change rides a relaunch exactly as before.
    async fn adopt_genome_in_place(
        &self,
        _target: &ServingTarget,
    ) -> Result<GenomeInPlace, LlamaServerError> {
        Ok(GenomeInPlace::Unsupported)
    }

    /// (Re)spawn llama-server to serve `target` and block until it is ready.
    /// Switching models is a relaunch — there is no load-by-name API. The
    /// target carries the resolved model AND the host-fit served window, so the
    /// launcher never re-resolves or re-clamps anything.
    async fn serve(&self, target: &ServingTarget) -> Result<(), LlamaServerError>;

    /// Take the lane DOWN: the plan has no servable model (an unload suppressed the
    /// last candidate, or nothing on disk can speak). Kills the child we own and
    /// reclaims the canonical port from an adopted holder, so "node is idle" means
    /// the VRAM is actually free — measured 2026-09-06 on the 5090 (BigMama):
    /// `serving/unload` answered "freed model from VRAM; node is idle" while the
    /// llama-server stayed alive, and the next pin spawned a SECOND server beside
    /// it. Default is a no-op so test fakes that never model a process still compile.
    async fn idle(&self) -> Result<(), LlamaServerError> {
        Ok(())
    }

    /// Physical launch-stage receipt, advanced immediately before OS spawn.
    /// Checkpoint refusal and deferred preparation must leave this unchanged.
    fn last_launch_generation(&self) -> Option<crate::inference::slots::EngineGeneration> {
        None
    }

    async fn serve_if_current(
        &self,
        target: &ServingTarget,
        current: &ServingLifecycleCheck<'_>,
    ) -> Result<(), LlamaServerError> {
        if !current() {
            return Err(LlamaServerError::Superseded);
        }
        self.serve(target).await
    }

    async fn idle_if_current(
        &self,
        current: &ServingLifecycleCheck<'_>,
    ) -> Result<(), LlamaServerError> {
        if !current() {
            return Err(LlamaServerError::Superseded);
        }
        self.idle().await
    }

    /// Validate only original launch evidence, before an exclusive operation
    /// checkpoints or retires anything. This grants no capacity or intent lease.
    async fn validate_owned_launch(
        &self,
        _original: &OwnedServingTarget,
    ) -> Result<(), LlamaServerError> {
        Err(LlamaServerError::Spawn(
            "exact local restoration is unsupported by this controller".into(),
        ))
    }

    /// Low-level replay into an empty owned endpoint after confirmed child exit.
    /// The existing serving operation must retain resource admission and intent;
    /// this method creates neither an academy window nor a new lifecycle owner.
    async fn restore_owned_launch(
        &self,
        _original: &OwnedServingTarget,
        _current: &ServingLifecycleCheck<'_>,
    ) -> Result<(), LlamaServerError> {
        Err(LlamaServerError::Spawn(
            "exact local restoration is unsupported by this controller".into(),
        ))
    }

    async fn begin_owned_restore(
        &self,
        _original: &OwnedServingTarget,
    ) -> Result<OwnedRestoreSession, LlamaServerError> {
        Err(LlamaServerError::Spawn(
            "owned restore sessions are unsupported".into(),
        ))
    }

    async fn checkpoint_owned_restore(
        &self,
        _session: &OwnedRestoreSession,
        _current: &ServingLifecycleCheck<'_>,
    ) -> Result<EngineRetirementReceipt, LlamaServerError> {
        Err(LlamaServerError::Spawn(
            "owned checkpoint retirement is unsupported".into(),
        ))
    }

    async fn resume_owned_checkpoint(
        &self,
        _session: &OwnedRestoreSession,
        _current: &ServingLifecycleCheck<'_>,
    ) -> Result<(), LlamaServerError> {
        Err(LlamaServerError::Spawn(
            "owned checkpoint resume is unsupported".into(),
        ))
    }

    async fn restore_owned_session(
        &self,
        _session: &OwnedRestoreSession,
        _reservation: Option<crate::resources::LeaseGuard>,
        _current: &ServingLifecycleCheck<'_>,
    ) -> Result<(), LlamaServerError> {
        Err(LlamaServerError::Spawn(
            "owned restore sessions are unsupported".into(),
        ))
    }

    fn owned_serving_target(&self) -> Option<OwnedServingTarget> {
        None
    }

    /// Capture local child identity; an externally adopted endpoint has none.
    fn owned_engine(&self) -> Option<OwnedEngineIdentity> {
        None
    }

    /// Drain and request retirement of exactly this child, never an adopted PID.
    /// Success returns an observation handle, not evidence of exit or freed memory.
    async fn retire_owned_engine(
        &self,
        _expected: &OwnedEngineIdentity,
    ) -> Result<EngineRetirementReceipt, EngineRetirementError> {
        Err(EngineRetirementError::Unsupported)
    }

    /// Poll the retained process owner. No timer or signal acknowledgement proves exit.
    fn observe_engine_retirement(
        &self,
        _receipt: &EngineRetirementReceipt,
    ) -> Result<EngineRetirementStatus, EngineRetirementError> {
        Err(EngineRetirementError::Unsupported)
    }

    /// The REAL per-slot context window the running server serves, read from
    /// `/props` (`default_generation_settings.n_ctx`). This is the AUTHORITATIVE
    /// model metadata: llama.cpp pads the launch per-slot window (`-c / --parallel`)
    /// UP to a 256-multiple internally, so only the process can report what each
    /// slot will actually accept. The daemon reads this after a ready reconcile and
    /// stamps it onto [`ServingSnapshot::served_context_window`], so every persona
    /// budgets its prompt to the truth instead of the planner's drifting window
    /// (which overflows the slot → 500 "Compute error"). `Unreachable` = no server
    /// answering (pre-spawn), or a ready server whose `/props` shape is missing
    /// `n_ctx` (a loud invariant violation — never a guessed window).
    async fn served_context_window(&self) -> Result<u32, LlamaServerError>;

    /// The REAL number of continuous-batching slots the running server serves,
    /// read from its own `/props` (`total_slots`) — the LANE sibling of
    /// [`Self::served_context_window`], and for the same reason.
    ///
    /// # Why this must be probed and not remembered (measured 2026-08-19)
    ///
    /// The adopt-or-relaunch decision has two operands. The window one always
    /// asked the process. The lane one asked `current_serving().lanes` — our own
    /// process-local snapshot. For a lane THIS core spawned those agree, so the
    /// asymmetry was invisible. For a lane it did NOT spawn — a **past form of
    /// ourself**, surviving a crash on the canonical port and named in our own
    /// custody record — the snapshot describes nothing, and a healthy 4-lane
    /// 27B was declined, reaped, and rebuilt over ~110s only to come back at 3
    /// lanes and a smaller window than the lane that was already there.
    ///
    /// One operand asking the lane and the other asking our memory of it is the
    /// whole defect. Both now ask the lane.
    ///
    /// Error policy is deliberately identical to the window's: a probe failure
    /// returns `Err` and the CALLER treats it as "lanes OK" so a transient
    /// `/props` hiccup can never spuriously relaunch a healthy lane. `Ok(0)`
    /// means the server named no slot count — nothing to compare, not zero
    /// capacity ([[an-absence-is-an-unfinished-measurement]]).
    async fn served_lanes(&self) -> Result<u32, LlamaServerError>;

    /// The engine build the running server reports (`/props` `build_info`, e.g.
    /// `b10765-965d38a90`): the fourth adopt-or-relaunch operand, asked of the lane like the
    /// window and the lanes (card 7c5f139d). `Ok(None)` = the server names none (a fake or
    /// remote control, or an engine without the field): nothing to compare.
    async fn served_engine_build(&self) -> Result<Option<String>, LlamaServerError> {
        Ok(None)
    }

    /// The engine process serving this endpoint now ([`EngineIncarnation`]: pid + OS start
    /// time + port), owned or adopted, read from the lane pidfile and its registry record:
    /// what a `/props` reading is evidence about. `None` = unknown (a fake or remote control,
    /// no pidfile, a record without a start time): no reading outlives a probe error then.
    ///
    /// [`EngineIncarnation`]: crate::inference::engine_residency::EngineIncarnation
    fn served_incarnation(&self) -> Option<crate::inference::engine_residency::EngineIncarnation> {
        None
    }

    /// The multimodal capabilities the running server ITSELF reports in `/props`
    /// (`modalities.vision` / `modalities.audio`) — the endpoint-side truth of
    /// whether the `--mmproj` projector actually loaded (#106). `Ok(None)` means
    /// the server answered but its `/props` carries no `modalities` block (an
    /// older llama-server build, or a fake control that doesn't probe) — the
    /// caller must treat that as UNVERIFIED, never as "vision works". Default
    /// impl returns `Ok(None)` so fakes/remote controls stay honest by
    /// construction.
    async fn multimodal_support(&self) -> Result<Option<MultimodalSupport>, LlamaServerError> {
        Ok(None)
    }

    /// Did THIS control's own spawn pass a projector? A FACT about the running
    /// process, never a re-derivation of policy. `None` = unknown: adopted from an
    /// earlier core, not yet spawned by this one, or a fake/remote control that has
    /// no spawn — the default, so fakes stay honest by construction.
    fn mmproj_on_lane(&self) -> Option<bool> {
        None
    }

    /// The `--cache-ram` grant (MiB) THIS control's own spawn passed. A FACT about
    /// the running process, carried on the serving snapshot so the footprint
    /// measurement and the board charge what the engine holds. `None` = unknown:
    /// adopted from an earlier core, not yet spawned by this one, or a fake/remote
    /// control — the default, so fakes stay honest by construction.
    fn launched_prompt_cache_mib(&self) -> Option<u32> {
        None
    }

    /// Prove the GPU DECODE path works, not just that the HTTP server is up. A
    /// llama-server can answer `/health`, `/v1/models` and `/props` with 200
    /// while EVERY `llama_decode` returns 500 "Compute error" — observed live in
    /// a 47-min-old orphan reclaimed from a SIGKILLed core, whose Metal compute
    /// context had gone bad. `/models` proves the listener thread is alive; only
    /// an actual generation proves the model can think. This runs a trivial
    /// 1-token completion and returns `true` iff it succeeds — the readiness
    /// signal `/health` alone cannot give. Any error (unreachable, non-200,
    /// compute error) → `false`: "this lane cannot decode", which the caller
    /// turns into reap + respawn rather than adopting a wedged server.
    async fn decode_smoke_ok(&self) -> bool;

    /// Does THIS control own the running child (we spawned it), versus pointing
    /// at a server some other process launched (an adopted orphan)? The adopt
    /// decision trusts a child we spawned and verified at [`wait_ready`], but
    /// must decode-smoke-probe an orphan we did not — that orphan is exactly the
    /// one that can be compute-wedged. Default `false` (a fake/remote control
    /// owns no local child) so the conservative path (probe before adopt) is the
    /// default.
    fn owns_child(&self) -> bool {
        false
    }

    /// The KV decision the RUNNING engine was launched with — read off its own argv
    /// (card 977842fd), so the adopt rail can compare it against the decision a spawn
    /// would make NOW. `None` = unknown (no live lane, argv unreadable, a fake): an
    /// absence is not a mismatch; adoption proceeds on the other axes.
    fn launch_kv(&self) -> Option<crate::inference::lane_process::LaunchKv> {
        None
    }

    /// Fingerprint of the server's `/slots` activity state (see
    /// [`slots_activity_fingerprint_of`]). The health heartbeat compares the value
    /// across two consecutive smoke-probe MISSES: changed → the serve loop is
    /// advancing (ghost work, other clients' turns — work the adapter stamps can't
    /// see) and the miss is queue contention; frozen → the miss counts as wedge
    /// evidence exactly as before. `None` = "cannot say" (no endpoint, fake/remote
    /// control) — never exoneration. Default `None` keeps fakes honest.
    async fn slots_activity_fingerprint(&self) -> Option<u64> {
        None
    }

    /// Largest in-flight prompt investment on the lane (tokens) — see
    /// [`slots_max_inflight_prompt_tokens_of`]. Default `None` = unknown/none;
    /// the health tick then applies base patience.
    async fn slots_max_inflight_prompt_tokens(&self) -> Option<u64> {
        None
    }

    /// The `/slots` body itself, for a reader that needs more than one derived number (the
    /// prefill knee's server-total rate, card e370a673). Default `None` = no endpoint to
    /// read, so a fake or remote control publishes nothing.
    async fn slots_body(&self) -> Option<serde_json::Value> {
        None
    }

    /// The flag this control's stderr watcher raises when the running lane proves itself
    /// WEDGED — a slot reporting arithmetically impossible progress (see
    /// [`crate::inference::wedge`]). The serving daemon polls it on its tick and owns the
    /// response; the watcher only reports, so lane lifecycle stays with exactly one owner.
    ///
    /// `None` for a control with nobody to report to: a fake, a remote control, or an
    /// ephemeral eval lane that tears down its own process. Default `None` keeps that the
    /// honest answer rather than a silently-never-raised flag.
    fn wedge_flag(&self) -> Option<crate::inference::wedge::WedgeFlag> {
        None
    }

    /// Record the adopted lane's placement from the engine's own account when its lane record
    /// has none (a lane kept across a deploy from before records carried one; card 22aa3379).
    /// The reconcile calls it on every `AlreadyServing`. Default: nothing, for a control with
    /// no lane record to write (a fake or a remote control).
    async fn record_observed_placement(&self) {}

    /// Follow the served engine's log relay, so an engine this core adopted has its lines watched
    /// as a spawned one's are (card 1d7d6d4c). Default: a fake or remote control has no local
    /// engine to follow.
    fn follow_engine_log(&self) {}
}
