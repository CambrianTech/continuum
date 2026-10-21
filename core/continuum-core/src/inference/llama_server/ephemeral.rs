//! Ephemeral responsibility of the llama-server process owner.
use super::*;
use super::configuration::DEFAULT_HOST;
/// An ephemeral, self-contained llama-server lane on its OWN port — the
/// serving-LEASE primitive in embryo: stand up capacity for a
/// `(base model, adapters, context window)` on demand, use it, release it.
///
/// The live persona lane (the global [`serving_root`] snapshot) is never
/// touched: this spawns a SECOND server on a free port scanned up from a
/// caller-chosen base. Dropping the lane requests retirement; the retained child
/// owner releases its reservation only after observed exit. That isolation is exactly what
/// [`crate::cognition::eval`]'s genome A/B needs — the humane-eval invariant
/// (#59) is "measure a copy, never degrade the living persona", and you cannot
/// measure a gene against its forged base if doing so re-homes the model the
/// living persona is currently thinking with.
///
/// Generalized, this is the same lease the grid uses to place demand-driven
/// capacity across heterogeneous nodes: today the host is `localhost` and the
/// budget check is one machine's free VRAM (the `ResourceGovernor`'s job, #56);
/// tomorrow the host is a peer and the budget spans the interlinked grid. One
/// abstraction, misfit toys.
// ---------------------------------------------------------------------------
// Ephemeral-lane admission — the invariant that used to be #56's unbuilt governor
// ---------------------------------------------------------------------------

/// Serializes ephemeral cold-spawns process-wide: only one `llama-server` cold-loads
/// at a time, so two lanes cannot thrash the device against each other and same-target
/// racers collapse onto the first spawn's result.
///
/// This lived in `cognition::eval` as `EVAL_LANE_SPAWN_GATE` — correct behaviour, at
/// ONE caller. The vision sidecar spawns through the same primitive and inherited none
/// of it, which is how three sidecars ended up co-resident. A discipline that only one
/// caller opts into is not a discipline; it lives at the primitive now.
///
/// `cognition::eval::EVAL_LANE_SPAWN_GATE` still exists and is NOT a duplicate: it
/// guards eval's warm-lane map so same-key racers share one lane. LOCK ORDER is that
/// gate, then this one — the only nesting in the tree, so it cannot invert.
pub(super) static EPHEMERAL_SPAWN_GATE: std::sync::LazyLock<tokio::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));


/// What an admission authority answers. `Refuse` carries the operator-readable WHY,
/// because a refusal nobody can explain gets deleted by the next person who hits it.
#[derive(Debug, Clone)]
pub enum LaneAdmission {
    Admit,
    Refuse(String),
}


/// The authority that decides whether another engine may be placed on this device.
///
/// Late-bound because the budget belongs to the serving daemon (it owns the GPU
/// manager and the live memory monitor) while the decision is needed down here at the
/// spawn. Same shape as [`install_serving_state`]: the daemon is a singleton, so this
/// is set-once.
///
/// `resident` is the verified-live ephemeral census at decision time — the authority
/// never has to re-derive it, and cannot disagree with the caller about what is
/// already running.
pub type LaneAdmissionAuthority = Arc<
    dyn Fn(&ServingTarget, &[crate::inference::lane_registry::LaneRecord]) -> LaneAdmission
        + Send
        + Sync,
>;


pub(super) static LANE_ADMISSION: OnceLock<LaneAdmissionAuthority> = OnceLock::new();


/// Install the device-budget authority. Returns `true` iff this call installed it.
pub fn install_lane_admission(authority: LaneAdmissionAuthority) -> bool {
    LANE_ADMISSION.set(authority).is_ok()
}


/// True once an authority is installed — so a wiring test can assert production
/// actually installs one rather than trusting the permissive default below.
pub fn lane_admission_installed() -> bool {
    LANE_ADMISSION.get().is_some()
}


/// Reconcile the lane registry against reality and return the EPHEMERAL lanes that are
/// genuinely alive.
///
/// Bookkeeping first, decision second: a record whose pid is dead (or whose number the
/// OS recycled onto something else) is removed, never signalled and never counted.
/// `is_llama_server` is the same never-blind-kill identity check `lane_pidfile` and
/// `lane_registry` already share — one decision, one place.
///
/// Doing this INSIDE the spawn is the point. `sweep_orphans` runs once at daemon init
/// and `sweep_all` only on `stop` (which `reboot` skips by design, keeping the live
/// lane up for adoption). Neither can see a sibling that appeared five seconds ago.
pub(super) fn reconcile_ephemeral_census() -> Vec<crate::inference::lane_registry::LaneRecord> {
    use crate::inference::{lane_process, lane_registry};
    lane_registry::records()
        .into_iter()
        .filter(|rec| rec.role == lane_registry::LaneRole::Ephemeral)
        .filter(|rec| {
            if lane_process::is_llama_server(rec.pid) {
                return true;
            }
            // Dead, or the pid was recycled onto an unrelated process. Either way it
            // holds no VRAM of ours and must not weigh on the decision.
            lane_registry::remove(rec.pid);
            false
        })
        .collect()
}


/// `Some(reason)` when this spawn must be refused. Consults the installed authority;
/// with none installed the census is still reconciled and the spawn is admitted, but
/// LOUDLY — an unwired budget must be visible, not silently permissive
/// ([[silently-unwired-capability]]). Production wiring is asserted by
/// `the_serving_daemon_installs_the_admission_authority`.
pub(super) fn ephemeral_admission_refusal(
    target: &ServingTarget,
    resident: &[crate::inference::lane_registry::LaneRecord],
) -> Option<String> {
    let Some(authority) = LANE_ADMISSION.get() else {
        crate::probe!(target: "continuum_core::inference::llama_server", class = "serving.lane.admission_unwired",
            model = target.model_id(),
            resident_ephemeral = resident.len(),
            "no device-budget authority is installed — admitting this engine WITHOUT a \
             budget check; in a live core the serving daemon installs one at wiring time"
        );
        return None;
    };
    match authority(target, resident) {
        LaneAdmission::Admit => None,
        LaneAdmission::Refuse(reason) => Some(reason),
    }
}


pub struct EphemeralServingLane {
    pub(super) proc: LlamaServerProcess,
    pub(super) port: u16,
    pub(super) spawn_permit: Option<tokio::sync::MutexGuard<'static, ()>>,
}


impl EphemeralServingLane {
    /// Stand up a fresh llama-server for `target` on a free port scanned up from
    /// `base_port` (distinct from the live lane). Fails loud
    /// ([[fallbacks-are-illegal-fail-loud]]) if no port binds, the model GGUF is
    /// missing, or any `--lora` gene file is absent — never serves a substitute.
    ///
    /// # Admission is an INVARIANT here, not an event somewhere else
    ///
    /// This doc used to say: *"Budget-gating (does this fit alongside the live lane?)
    /// is the `ResourceGovernor`'s concern (#56); this primitive only stands the lane
    /// up."* **`ResourceGovernor` does not exist.** Every one of its eight mentions in
    /// the tree is a doc comment promising a future owner, so the only question ever
    /// asked before putting another engine on the device was `first_free_port` — *is a
    /// port free?* A port is cheap and genuinely per-lane; the GPU is one shared thing.
    /// Using port-freeness as a proxy for device capacity is what let three vision
    /// sidecars co-exist on one GPU (ports 58091/58092/58093 — and `first_free_port`
    /// means N+1 exists ONLY because N was occupied, so the filenames themselves prove
    /// the concurrency). Each ran at 3.1-8.6 tok/s against 60-80 normal for that box:
    /// not a slow model, three engines time-slicing one device.
    ///
    /// The reclaim machinery was never the problem — `lane_registry` records every lane
    /// and `lane_pidfile` adopts-or-reaps the live one, both with proper Windows arms.
    /// The problem was that every reclaim fired on an EDGE (`sweep_orphans` at daemon
    /// init, `sweep_all` at `stop`, `Drop` on graceful exit) and `reboot` deliberately
    /// skips the stop-side sweep, so nothing bounded engines WITHIN one core's life.
    ///
    /// So the question moves here, where the decision actually is:
    ///
    /// 1. **Serialize.** One ephemeral cold-spawn at a time, process-wide. This gate
    ///    already existed as `EVAL_LANE_SPAWN_GATE` in `cognition::eval` — at ONE
    ///    caller, so the vision sidecar inherited nothing from it. Moved down so every
    ///    ephemeral lane gets it by construction.
    /// 2. **Census.** Reconcile the registry against reality first, so the admission
    ///    decision is taken against lanes that are actually alive.
    /// 3. **Admit or refuse.** Ask the installed authority (the serving daemon owns the
    ///    device budget). Refusal is LOUD and returns an error — never a quiet second
    ///    engine.
    pub async fn spawn(target: &ServingTarget, base_port: u16) -> Result<Self, LlamaServerError> {
        Self::spawn_with_reservation(target, base_port, None).await
    }

    /// Transfer the governed reservation to the process owner before process startup
    /// awaits. Failed/cancelled startup and normal teardown retain it until exit.
    pub(crate) async fn spawn_with_reservation(
        target: &ServingTarget,
        base_port: u16,
        reservation: Option<crate::resources::LeaseGuard>,
    ) -> Result<Self, LlamaServerError> {
        let mut lane = Self::prepare_with_reservation(target, base_port, reservation).await?;
        lane.start(target).await?;
        Ok(lane)
    }

    pub(crate) async fn prepare_with_reservation(
        target: &ServingTarget,
        base_port: u16,
        reservation: Option<crate::resources::LeaseGuard>,
    ) -> Result<Self, LlamaServerError> {
        // Held across the spawn await, so a second ephemeral spawn cannot interleave and
        // land on `first_free_port + 1`. A tokio mutex because it spans `.await` — never
        // a std lock across await (docs/architecture/CONCURRENCY-STYLE-GUIDE.md).
        let permit = EPHEMERAL_SPAWN_GATE.lock().await;
        let resident = reconcile_ephemeral_census();
        if let Some(reason) = ephemeral_admission_refusal(target, &resident) {
            crate::probe!(target: "continuum_core::inference::llama_server", class = "serving.lane.admission_refused",
                model = target.model_id(),
                resident_ephemeral = resident.len(),
                reason = reason.as_str(),
                "refused another engine on this device — admission is decided by the                  device budget, never by port availability"
            );
            return Err(LlamaServerError::NotReady(DEFAULT_SERVING_WAIT, reason));
        }
        let port = first_free_port(base_port);
        let root = format!("http://{}:{}", DEFAULT_HOST, port);
        let mut lane =
            Self::with_reservation(LlamaServerProcess::with_root(root), port, reservation);
        lane.spawn_permit = Some(permit);
        Ok(lane)
    }

    /// The owner stores this lane before awaiting startup, retaining its generation
    /// receipt on timeout, readiness error or cancellation.
    pub(crate) async fn start(&mut self, target: &ServingTarget) -> Result<(), LlamaServerError> {
        self.start_inner(target, false).await
    }

    /// The exclusive teacher holds a device grant; a backend fallback cannot
    /// silently change that admitted workload into a host-memory CPU load.
    pub(crate) async fn start_gpu(
        &mut self,
        target: &ServingTarget,
    ) -> Result<(), LlamaServerError> {
        self.start_inner(target, true).await
    }

    pub(super) async fn start_inner(
        &mut self,
        target: &ServingTarget,
        require_gpu: bool,
    ) -> Result<(), LlamaServerError> {
        // HARD wall-clock cap on the WHOLE bring-up. `wait_ready`'s budget only
        // bounds the /health poll, and its deadline is checked BETWEEN attempts — so a
        // hang INSIDE an attempt (a stalled `decode_smoke_ok`, a wedged model-mmap, a
        // process-launch that never returns) can wait past it indefinitely. Glass-boxed
        // 2026-07-19: an ephemeral eval lane hung 11 min with no process and free VRAM,
        // no timeout ever firing — a silent glacial wedge that violated fail-loud. This
        // net guarantees the eval fails LOUD after the lane's own load budget instead.
        // On timeout the owning caller retires this lane (ordinary callers drop
        // it; an explicit batch retains its exit receipt). The live lane keeps
        // its own bring-up policy.
        // MUST exceed the ephemeral `wait_ready` budget (EPHEMERAL_READY_TIMEOUT) + margin
        // so the INNER deadline fires first with its specific `/health`/decode reason —
        // otherwise this coarse net pre-empts the diagnostic error (glass-boxed 2026-07-21:
        // a co-resident cold 24B warmup exceeded the old 90s inner budget; the fix raised
        // the inner budget, so this outer cap must follow or it clips the warmup at 120s).
        let spawn_cap = EPHEMERAL_READY_TIMEOUT + Duration::from_secs(30);
        let startup = async {
            if require_gpu {
                let prepared = self.proc.prepare_local_launch(target).await?;
                // Flags are last-wins: the backend probe appends its CPU verdict.
                let offload = prepared
                    .invocation
                    .args
                    .windows(2)
                    .rev()
                    .find(|pair| pair[0] == "--n-gpu-layers")
                    .and_then(|pair| pair[1].parse::<i64>().ok());
                if !offload.is_some_and(|layers| layers != 0) {
                    return Err(LlamaServerError::Spawn(
                        "private teacher requires GPU offload; backend CPU fallback refused".into(),
                    ));
                }
                self.proc
                    .launch_prepared(Arc::new(prepared), &|| true, None, None)
                    .await
            } else {
                self.proc.serve(target).await
            }
        };
        match tokio::time::timeout(spawn_cap, startup).await {
            Ok(res) => res?,
            Err(_) => {
                return Err(LlamaServerError::NotReady(
                    DEFAULT_SERVING_WAIT,
                    "ephemeral eval lane bring-up exceeded the wall-clock budget \
                     (launch / port-bind / model-mmap / health-probe hang)"
                        .to_string(),
                ));
            }
        }
        self.spawn_permit.take();
        Ok(())
    }

    pub(super) fn with_reservation(
        proc: LlamaServerProcess,
        port: u16,
        reservation: Option<crate::resources::LeaseGuard>,
    ) -> Self {
        *proc.pending_resource_reservation.lock() = reservation;
        Self {
            proc,
            port,
            spawn_permit: None,
        }
    }

    /// Request teardown without losing the observation handle on failed startup.
    /// The private batch owner retains this lane until this returns Exited.
    pub(crate) async fn finish(&mut self) -> Result<EngineRetirementStatus, LlamaServerError> {
        self.spawn_permit.take();
        let generation = self.proc.last_started.lock().clone();
        let Some(generation) = generation else {
            self.proc.pending_resource_reservation.lock().take();
            return Ok(EngineRetirementStatus::Exited);
        };
        let identity = OwnedEngineIdentity {
            owner: Arc::downgrade(&self.proc.child),
            generation,
        };
        if self.proc.owned_engine().as_ref() == Some(&identity) {
            self.proc
                .retire_owned_engine(&identity)
                .await
                .map_err(|e| LlamaServerError::Spawn(e.to_string()))?;
        }
        self.proc
            .observe_engine_retirement(&EngineRetirementReceipt { identity })
            .map_err(|e| LlamaServerError::Spawn(e.to_string()))
    }

    /// The OpenAI-compatible `/v1` base url a persona's inference adapter points
    /// at to route deliberation through THIS lane instead of the live one.
    pub fn v1_url(&self) -> String {
        self.proc.v1_url.clone()
    }

    /// `http://host:port` of this lane (no `/v1`).
    pub fn root(&self) -> &str {
        &self.proc.root
    }

    /// The bound port — for probes/telemetry that want to name the lane.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The REAL per-slot window this lane serves, read from its own `/props`
    /// (`default_generation_settings.n_ctx`) — the SAME served-truth source the
    /// live daemon pins the resident persona's window to
    /// (`supervisor.rs`: `profile.context_length = snap.served_context_window`).
    /// An eval fork's cognition is sized to THIS, never the planned launch `-c`,
    /// so a measurement copy plans against exactly the window it is served — no
    /// planned-vs-served drift between training and the lane it runs on
    /// ([[dreaming-mind-eval-must-match-live-cognition]], task #50). Fails loud if
    /// the server is up but `/props` is unreadable — never a guessed window.
    pub async fn served_context_window(&self) -> Result<u32, LlamaServerError> {
        self.proc.served_context_window().await
    }

    /// The running lane's own `/props modalities` verdict — the #106 endpoint
    /// truth, delegated to the child probe. `Ok(None)` = this llama-server
    /// build publishes no modalities block (unverifiable ≠ working).
    pub async fn multimodal_support(&self) -> Result<Option<MultimodalSupport>, LlamaServerError> {
        LlamaServerControl::multimodal_support(&self.proc).await
    }

    /// The model id this lane actually serves (its `/v1/models` alias) — for
    /// incumbent verification against the process's own report, never our spawn
    /// memory.
    pub async fn active_model(&self) -> Result<Option<String>, LlamaServerError> {
        LlamaServerControl::active_model(&self.proc).await
    }
}
