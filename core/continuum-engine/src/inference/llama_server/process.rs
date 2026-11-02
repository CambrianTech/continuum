//! Process responsibility of the llama-server process owner.
use super::*;
pub(super) struct OwnedEngine {
    pub(super) child: tokio::process::Child,
    pub(super) generation: crate::inference::slots::EngineGeneration,
    pub(super) verified_target: Option<OwnedServingTarget>,
    pub(super) resource_reservation: Option<crate::resources::LeaseGuard>,
    pub(super) engine_install: Option<crate::inference::engine_install::EngineInstallReceipt>,
    pub(super) local_inputs: Option<crate::inference::launch_inputs::LocalLaunchInputs>,
}


/// Owns the supervised `llama-server` child. One per host. `serve` kills any
/// prior child before launching the new model, so there is never more than one
/// llama-server competing for the GPU. `Drop` kills the child — the core owning
/// the launch means the core owns the teardown.
pub struct LlamaServerProcess {
    pub(super) root: String,
    pub(super) v1_url: String,
    /// A test's fixture engine in place of [`server_bin`]'s per-launch resolution.
    #[cfg(any(test, feature = "test-fixtures"))]
    pub(super) bin_pin: Option<String>,
    pub(super) client: reqwest::Client,
    /// The live child, if one is running. `std::sync::Mutex` (not tokio) because
    /// it is held only for the brief swap/kill, never across an await.
    pub(super) child: Arc<StdMutex<Option<OwnedEngine>>>,
    /// Before OS spawn only; transferred atomically to the actual owned child.
    pub(super) pending_resource_reservation: parking_lot::Mutex<Option<crate::resources::LeaseGuard>>,
    // Retains the exact startup generation even when readiness already retired it.
    pub(super) last_started: parking_lot::Mutex<Option<crate::inference::slots::EngineGeneration>>,
    /// A launch whose readiness wait ended while its engine still loaded ([`super::late_ready`]):
    /// the next launch finishes that engine instead of replacing it, when it is still the same.
    pub(super) loading: parking_lot::Mutex<Option<super::late_ready::LoadingLaunch>>,
    /// A signalled adopted predecessor stays fenced across timeout/cancellation.
    pub(super) retiring_adopted: parking_lot::Mutex<Option<crate::inference::engine_residency::EngineIncarnation>>,
    /// The residents that predecessor loses unsaved, recorded only once its exit is observed
    /// (card 6b28a445). Held with `retiring_adopted`, so a re-entered wait still records it.
    pub(super) retiring_discard: parking_lot::Mutex<Option<crate::inference::slots::PendingDiscard>>,
    /// The LoRA genome set (sorted paths) the CURRENT child was launched with —
    /// the truthful record of what `/lora-adapters` holds, since llama.cpp has no
    /// API to query it. `ensure_model_serving` compares this against the desired
    /// set to decide a relaunch. Reset on each `serve()`. After a core restart
    /// with a persisting server this reads empty → one safe extra relaunch
    /// re-establishes the genome (core owns the lifecycle, so this is truthful in
    /// the steady state).
    pub(super) served_adapters: Arc<StdMutex<Vec<String>>>,
    /// The page-dir reservation of the CURRENT child when its registry record could
    /// not be written — kept until the owned child reports exit
    /// (see [`PageDirReservation`]). Empty when the record carries the dir.
    pub(super) retained_page_dir: Arc<parking_lot::Mutex<Option<PageDirReservation>>>,
    /// True for THE host's live persona lane (pins the canonical port, writes the
    /// reclaim pidfile, reaps a crashed predecessor's orphan). False for an
    /// [`EphemeralServingLane`]'s `with_root` process, which owns its own scanned
    /// port and must NEVER write the canonical pidfile or reclaim the live port —
    /// it would reap the living persona's own server. This boolean is the seam
    /// that keeps the reclaim machinery bound to exactly one process.
    pub(super) is_live_lane: bool,
    /// Where a spawned candidate's `tail_tools` evidence is published ([`LaunchedEvidence`]):
    /// the serving lane's cell for the live lane, none for an ephemeral lane, which serves no
    /// resident's tool head.
    pub(super) served_evidence: Option<&'static ServedEvidence>,
    /// Raised by this lane's stderr watcher when a slot reports impossible progress,
    /// polled by the serving daemon. Live lane only — an ephemeral lane has no daemon
    /// watching it and tears down its own process, so its watcher would report into a void.
    pub(super) wedge: Option<crate::inference::wedge::WedgeFlag>,
    /// The engine's own offload banner (`offloaded X/Y layers to GPU`), recorded by the
    /// same stderr pump — the READBACK half of the placement contract (#441). Every lane
    /// carries it (live and ephemeral): a CPU-fallback eval lane silently corrupts a
    /// benchmark's wall-clock exactly like a live one starves citizens.
    pub(super) offload: crate::inference::placement_watch::OffloadReport,
    /// LEARNED, once per lane: the served model spends its quick smoke budget inside
    /// the reasoning channel (a thinking model). Latched on the first `ThinkStarved`
    /// verdict; every later readiness check then STARTS at the thinking budget instead
    /// of re-proving the same property by burning a doomed 24-token generation first.
    /// Measured before this latch: the identical `think_retry` fired ~30×/hour on a
    /// contended lane — two generations per readiness check, forever, on the same
    /// slots the citizens were starving for. Thinking-ness is a property of the MODEL;
    /// a probe that rediscovers a known property by experiment on every tick is waste
    /// (ADMISSION-IS-UNOBSERVABLE §4c). Lane-scoped (one lane serves one model), reset
    /// naturally with the lane; the model catalog inherits this as a declared field
    /// when the recipe/entity work lands.
    pub(super) smoke_proven_thinking: std::sync::atomic::AtomicBool,
    /// Whether THIS handle's spawn passed `--mmproj`: 0 = unknown (adopted, or not
    /// yet spawned by this core), 1 = no, 2 = yes. A fact about the running process,
    /// read by the serving daemon's sight check — never re-derived from policy.
    pub(super) mmproj_on_lane: std::sync::atomic::AtomicU8,
    /// The `--cache-ram` grant (MiB) THIS handle's spawn passed: 0 = unknown (adopted,
    /// or not yet spawned by this core). The same kind of fact as `mmproj_on_lane`:
    /// what the running process was given, read by the serving daemon into the
    /// snapshot — never the plan's latest derivation of what it would give.
    pub(super) launched_prompt_cache_mib: std::sync::atomic::AtomicU32,
}


impl LlamaServerProcess {
    pub(super) async fn props_json(&self) -> Result<serde_json::Value, LlamaServerError> {
        engine_props(&self.root, &self.client).await
    }

    /// Where the weights were ALLOCATED, from `/props` (`model_weight_buffers`).
    /// `Ok(None)` = the engine predates the field: channel unavailable, which is
    /// not a placement.
    pub async fn weight_residency(
        &self,
    ) -> Result<Option<crate::inference::weight_residency::WeightResidency>, LlamaServerError> {
        let body = self.props_json().await?;
        Ok(crate::inference::weight_residency::WeightResidency::from_props(&body))
    }

    /// Did THIS handle's spawn pass a projector? `None` = unknown: the lane was
    /// adopted from an earlier core or has not been spawned by this one. The serving
    /// daemon's sight check reads this fact; the grow-check relaunches a lane whose
    /// fact is below today's decision.
    pub fn mmproj_on_lane(&self) -> Option<bool> {
        match self.mmproj_on_lane.load(std::sync::atomic::Ordering::Relaxed) {
            2 => Some(true),
            1 => Some(false),
            _ => None,
        }
    }

    /// The `--cache-ram` grant THIS handle's spawn passed (MiB); `None` until this
    /// core has spawned the child (an adopted lane's grant is unknown).
    pub fn launched_prompt_cache_mib(&self) -> Option<u32> {
        match self.launched_prompt_cache_mib.load(std::sync::atomic::Ordering::Relaxed) {
            0 => None,
            n => Some(n),
        }
    }

    pub fn new() -> Self {
        Self::with_client(reqwest::Client::new())
    }

    pub fn with_client(client: reqwest::Client) -> Self {
        Self {
            root: serving_root(),
            v1_url: serving_v1_url(),
            #[cfg(any(test, feature = "test-fixtures"))]
            bin_pin: None,
            client,
            child: Arc::new(StdMutex::new(None)),
            pending_resource_reservation: parking_lot::Mutex::new(None),
            last_started: parking_lot::Mutex::new(None),
            loading: parking_lot::Mutex::new(None),
            retiring_adopted: parking_lot::Mutex::new(None),
            retiring_discard: parking_lot::Mutex::new(None),
            served_adapters: Arc::new(StdMutex::new(Vec::new())),
            retained_page_dir: Arc::new(parking_lot::Mutex::new(None)),
            // THE host's live lane — pins the canonical port, owns the reclaim
            // pidfile. `new()`/`with_client()` are the live constructors.
            is_live_lane: true,
            served_evidence: Some(serving_lane_evidence()),
            wedge: Some(crate::inference::wedge::WedgeFlag::new()),
            offload: crate::inference::placement_watch::OffloadReport::new(),
            smoke_proven_thinking: std::sync::atomic::AtomicBool::new(false),
            mmproj_on_lane: std::sync::atomic::AtomicU8::new(0),
            launched_prompt_cache_mib: std::sync::atomic::AtomicU32::new(0),
        }
    }

    /// Build a process bound to an EXPLICIT `root` (`http://host:port`) instead of
    /// the global [`serving_root`]. This is the seam that lets a SECOND
    /// llama-server run on its own port without colliding with the live lane or
    /// publishing to the global serving snapshot (`serve()` is already
    /// snapshot-free — only [`crate::modules::serving_daemon`] publishes the
    /// global state). [`EphemeralServingLane`] is the only caller; the `v1_url` is
    /// derived from `root` so a persona's inference adapter can point straight at
    /// this lane.
    pub fn with_root(root: String) -> Self {
        let v1_url = format!("{}/v1", root.trim_end_matches('/'));
        Self {
            root,
            v1_url,
            #[cfg(any(test, feature = "test-fixtures"))]
            bin_pin: None,
            client: reqwest::Client::new(),
            child: Arc::new(StdMutex::new(None)),
            pending_resource_reservation: parking_lot::Mutex::new(None),
            last_started: parking_lot::Mutex::new(None),
            loading: parking_lot::Mutex::new(None),
            retiring_adopted: parking_lot::Mutex::new(None),
            retiring_discard: parking_lot::Mutex::new(None),
            served_adapters: Arc::new(StdMutex::new(Vec::new())),
            retained_page_dir: Arc::new(parking_lot::Mutex::new(None)),
            // Ephemeral lane on its OWN scanned port: NOT the canonical live lane.
            // It must never write the canonical pidfile or reclaim the live port.
            is_live_lane: false,
            served_evidence: None,
            // No serving daemon watches an ephemeral lane, and its owner tears the process
            // down when the eval ends — a wedge report would have no consumer.
            wedge: None,
            offload: crate::inference::placement_watch::OffloadReport::new(),
            smoke_proven_thinking: std::sync::atomic::AtomicBool::new(false),
            mmproj_on_lane: std::sync::atomic::AtomicU8::new(0),
            launched_prompt_cache_mib: std::sync::atomic::AtomicU32::new(0),
        }
    }

    /// No await between OS spawn and this transfer: cancellation/readiness failure
    /// cannot release accounting independently of the child that consumes it.
    pub(super) fn install_child(
        &self,
        child: tokio::process::Child,
        generation: crate::inference::slots::EngineGeneration,
        engine_install: Option<crate::inference::engine_install::EngineInstallReceipt>,
        local_inputs: Option<crate::inference::launch_inputs::LocalLaunchInputs>,
    ) {
        *self.last_started.lock() = Some(generation.clone());
        let mut owned = self.child.lock().unwrap(); // JUSTIFIED: poison means a prior panic mutating the owned child.
        *owned = Some(OwnedEngine {
            child,
            generation,
            verified_target: None,
            resource_reservation: self.pending_resource_reservation.lock().take(),
            engine_install,
            local_inputs,
        });
    }

    /// Signal the owned child, retaining pages and resources until verified exit.
    pub(super) fn kill_child(&self) {
        self.kill_child_with(tokio::process::Child::start_kill);
    }

    pub(super) fn kill_child_with(
        &self,
        signal: impl FnOnce(&mut tokio::process::Child) -> std::io::Result<()>,
    ) {
        let child = self.child.lock().unwrap().take(); // unwrap: poisoned = prior panic while mutating the owned child
        if let Some(child) = child {
            let reservation = self.retained_page_dir.lock().take();
            retire_page_owner(child, reservation, signal);
        }
    }

    pub(super) fn check_owned_engine(
        &self,
        expected: &OwnedEngineIdentity,
    ) -> Result<(), EngineRetirementError> {
        if !expected.owner.ptr_eq(&Arc::downgrade(&self.child)) {
            return Err(EngineRetirementError::NotOwned);
        }
        let child = self.child.lock().unwrap(); // JUSTIFIED: poison means a prior panic while mutating the owned child.
        match child.as_ref() {
            None => Err(EngineRetirementError::NotOwned),
            Some(owned) if !owned.generation.same_engine(&expected.generation) => {
                Err(EngineRetirementError::StaleGeneration)
            }
            Some(_) => Ok(()),
        }
    }

    pub(super) async fn retire_owned_engine_with(
        &self,
        expected: &OwnedEngineIdentity,
        signal: impl FnOnce(&mut tokio::process::Child) -> std::io::Result<()> + Send,
    ) -> Result<EngineRetirementReceipt, EngineRetirementError> {
        self.check_owned_engine(expected)?;
        let endpoint = crate::inference::slots::directory().endpoint(&self.root);
        let transition = endpoint
            .transition_for_generation(&expected.generation)
            .await
            .map_err(|()| EngineRetirementError::StaleGeneration)?;
        self.retire_owned_engine_drained(expected, &transition, signal)
    }

    // Shared drained seam: a future save-all owner must pass its existing transition,
    // rather than reacquiring the endpoint writer through idle(). No awaits after take.
    pub(super) fn retire_owned_engine_drained(
        &self,
        expected: &OwnedEngineIdentity,
        transition: &crate::inference::slots::EndpointTransition,
        signal: impl FnOnce(&mut tokio::process::Child) -> std::io::Result<()>,
    ) -> Result<EngineRetirementReceipt, EngineRetirementError> {
        if !transition
            .previous_generation()
            .is_some_and(|g| g.same_engine(&expected.generation))
        {
            return Err(EngineRetirementError::StaleGeneration);
        }
        if !expected.owner.ptr_eq(&Arc::downgrade(&self.child)) {
            return Err(EngineRetirementError::NotOwned);
        }
        let owned = {
            let mut child = self.child.lock().unwrap(); // JUSTIFIED: poison means a prior panic while mutating the owned child.
            match child.as_ref() {
                None => return Err(EngineRetirementError::NotOwned),
                Some(owned) if !owned.generation.same_engine(&expected.generation) => {
                    return Err(EngineRetirementError::StaleGeneration);
                }
                Some(_) => child
                    .take()
                    .expect("matching child checked under the same lock"), // JUSTIFIED: Some was checked under this still-held child lock.
            }
        };
        let reservation = self.retained_page_dir.lock().take();
        retire_page_owner(owned, reservation, signal);
        Ok(EngineRetirementReceipt {
            identity: expected.clone(),
        })
    }

    /// Poll `/health` until the server answers 200, THEN prove the compute path
    /// with a one-token decode — only then is the lane truly ready. 503 means
    /// "still loading the model" — keep waiting. A connection error means "not up
    /// yet" early in the launch — also keep waiting, until the deadline. The
    /// decode check makes "ready" mean "can think", not merely "HTTP listener is
    /// up": a freshly-spawned server that answers `/health` 200 but cannot decode
    /// is a loud, novel failure surfaced as `NotReady` (→ Degraded) rather than a
    /// silently-adopted wedged lane. (This is the spawn-side guarantee that lets
    /// the adopt path in `ensure_model_serving` TRUST a child we own without
    /// re-probing every reconcile tick.)
    /// Non-blocking: has the child WE spawned already exited? `Some(status)` once
    /// it has, `None` while it runs (or when we own no child — the adopt path).
    /// Lets [`wait_ready`] fail LOUD the instant a bring-up dies instead of polling
    /// a dead port for the whole budget.
    /// Whether this handle spawned the process it waits on (an adopted lane has no child,
    /// so its liveness is unknown here and it never earns the loading allowance).
    pub(super) fn owns_child(&self) -> bool {
        self.child.lock().unwrap().is_some() // JUSTIFIED unwrap: poison means a prior panic while mutating this owned child.
    }

    pub(super) fn child_exit_status(&self) -> Option<std::process::ExitStatus> {
        let mut child = self.child.lock().unwrap(); // JUSTIFIED unwrap: poison means a prior panic while mutating this owned child.
        child.as_mut().and_then(|owned| {
            let status = owned.child.try_wait().ok().flatten();
            if status.is_some() {
                owned.generation.observed_exit();
            }
            status
        })
    }

    /// The tail of this lane's stderr log (`llama-server-<port>.log`, the file the
    /// spawn wires stderr to) — the llama.cpp load banner or ggml/Metal fault that
    /// explains a bring-up failure. Best-effort: an unreadable/absent log yields a
    /// marker, never blocks the error path. An EMPTY log is itself a signal — a
    /// child that produced no output before the deadline hung in early init (e.g.
    /// a co-resident second Metal context that never acquired the device), distinct
    /// from a crash (which prints a banner + fault first).
    pub(super) fn stderr_log_tail(&self) -> String {
        let Some(port) = super::port_of_base_url(&self.root) else {
            return "<lane port unparseable>".to_string();
        };
        let Some(path) = crate::inference::child_log::llama_server_log(port) else {
            return "<no home dir>".to_string();
        };
        match std::fs::read_to_string(&path) {
            Ok(s) => tail_or_hang_marker(&s),
            // The marker is the contract (never block the error path), but a bare
            // "unreadable" hides WHY — permissions vs absent vs mid-rotation.
            Err(e) => format!("<stderr log unreadable: {e}>"),
        }
    }
}


/// The last lines of an llama-server stderr log, OR — when the log is empty — a
/// diagnostic marker naming what an empty log MEANS: the child produced NO output
/// before it stopped. Read alongside the exit-status arm in `wait_ready`, that
/// disambiguates the two causes: an OS OOM/jetsam kill (SIGKILL/137 — the proven
/// cause when a second large model can't fit in free unified memory; the child
/// dies before llama.cpp prints its banner) vs. a genuine early-init hang (child
/// still running at the deadline). A crash from bad args or a model-load fault
/// prints the banner + fault FIRST, so a non-empty tail carries that directly.
/// Pure so the load-bearing empty-vs-tail decision is unit-tested without touching
/// the real `~/.continuum/logs` path ([[self-test-via-command-feedback-surface-
/// never-blind]], and #72's env-dependent-test lesson).
pub(super) fn tail_or_hang_marker(contents: &str) -> String {
    let mut lines: Vec<&str> = contents.lines().rev().take(20).collect();
    lines.reverse();
    let tail = lines.join("\n");
    if tail.trim().is_empty() {
        "<stderr log empty — child produced no output before it stopped: with a SIGKILL/137 \
         exit this is an OS OOM/jetsam kill (a second large model that didn't fit in free \
         memory); with no exit status it is a genuine early-init hang>"
            .to_string()
    } else {
        tail
    }
}


impl Default for LlamaServerProcess {
    fn default() -> Self {
        Self::new()
    }
}


impl Drop for LlamaServerProcess {
    fn drop(&mut self) {
        self.kill_child();
        // Signalling is not exit. Preserve the registry and reclaim pidfile;
        // their existing identity-aware cleanup owners retire stale records.
        // In particular, never clear a successor's canonical pidfile here.
    }
}

impl LlamaServerProcess {

    pub(super) fn record_verified_target(
        &self,
        generation: &crate::inference::slots::EngineGeneration,
        target: &ServingTarget,
        invocation: &crate::inference::lane_args::LaneInvocation,
        prepared: Option<Arc<PreparedLocalLaunch>>,
        engine_program: &str,
        context: u32,
        lanes: u32,
    ) {
        let mut child = self.child.lock().unwrap(); // JUSTIFIED: poison means prior panic mutating the owned child.
        if let Some(owned) = child
            .as_mut()
            .filter(|owned| owned.generation.same_engine(generation))
        {
            let prepared = prepared.map(|original| {
                let mut verified = original.as_ref().clone();
                verified.expected_geometry = Some((context, lanes));
                Arc::new(verified)
            });
            owned.verified_target = Some(OwnedServingTarget {
                prepared,
                identity: OwnedEngineIdentity {
                    owner: Arc::downgrade(&self.child),
                    generation: generation.clone(),
                },
                target: target.clone(),
                engine_program: engine_program.to_string(),
                invocation: invocation.clone(),
                observed_context_window: context,
                observed_lanes: lanes,
                launched_host_prompt_cache_mib: self
                    .launched_prompt_cache_mib
                    .load(std::sync::atomic::Ordering::Relaxed),
            });
        }
    }
}

