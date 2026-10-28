//! Publication ownership for the serving daemon.

use super::*;

/// Bus topic the live [`ServingSnapshot`] is emitted on whenever it changes.
/// Subscribers (embedding, supervisor, inference_session, ai_provider) declare
/// this in their `event_subscriptions` and cache the latest in `handle_event`
/// instead of probing the process — the cbar pipeline-stage shape: one organ
/// emits its state, everything that needs it subscribes. Because the bus spans
/// the grid, a remote lease allocator subscribes to the SAME topic. Routed by
/// name (no body parse in middleware), payload fans out as a shared pointer,
/// emitted only on a rare state change — never on the token hot path.
pub(super) const SERVING_SNAPSHOT_EVENT: &str = "serving.snapshot";

/// Exact local launch provenance acknowledged under one serving intent revision.
/// Eligibility is queried from the daemon; this receipt alone is not a loan or lease.
#[derive(Clone)]
pub struct VerifiedServingCapture {
    pub launch: crate::inference::llama_server::OwnedServingTarget,
    pub intent_revision: u64,
}

pub(super) fn verified_capture(
    server: &dyn LlamaServerControl,
    snapshot: &ServingSnapshot,
    intent_revision: u64,
) -> Option<VerifiedServingCapture> {
    if !snapshot.ready || server.paging_recovery_required() {
        return None;
    }
    let launch = server.owned_serving_target()?;
    if snapshot.active_model.as_deref() != Some(launch.target.model.id.as_str())
        || snapshot.adapters != launch.target.adapter_paths()
        || snapshot.served_context_window != launch.observed_context_window
        || snapshot.lanes != launch.observed_lanes
        || snapshot.host_prompt_cache_mib != launch.launched_host_prompt_cache_mib
        || server.owned_engine()? != launch.identity
    {
        return None;
    }
    Some(VerifiedServingCapture {
        launch,
        intent_revision,
    })
}

/// PURE: the lane count a ready snapshot publishes — the engine's `/props total_slots`
/// when it names one, else what was asked of it. An engine adopted with MORE slots than
/// the plan wants (a down-plan is the daemon's sticky choice; only growth relaunches)
/// still serves every slot it has: that is the residency the board charges and the
/// ambient budget the citizens get.
pub fn published_lanes(served: Option<u32>, target_lanes: u32) -> u32 {
    served.filter(|n| *n > 0).unwrap_or(target_lanes)
}

pub(super) fn snapshot_from_outcome(
    outcome: &EnsureOutcome,
    desired: &str,
    adapters: &[String],
    served_context_window: u32,
    lanes: u32,
    // The `--cache-ram` the running engine was launched with (a fact off the control;
    // 0 = unknown). Rides the snapshot so the measurement and the board charge the
    // engine's grant, never the plan's latest derivation.
    host_prompt_cache_mib: u32,
    // The VERIFIED vision endpoint on this node, if any (#106): the main lane
    // itself when its model sees, or the sidecar lane. `Some` ⇒ its `/props`
    // confirmed sight; the snapshot's `vision_ready`/`vision_base_url`/
    // `vision_model` are all projected from this ONE value, so an address can
    // never be published without the verified flag (or vice versa).
    vision: Option<crate::inference::vision_sidecar::SidecarLane>,
) -> ServingSnapshot {
    match outcome {
        EnsureOutcome::AlreadyServing | EnsureOutcome::Spawned { .. }
            if served_context_window > 0 =>
        {
            ServingSnapshot {
                // A lane with a verified window is serving, not loading.
                loading_model: None,
                held_for: None,
                // The claim carries the age of its evidence. Stamped HERE, at the
                // moment the reconcile confirmed readiness — not at read time, which
                // is what let `ready:true` survive a SIGKILLed process for as long as
                // nobody republished. A reader compares this against now and decides
                // whether the claim is still worth trusting.
                //
                // `.ok()`, NOT `.unwrap_or(0)`: if the clock is before the epoch we
                // cannot attest to WHEN this was verified, and `Some(0)` would be a
                // fabricated attestation (verified in 1970) rather than an absent one.
                // `None` already means "never confirmed" in this field's contract, so
                // the honest answer is available — take it. Inventing a plausible
                // number is the worse half of swallowing an error: it doesn't just
                // hide the failure, it feeds fiction to every downstream reader.
                ready_verified_at_ms: continuum_base::time::EpochMs::read()
                    .ok()
                    .map(continuum_base::time::EpochMs::millis),
                active_model: Some(desired.to_string()),
                ready: true,
                base_url: serving_v1_url(),
                // The genome set now live — feeds the next reconcile's relaunch
                // guard and gives readers the active genome without probing.
                adapters: adapters.to_vec(),
                // The real per-slot window the process serves — every persona
                // budgets its prompt to THIS (task #50, the drift fix).
                served_context_window,
                // The `--parallel` slot count — lets a reader (the resource
                // authority's footprint(), a grid allocator) charge total resident
                // KV as `lanes × kv_at(served_context_window)` (#79).
                lanes,
                host_prompt_cache_mib,
                degraded_reason: None,
                // The reconcile's verified multimodal verdict (#106): the ONE
                // `vision` value projects all three fields, so readiness and
                // the routing address can never disagree. The observe path
                // gates on THIS, never on a row's declaration alone.
                vision_ready: vision.is_some(),
                vision_base_url: vision.as_ref().map(|v| v.base_url.clone()),
                vision_model: vision.map(|v| v.model_id),
            }
        }
        // Ready outcome but the served window was unreadable (0) → do NOT publish
        // a ready snapshot with a zero window; that would poison every binding
        // persona's budget. Publish "nothing live"; the server stays up and the
        // next reconcile re-reads /props.
        // LOADING. The lane is up (spawned or already serving) but `/props` has not yet
        // given us a real window, so it is not READY. This arm used to return
        // `ServingSnapshot::empty()` and DISCARD `desired` — throwing away the only
        // knowledge of what is being brought up, for the whole load window. Measured on a
        // cold boot 2026-08-19: physical climbed 29.90 → 36.88 GB while serving attributed
        // 0.00 GB, because the consumer had nothing to name. Those bytes read as unowned,
        // and a plan computed in that window sizes against a machine that looks two thirds
        // full of someone else's memory.
        //
        // The bytes exist from spawn, not from readiness. Name the model so the consumer
        // can charge them.
        EnsureOutcome::AlreadyServing | EnsureOutcome::Spawned { .. } => ServingSnapshot {
            loading_model: Some(desired.to_string()),
            ..ServingSnapshot::empty()
        },
        // A Degraded reconcile PUBLISHES its reason — the spawn/probe failure
        // (e.g. a missing llama-server binary, its path in the text) reaches
        // `serving/status` instead of dying as an anonymous empty snapshot
        // (live repro 2026-07-24: Windows spawn failed every tick, status
        // showed null/false with no why).
        EnsureOutcome::Degraded { reason } => ServingSnapshot::degraded(reason.clone()),
        EnsureOutcome::Superseded => ServingSnapshot::degraded("serving intent superseded".into()),
    }
}

impl ServingDaemonModule {
    /// Emit the live snapshot on the bus. Routed by topic name (cheap match, no
    /// body parse in middleware); the payload fans out as a shared pointer. A
    /// no-op when the bus isn't set (tests). The watch is the in-process
    /// materialized view; THIS is the canonical fan-out that reaches every
    /// subscriber, including a grid allocator on a remote node.
    pub(super) fn emit_serving(bus: Option<&Arc<MessageBus>>, snapshot: &ServingSnapshot) {
        // Every publish passes here: a ready one answers every condemnation so far, so
        // `engine_condemned` reads the condemnation, never the readiness bit (card 9c67e5fe).
        // INVARIANT this relies on: every READY publish runs inside a `ServingOperation`, holding
        // the one reconcile gate (`reconcile_to_plan`, the academy batch), and `condemn()` runs
        // inside an operation's `admit`. So no stale ready of a condemned engine can land between
        // a condemnation and that operation's own outcome. A ready publish added OUTSIDE the gate
        // must instead answer only a ready from a new engine incarnation (Fable on #5508).
        if snapshot.ready {
            crate::inference::llama_server::note_serving_ready();
        }
        if let Some(bus) = bus {
            if let Ok(payload) = serde_json::to_value(snapshot) {
                bus.publish_async_only(SERVING_SNAPSHOT_EVENT, payload);
            }
        }
    }

    /// Subscribe to the published serving plan. Consumers (scheduler, spawner)
    /// hold the receiver and react to plan changes — the ebb/flow seam.
    pub fn subscribe(&self) -> watch::Receiver<Option<ServingPlan>> {
        self.plan_view_tx.subscribe()
    }

    /// A current, ready, owned launch, preserving its original requested target.
    /// Unknown adopted origins and superseded/in-flight operations are ineligible.
    pub fn verified_serving_target(&self) -> Option<VerifiedServingCapture> {
        if self.reconciling.load(Ordering::Acquire) {
            return None;
        }
        let capture = self.verified_target.borrow().clone()?;
        if capture.intent_revision != self.intent.snapshot().revision {
            return None;
        }
        let current = verified_capture(
            self.server.as_ref(),
            &self.serving_tx.borrow(),
            capture.intent_revision,
        )?;
        (current.launch.identity == capture.launch.identity).then_some(capture)
    }

    pub(super) fn acknowledge_verified_target(&self, live: &ServingSnapshot, intent_revision: u64) {
        if self.intent.snapshot().revision == intent_revision {
            let _ = self.verified_target.send_replace(verified_capture(
                self.server.as_ref(),
                live,
                intent_revision,
            ));
        }
    }

    /// Admission commits physical unavailability before the first lifecycle await.
    /// Keeping the incumbent/loading identities preserves resource attribution;
    /// cancellation can no longer leave an old ready snapshot blocking recovery.
    pub(super) fn publish_lifecycle_admission(
        serving: &watch::Sender<ServingSnapshot>,
        verified: &watch::Sender<Option<VerifiedServingCapture>>,
        bus: Option<&Arc<MessageBus>>,
        loading: Option<&str>,
    ) {
        let mut snapshot = serving.borrow().clone();
        snapshot.ready = false;
        snapshot.ready_verified_at_ms = None;
        snapshot.loading_model = loading.map(str::to_string);
        snapshot.degraded_reason = Some("serving lifecycle transition admitted".into());
        let _ = verified.send_replace(None);
        Self::emit_serving(bus, &snapshot);
        let _ = serving.send_replace(snapshot);
        // THE condemnation: every transition admitted here drains the endpoint's admitted
        // turns (`transition_if`), so acts in flight on it end now instead of holding the
        // drain to their next checkpoint (card 4d2a64a2). A transient not-ready publish (a
        // 0 `/props` window under AlreadyServing) never reaches here (Fable on #5500).
        crate::inference::llama_server::condemn("serving lifecycle transition admitted");
    }

    /// Subscribe to the published serving SNAPSHOT — the live `(active_model,
    /// ready, base_url)` state. Inference adapters hold this and point at
    /// `base_url` once `ready`; a grid allocator holds it to contract leases.
    pub fn subscribe_serving(&self) -> watch::Receiver<ServingSnapshot> {
        self.serving_tx.subscribe()
    }

    /// #287 slice 2 — the governed host-cache lease, derived LIVE on the tick and
    /// published to the per-port plan file her `ResidencyCache` mtime-polls. This is
    /// the loop-closer that retires the `GGML_MOE_HOST_CACHE_GB=40` scratchpad
    /// constant: the budget is arithmetic over the serve's REAL working set
    /// ([`host_cache_lease_bytes`](crate::capacity::host_cache_lease::host_cache_lease_bytes)),
    /// re-derived every tick because KV grows as slots fill, and published through
    /// the sticky band so sub-band flutter never churns the file. Publishes only for
    /// a READY MoE serve (the moe context exists for the active model) — a dense
    /// model or a warming lane writes nothing.
    /// Track the resident model's on-disk artifact in the process-wide
    /// active set (#302 invariant 1). Registered BEFORE spawn (the pool
    /// must not migrate a GGUF mid-load), swapped on model change (old
    /// registration released exactly once), cleared when nothing serves.
    /// Prefix containment in [`ActiveArtifactSet::protects`] means the
    /// file path protects its per-model dir and vice versa, whichever
    /// layout the artifact uses.
    pub(super) fn set_active_artifact(&self, path: Option<std::path::PathBuf>) {
        Self::set_active_artifact_in(&self.active_artifact, path, None);
    }

    pub(super) fn set_active_artifact_in(
        active: &std::sync::Mutex<Option<std::path::PathBuf>>,
        path: Option<std::path::PathBuf>,
        physical: Option<std::path::PathBuf>,
    ) {
        let Ok(mut current) = active.lock() else {
            return; // poisoned: the set fails SAFE (protects everything)
        };

        let set = crate::system_resources::serving_active_artifacts();
        set.replace(current.as_deref(), path.clone().map(|raw| (raw, physical)));
        *current = path;
    }
}
