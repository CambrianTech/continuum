//! The grid-peer trainer (card e53bb821, S2b-2b of 6f05b89d): a node with no trainer for a
//! base leases its LoRA job to ONE grid peer that advertises it, so its residents still learn.
//!
//! It is one more [`FineTuningAdapter`] behind the coordinator, ranked [`TrainingPlace::GridPeer`]
//! (after the resident weights here, before a separate local copy). The origin keeps every
//! training decision; `genome/job-create` asks this trainer for its destination ONCE, records it
//! with the attempt, and hands it back through [`FineTuningAdapter::create_bound`], which sends
//! to exactly that peer and never chooses again. The peer runs the execute-only
//! `genome/lease/train` (card cc34db0d). Poll and cancel go to the same peer by attempt;
//! `Cancelled` there means the peer accepted the cancel, never that training was seen to stop.
//!
//! Boot registers it; it leases only when BOTH this node and the peer emit
//! `training_lease_return`. The request is recorded in the job's own directory before the
//! lease is sent; a completed lease's adapter is pulled, verified and placed there
//! ([`LeasePull`], card 92cd2251), so the completion sentinel adopts it and the examiner sits
//! it on held-out work as for any local job.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;

use super::lease::{LeaseCancelOutcome, LeasePollOutcome, LeaseStatusParams, LeaseTrainOutcome, LeaseTrainParams};
use super::lease_pull::LeasePull;
use crate::capacity::gossip::{CapabilityId, CapacityOffer};
use crate::genome::fine_tuning::adapter::{FineTuningCapabilities, TrainerHardware, TrainingPlace};
use crate::genome::fine_tuning::{
    Destination, FineTuningAdapter, FineTuningError, JobBinding, JobHandle, TrainingJobRequest, TrainingStatus,
};

/// This trainer's provider id: the one name a grid lease's job handle carries.
pub(crate) const GRID_PEER_PROVIDER: &str = "grid-peer";

/// How one command reaches one peer and comes back. Production rides the executor's airc
/// route (`airc://{peer}/{command}`); a test drives the real receiver commands in process.
#[async_trait]
pub trait LeaseTransport: Send + Sync {
    async fn call(&self, peer: Uuid, command: &str, params: Value) -> Result<Value, String>;
}

/// The production transport: the core's own executor, over airc, to one peer.
pub(crate) struct ExecutorLeaseTransport(pub(crate) Arc<crate::runtime::LateBound<crate::runtime::CommandExecutor>>);

#[async_trait]
impl LeaseTransport for ExecutorLeaseTransport {
    async fn call(&self, peer: Uuid, command: &str, params: Value) -> Result<Value, String> {
        let executor = self.0.require()?.clone();
        let uri = crate::routing::CommandUri::parse(&format!("airc://{peer}/{command}")).map_err(|e| format!("{e:?}"))?;
        executor.execute_json(uri, params).await
    }
}

/// What the grid currently offers: (peer, offer, heard-at ms by this node's clock).
pub(crate) type OfferSource = Arc<dyn Fn() -> Vec<(Uuid, CapacityOffer, u64)> + Send + Sync>;

pub struct GridPeerAdapter {
    transport: Arc<dyn LeaseTransport>,
    offers: OfferSource,
    now_ms: Arc<dyn Fn() -> u64 + Send + Sync>,
    /// Whether THIS node can take a leased job's adapter back (it emits
    /// `training_lease_return` itself). A lease needs the return on BOTH ends: an origin
    /// without it never leases, whatever a newer peer advertises (Codex on #5417).
    returns_here: bool,
    /// Brings a completed lease's adapter back into this node's own job directory.
    pull: Arc<LeasePull>,
}

impl GridPeerAdapter {
    /// The production trainer: the executor's airc route and the gossip ledger's heard offers.
    pub fn on_grid(executor: Arc<crate::runtime::LateBound<crate::runtime::CommandExecutor>>) -> Self {
        let transport: Arc<dyn LeaseTransport> = Arc::new(ExecutorLeaseTransport(executor));
        Self::new(
            transport.clone(),
            Arc::new(|| crate::capacity::gossip::global_ledger().foreign_offers_with_age()),
            Arc::new(|| continuum_base::time::EpochMs::now().millis()),
            crate::capacity::gossip::this_build_capabilities().contains(&CapabilityId::TrainingLeaseReturn),
            Arc::new(LeasePull::on(transport)),
        )
    }

    pub(crate) fn new(
        transport: Arc<dyn LeaseTransport>,
        offers: OfferSource,
        now_ms: Arc<dyn Fn() -> u64 + Send + Sync>,
        returns_here: bool,
        pull: Arc<LeasePull>,
    ) -> Self {
        Self { transport, offers, now_ms, returns_here, pull }
    }

    /// The ONE peer a job for `base` goes to: a fresh offer (heard within the freshness
    /// window) that lists `base` as trainable AND advertises both `training_lease` and
    /// `training_lease_return` (its adapter can come back); ties by peer id.
    /// Never a union across peers: the chosen offer satisfies the whole request by itself.
    pub(crate) fn choose_peer(&self, base: &str) -> Option<Uuid> {
        if !self.returns_here {
            return None;
        }
        let now = (self.now_ms)();
        (self.offers)()
            .into_iter()
            .filter(|(_, offer, heard)| {
                now.saturating_sub(*heard) <= crate::capacity::gossip::FRESHNESS_WINDOW_MS
                    && offer.trainable_bases.iter().any(|b| b == base)
                    && offer.capabilities.contains(&CapabilityId::TrainingLease)
                    && offer.capabilities.contains(&CapabilityId::TrainingLeaseReturn)
            })
            .map(|(peer, _, _)| peer)
            .min()
    }

    async fn ask(&self, peer: Uuid, command: &str, params: Value) -> Result<Value, FineTuningError> {
        self.transport
            .call(peer, command, params)
            .await
            .map_err(|e| FineTuningError::Transient(format!("{command} on peer {peer}: {e}")))
    }
}

/// A grid lease's handle names its peer and the origin's attempt, so poll, cancel and a
/// restarted core reach the same job without any other state.
pub(crate) fn lease_handle(peer: Uuid, attempt: Uuid) -> JobHandle {
    JobHandle { provider_id: GRID_PEER_PROVIDER.into(), provider_job_id: format!("{peer}/{attempt}"), local_id: attempt }
}

fn handle_lease(handle: &JobHandle) -> Option<(Uuid, Uuid)> {
    let (peer, attempt) = handle.provider_job_id.split_once('/')?;
    Some((Uuid::parse_str(peer).ok()?, Uuid::parse_str(attempt).ok()?))
}

fn decode<T: serde::de::DeserializeOwned>(command: &str, value: Value) -> Result<T, FineTuningError> {
    serde_json::from_value(value) // lease wire boundary: the peer's reply arrives as JSON over airc
        .map_err(|e| FineTuningError::MalformedResponse(format!("{command}: {e}")))
}

#[async_trait]
impl FineTuningAdapter for GridPeerAdapter {
    fn capabilities(&self) -> FineTuningCapabilities {
        FineTuningCapabilities {
            provider_id: GRID_PEER_PROVIDER.into(),
            supports_lora: true,
            supports_validation: false,
            produces_local_artifact: false,
            // Any base, gated live by `serves_base` against what peers advertise.
            supported_base_model_prefixes: Vec::new(),
            requires: TrainerHardware::Any,
            place: TrainingPlace::GridPeer,
        }
    }

    fn serves_base(&self, base_model: &str) -> bool {
        self.choose_peer(base_model).is_some()
    }

    fn destination(&self, request: &TrainingJobRequest) -> Result<Destination, FineTuningError> {
        self.choose_peer(&request.base_model)
            .map(Destination::Peer)
            .ok_or_else(|| FineTuningError::ProviderRejected(format!("no fresh grid peer leases training for {}", request.base_model)))
    }

    async fn create_job(&self, _request: TrainingJobRequest) -> Result<JobHandle, FineTuningError> {
        Err(FineTuningError::InvalidRequest("a grid lease is sent under its recorded binding (create_bound), never chosen at create".into()))
    }

    async fn create_bound(&self, request: TrainingJobRequest, binding: &JobBinding) -> Result<JobHandle, FineTuningError> {
        let (Destination::Peer(peer), Some(attempt)) = (&binding.destination, binding.attempt) else {
            return Err(FineTuningError::InvalidRequest("a grid lease needs a peer destination and an attempt".into()));
        };
        // The request is this node's before it is anyone else's: recorded durably in the job's
        // own directory first, where the adapter will land and the examiner reads what it
        // trained on. Unrecorded, nothing is sent.
        self.pull
            .record_request(*peer, attempt, &request)
            .map_err(|e| FineTuningError::TrainerUnavailable(format!("the lease's request could not be recorded here, nothing sent: {e}")))?;
        let (persona, base) = (request.persona_name.clone(), request.base_model.clone());
        let params = serde_json::to_value(LeaseTrainParams { attempt, dispatch: binding.dispatch, request }) // lease wire boundary: the request crosses the grid as JSON
            .map_err(|e| FineTuningError::InvalidRequest(e.to_string()))?;
        match decode::<LeaseTrainOutcome>("genome/lease/train", self.ask(*peer, "genome/lease/train", params).await?)? {
            LeaseTrainOutcome::Created { .. } => {
                continuum_base::probe!(
                    class = "genome.lease.sent",
                    peer = %peer,
                    attempt = %attempt,
                    persona = %persona,
                    base = %base,
                    "a LoRA job is leased to a grid peer: it trains there and its adapter comes back here"
                );
                Ok(lease_handle(*peer, attempt))
            }
            // The peer started nothing: a refusal settles the attempt (it may be retried).
            LeaseTrainOutcome::Refused { error_kind, error } => {
                Err(FineTuningError::ProviderRejected(format!("peer {peer} refused the lease ({error_kind}): {error}")))
            }
            // A trainer may have started there: never read as "not created".
            LeaseTrainOutcome::Unknown { reason } => Err(FineTuningError::Transient(format!("peer {peer}: {reason}"))),
        }
    }

    async fn poll(&self, handle: &JobHandle) -> Result<TrainingStatus, FineTuningError> {
        let (peer, attempt) = handle_lease(handle).ok_or_else(|| FineTuningError::UnknownHandle(handle.clone()))?;
        let params = serde_json::to_value(LeaseStatusParams { attempt }).map_err(|e| FineTuningError::InvalidRequest(e.to_string()))?; // lease wire boundary
        match decode::<LeasePollOutcome>("genome/lease/poll", self.ask(peer, "genome/lease/poll", params).await?)? {
            LeasePollOutcome::Status { status } => Ok(status),
            // Finished there: pulled, verified and placed here before it reads as completed.
            LeasePollOutcome::Completed { artifact } => Ok(self.pull.status(peer, attempt, artifact)),
            LeasePollOutcome::Unreturnable { reason } => Ok(TrainingStatus::Failed { error: format!("peer {peer}: {reason}") }),
            LeasePollOutcome::NotCreated => Err(FineTuningError::UnknownHandle(handle.clone())),
            LeasePollOutcome::Unknown { reason } => Err(FineTuningError::Transient(format!("peer {peer}: {reason}"))),
        }
    }

    async fn cancel(&self, handle: &JobHandle) -> Result<(), FineTuningError> {
        let (peer, attempt) = handle_lease(handle).ok_or_else(|| FineTuningError::UnknownHandle(handle.clone()))?;
        let params = serde_json::to_value(LeaseStatusParams { attempt }).map_err(|e| FineTuningError::InvalidRequest(e.to_string()))?; // lease wire boundary
        match decode::<LeaseCancelOutcome>("genome/lease/cancel", self.ask(peer, "genome/lease/cancel", params).await?)? {
            // Accepted by the peer, not observed stopped: poll keeps reporting what it sees.
            LeaseCancelOutcome::Cancelled | LeaseCancelOutcome::NotCreated => Ok(()),
            LeaseCancelOutcome::Unknown { reason } => Err(FineTuningError::Transient(format!("peer {peer}: {reason}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::genome::attempt_ledger::AttemptLedger;
    use crate::commands::genome::lease::{GenomeLeaseArtifact, GenomeLeaseCancel, GenomeLeasePoll, GenomeLeaseStatus, GenomeLeaseTrain};
    use crate::genome::fine_tuning::coordinator::FineTuningCoordinator;
    use crate::genome::fine_tuning::{FineTuningRegistry, RecordingFineTuningAdapter, RECORDING_BASE_PREFIX};
    use crate::sdk_codegen::{ActionCommand, Ctx};
    use std::sync::atomic::{AtomicBool, Ordering};

    async fn ledger() -> (Arc<AttemptLedger>, tempfile::TempDir) {
        let (adapter, dir) = crate::orm::store::fresh_adapter().await;
        let ledger = AttemptLedger::new();
        ledger.install(Arc::new(crate::orm::OrmStore::new(adapter).await.expect("attempt store"))); // JUSTIFIED: test fixture
        (Arc::new(ledger), dir)
    }

    /// A grid peer in process: the REAL receiver commands over its own ledger and an on-host
    /// trainer, called as the origin's authenticated airc identity. `drop_reply` loses the next
    /// reply AFTER the peer acted, as a lost acknowledgement does.
    struct InProcessPeer {
        origin: Uuid,
        train: GenomeLeaseTrain,
        status: GenomeLeaseStatus,
        poll: GenomeLeasePoll,
        cancel: GenomeLeaseCancel,
        artifact: GenomeLeaseArtifact,
        drop_reply: AtomicBool,
        /// Loses the `genome/lease/artifact` reply for the range at this offset, once, after the
        /// peer read it.
        drop_range_at: std::sync::Mutex<Option<u64>>,
        /// The offset of every range asked, so a resumed pull is told from a restarted one.
        ranges: std::sync::Mutex<Vec<u64>>,
        /// Every peer a call was addressed to, so a call to the wrong peer is caught.
        called: std::sync::Mutex<Vec<Uuid>>,
    }

    impl InProcessPeer {
        fn new(origin: Uuid, attempts: Arc<AttemptLedger>) -> Self {
            Self::with_trainer(origin, attempts, RecordingFineTuningAdapter::new())
        }
        /// A peer whose on-host trainer is `trainer` (one that leaves real files, for the return).
        fn with_trainer(origin: Uuid, attempts: Arc<AttemptLedger>, trainer: RecordingFineTuningAdapter) -> Self {
            let registry = Arc::new(FineTuningRegistry::new());
            registry.register(Arc::new(trainer));
            Self {
                origin,
                train: GenomeLeaseTrain { coordinator: Arc::new(FineTuningCoordinator::new(registry.clone())), attempts: attempts.clone() },
                status: GenomeLeaseStatus { attempts: attempts.clone() },
                poll: GenomeLeasePoll { attempts: attempts.clone(), registry: registry.clone() },
                artifact: GenomeLeaseArtifact { attempts: attempts.clone() },
                cancel: GenomeLeaseCancel { attempts, registry },
                drop_reply: AtomicBool::new(false),
                drop_range_at: std::sync::Mutex::new(None),
                ranges: std::sync::Mutex::new(Vec::new()),
                called: std::sync::Mutex::new(Vec::new()),
            }
        }
        fn as_origin(&self, origin: Uuid) -> Ctx {
            let mut ctx = Ctx::default();
            ctx.caller = Some(crate::routing::CallerIdentity::airc(crate::identity::PeerId::from_uuid(origin)));
            ctx
        }
    }

    #[async_trait]
    impl LeaseTransport for InProcessPeer {
        async fn call(&self, peer: Uuid, command: &str, params: Value) -> Result<Value, String> {
            self.called.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(peer); // unwrap_or_else: a test fixture's poisoned log still reads
            let asked_offset = params.get("offset").and_then(Value::as_u64);
            let ctx = self.as_origin(self.origin);
            let err = |e: crate::sdk_codegen::CommandError| format!("{e:?}");
            let reply = match command {
                "genome/lease/train" => serde_json::to_value(self.train.run(&ctx, serde_json::from_value(params).map_err(|e| e.to_string())?).await.map_err(err)?),
                "genome/lease/status" => serde_json::to_value(self.status.run(&ctx, serde_json::from_value(params).map_err(|e| e.to_string())?).await.map_err(err)?),
                "genome/lease/poll" => serde_json::to_value(self.poll.run(&ctx, serde_json::from_value(params).map_err(|e| e.to_string())?).await.map_err(err)?),
                "genome/lease/cancel" => serde_json::to_value(self.cancel.run(&ctx, serde_json::from_value(params).map_err(|e| e.to_string())?).await.map_err(err)?),
                "genome/lease/artifact" => serde_json::to_value(self.artifact.run(&ctx, serde_json::from_value(params).map_err(|e| e.to_string())?).await.map_err(err)?),
                other => return Err(format!("no such lease verb {other}")),
            }
            .map_err(|e| e.to_string())?;
            if command == "genome/lease/artifact" {
                if let Some(offset) = asked_offset {
                    self.ranges.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(offset); // unwrap_or_else: a test fixture's poisoned log still reads
                }
                let mut drop_at = self.drop_range_at.lock().unwrap_or_else(std::sync::PoisonError::into_inner); // unwrap_or_else: a test fixture's poisoned flag still reads
                if asked_offset.is_some() && *drop_at == asked_offset {
                    *drop_at = None;
                    return Err("the range reply was lost after the peer read it".into());
                }
            }
            if self.drop_reply.swap(false, Ordering::SeqCst) {
                return Err("the reply was lost after the peer acted".into());
            }
            Ok(reply)
        }
    }

    fn offer(bases: &[&str], lease: bool) -> CapacityOffer {
        CapacityOffer {
            origin: Uuid::nil(),
            gpu_total_bytes: 0,
            gpu_free_bytes_live: 0,
            system_ram_free_bytes: 0,
            at_ms: 0,
            build: 0,
            build_number: 0,
            served_model: None,
            lanes: 0,
            residents: 0,
            free_slots_live: 0,
            lane_wait_p50_ms: 0,
            lane_wait_samples: 0,
            served_context_window: 0,
            decode_tps_milli: 0,
            capabilities: if lease { vec![CapabilityId::TrainingLease, CapabilityId::TrainingLeaseReturn] } else { Vec::new() },
            trainable_bases: bases.iter().map(|b| b.to_string()).collect(),
            resident_alarms: Vec::new(),
        }
    }

    fn lora_request() -> TrainingJobRequest {
        let mut request = crate::commands::genome::test_support::request_for(RECORDING_BASE_PREFIX);
        request.lora = Some(crate::genome::fine_tuning::LoRAHyperparams { rank: 8, alpha: 16, dropout: 0.0, target_modules: vec![], top_layers: None });
        request
    }

    /// The origin's pull over `transport`, under the genome root `root`, whose own base hashes to
    /// `base_sha`.
    fn pull_at(transport: Arc<dyn LeaseTransport>, root: &std::path::Path, base_sha: &str) -> Arc<LeasePull> {
        let (root, base_sha) = (root.to_path_buf(), base_sha.to_string());
        Arc::new(LeasePull::new(
            Arc::new(super::super::lease_pull::TransportSource(transport)),
            Arc::new(move |_: &str| Ok(base_sha.clone())),
            Arc::new(move || Some(root.clone())),
        ))
    }

    fn adapter_over(transport: Arc<dyn LeaseTransport>, offers: Vec<(Uuid, CapacityOffer, u64)>, now: u64, root: &std::path::Path) -> GridPeerAdapter {
        let pull = pull_at(transport.clone(), root, "no-base");
        GridPeerAdapter::new(transport, Arc::new(move || offers.clone()), Arc::new(move || now), true, pull)
    }

    // what this catches (card e53bb821): a lease sent to a peer that cannot take it or cannot
    // return its adapter, or to a union of peers. Only a FRESH offer listing the base AND
    // advertising training_lease AND training_lease_return is a candidate, ties go to the
    // lowest peer id, and with none the destination is refused.
    #[test]
    fn a_lease_goes_to_one_fresh_peer_that_advertises_the_base_and_the_capability() {
        let (low, high, stale, blind, other) = (Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(5), Uuid::from_u128(3), Uuid::from_u128(4));
        let now = 1_000_000;
        let window = crate::capacity::gossip::FRESHNESS_WINDOW_MS;
        let transport: Arc<dyn LeaseTransport> = Arc::new(ExecutorLeaseTransport(Arc::new(crate::runtime::LateBound::new("unused"))));
        let mut no_return = offer(&["qwen"], true);
        no_return.capabilities = vec![CapabilityId::TrainingLease];
        let genome = tempfile::tempdir().expect("genome root"); // JUSTIFIED: test fixture
        let adapter = adapter_over(transport.clone(), vec![
            (Uuid::from_u128(0), no_return, now),
            (high, offer(&["qwen"], true), now),
            (low, offer(&["qwen"], true), now - window),
            (stale, offer(&["qwen"], true), now - window - 1),
            (blind, offer(&["qwen"], false), now),
            (other, offer(&["llama"], true), now),
        ], now, genome.path());
        assert_eq!(adapter.choose_peer("qwen"), Some(low), "fresh, both capabilities, lowest id; the stale, the blind and the one whose adapter cannot return are not candidates");
        assert_eq!(adapter.choose_peer("mistral"), None);
        // An origin that cannot take an adapter back never leases, whatever peers advertise.
        let offers_now = vec![(low, offer(&["qwen"], true), now)];
        let no_return_here = GridPeerAdapter::new(transport.clone(), Arc::new(move || offers_now.clone()), Arc::new(move || now), false, pull_at(transport.clone(), genome.path(), "no-base"));
        assert_eq!(no_return_here.choose_peer("qwen"), None, "the return is needed on both ends");
        assert!(!adapter.serves_base("mistral"));
        let mut request = lora_request();
        request.base_model = "mistral".into();
        assert!(matches!(adapter.destination(&request), Err(FineTuningError::ProviderRejected(_))));
    }

    // what this catches (Codex's acceptance for S2b-2b): the lease's whole life over the real
    // receiver: created, polled, cancelled (accepted), every call addressed to the bound peer; a
    // changed payload under the same attempt refused; a new adapter instance (the origin's
    // in-memory state gone, the stores kept) reaching the same job from its handle alone; a
    // forged origin reaching nothing.
    #[tokio::test]
    async fn a_lease_lives_on_its_peer_and_only_its_origin_reaches_it() {
        let (peer_ledger, _dir) = ledger().await;
        let (origin, peer) = (Uuid::new_v4(), Uuid::new_v4());
        let transport = Arc::new(InProcessPeer::new(origin, peer_ledger.clone()));
        let genome = tempfile::tempdir().expect("genome root"); // JUSTIFIED: test fixture
        let adapter = adapter_over(transport.clone(), vec![(peer, offer(&[RECORDING_BASE_PREFIX], true), 10)], 10, genome.path());
        let attempt = Uuid::new_v4();
        let binding = JobBinding { attempt: Some(attempt), dispatch: None, destination: Destination::Peer(peer) };

        let handle = adapter.create_bound(lora_request(), &binding).await.expect("the peer took the lease"); // JUSTIFIED: test asserts the Ok arm
        assert_eq!(handle, lease_handle(peer, attempt));
        // The fixture's trainer keeps no local weights: the peer reports it unreturnable, final.
        assert!(matches!(adapter.poll(&handle).await, Ok(TrainingStatus::Failed { error }) if error.contains("no local weights to return")));
        assert!(adapter.cancel(&handle).await.is_ok(), "accepted by the peer");

        let mut changed = lora_request();
        changed.persona_name = "someone-else".into();
        // Refused here, before anything is sent: the attempt's recorded request is never rewritten.
        let calls = transport.called.lock().unwrap_or_else(std::sync::PoisonError::into_inner).len(); // unwrap_or_else: a test fixture's poisoned log still reads
        assert!(matches!(adapter.create_bound(changed, &binding).await, Err(FineTuningError::TrainerUnavailable(e)) if e.contains("nothing sent")));
        assert_eq!(transport.called.lock().unwrap_or_else(std::sync::PoisonError::into_inner).len(), calls, "nothing was sent"); // unwrap_or_else: a test fixture's poisoned log still reads

        // A new adapter instance (the origin's in-memory state gone), over the same stores.
        let restarted = adapter_over(transport.clone(), Vec::new(), 10, genome.path());
        assert!(matches!(restarted.poll(&handle).await, Ok(TrainingStatus::Failed { .. })), "the handle alone reaches the job");
        assert!(
            transport.called.lock().unwrap_or_else(std::sync::PoisonError::into_inner).iter().all(|to| *to == peer), // unwrap_or_else: a test fixture's poisoned log still reads
            "create, poll and cancel went only to the bound peer"
        );

        let forger = adapter_over(Arc::new(InProcessPeer::new(Uuid::new_v4(), peer_ledger)), Vec::new(), 10, genome.path());
        assert!(matches!(forger.poll(&handle).await, Err(FineTuningError::UnknownHandle(_))), "a forged origin reaches nothing");
    }

    // what this catches (Codex's acceptance for S2b-2b): a lost create acknowledgement read as
    // "not created", and an attempt the peer never received left open. The origin's ledger holds
    // the attempt as admitted to the peer; reconciliation asks the peer: a job made behind a lost
    // reply is Created (and settled here), and an attempt never sent is NotCreated, closed there
    // so a late create is refused.
    #[tokio::test]
    async fn a_lost_acknowledgement_and_a_never_received_lease_are_settled_by_the_peer() {
        use crate::commands::genome::attempt_ledger::Reconciled;
        use crate::commands::genome::job_attempt_reconcile::{GenomeJobAttemptReconcile, JobAttemptReconcileOutcome, JobAttemptReconcileParams};
        let (peer_ledger, _peer_dir) = ledger().await;
        let (origin_ledger, _origin_dir) = ledger().await;
        let (origin, peer) = (Uuid::new_v4(), Uuid::new_v4());
        let transport = Arc::new(InProcessPeer::new(origin, peer_ledger));
        let genome = tempfile::tempdir().expect("genome root"); // JUSTIFIED: test fixture
        let adapter = adapter_over(transport.clone(), vec![(peer, offer(&[RECORDING_BASE_PREFIX], true), 10)], 10, genome.path());
        let reconcile = GenomeJobAttemptReconcile { attempts: origin_ledger.clone(), transport: transport.clone() };
        let destination = Destination::Peer(peer).to_ledger();

        // Lost reply: the peer made the job, the origin never heard.
        let lost = Uuid::new_v4();
        origin_ledger.admit(lost, None, GRID_PEER_PROVIDER, &destination).await.expect("admitted"); // JUSTIFIED: test fixture
        transport.drop_reply.store(true, Ordering::SeqCst);
        let binding = JobBinding { attempt: Some(lost), dispatch: None, destination: Destination::Peer(peer) };
        assert!(matches!(adapter.create_bound(lora_request(), &binding).await, Err(FineTuningError::Transient(_))), "a lost reply is uncertain, never a refusal");
        assert!(matches!(origin_ledger.reconcile(lost).await, Reconciled::Unknown(_)), "this ledger alone cannot decide");
        assert!(matches!(
            reconcile.run(&Ctx::default(), JobAttemptReconcileParams { attempt: lost }).await,
            Ok(JobAttemptReconcileOutcome::Created { handle }) if handle == lease_handle(peer, lost)
        ));
        assert_eq!(origin_ledger.reconcile(lost).await, Reconciled::Created(lease_handle(peer, lost)), "settled here too");

        // Never received: admitted here, never sent.
        let never = Uuid::new_v4();
        origin_ledger.admit(never, None, GRID_PEER_PROVIDER, &destination).await.expect("admitted"); // JUSTIFIED: test fixture
        assert!(matches!(
            reconcile.run(&Ctx::default(), JobAttemptReconcileParams { attempt: never }).await,
            Ok(JobAttemptReconcileOutcome::NotCreated)
        ));
        let late = JobBinding { attempt: Some(never), dispatch: None, destination: Destination::Peer(peer) };
        assert!(matches!(adapter.create_bound(lora_request(), &late).await, Err(FineTuningError::ProviderRejected(e)) if e.contains("attempt_fenced")), "the late create is fenced at the peer");
    }

    /// Poll until the lease reads finished here (completed or failed), bounded.
    async fn settled(adapter: &GridPeerAdapter, handle: &JobHandle) -> TrainingStatus {
        for _ in 0..500 {
            match adapter.poll(handle).await {
                Ok(TrainingStatus::Running { .. }) | Err(FineTuningError::Transient(_)) => tokio::time::sleep(std::time::Duration::from_millis(10)).await,
                Ok(done) => return done,
                Err(other) => panic!("the lease's poll failed: {other:?}"), // JUSTIFIED: test fails loud on an unexpected arm
            }
        }
        panic!("the lease did not settle within 5 s"); // JUSTIFIED: test bound
    }

    // what this catches (card 92cd2251, Codex's acceptance for S3-2): a leased adapter that never
    // comes back, lands outside the origin's own job directory, is trusted unverified, restarts
    // from zero after a lost reply, is pulled again after a restart, or is adopted onto a base
    // this node does not serve. The request is recorded before the lease goes out; the adapter
    // (three ranges) is pulled, verified and placed beside it; a lost range reply resumes from
    // the partial file; a restarted origin finds the placed file; a base whose content is not
    // this node's is final.
    #[tokio::test]
    async fn a_leased_adapter_comes_back_verified_into_the_origins_own_job_dir() {
        use crate::commands::genome::lease::MAX_ARTIFACT_CHUNK;
        use crate::genome::fine_tuning::ArtifactFormat;
        use sha2::{Digest, Sha256};
        let (peer_ledger, _dir) = ledger().await;
        let (origin, peer) = (Uuid::new_v4(), Uuid::new_v4());
        let files = tempfile::tempdir().expect("files"); // JUSTIFIED: test fixture
        let bytes: Vec<u8> = (0..MAX_ARTIFACT_CHUNK as usize * 2 + 123).map(|i| (i % 251) as u8).collect();
        let (adapter_file, base) = (files.path().join("adapter.gguf"), files.path().join("base.gguf"));
        std::fs::write(&adapter_file, &bytes).expect("write"); // JUSTIFIED: test fixture
        crate::commands::genome::test_support::write_base_gguf(&base, 0.0);
        let base_sha = format!("{:x}", Sha256::digest(std::fs::read(&base).expect("read"))); // JUSTIFIED: test fixture
        let stamp = crate::inference::weights_stamp::WeightsStamp::of(&base).expect("stamp"); // JUSTIFIED: test fixture
        let trainer = RecordingFineTuningAdapter::new().with_local_artifact(adapter_file, ArtifactFormat::GgufLora).trained_on(stamp);
        let transport = Arc::new(InProcessPeer::with_trainer(origin, peer_ledger, trainer));
        let genome = tempfile::tempdir().expect("genome root"); // JUSTIFIED: test fixture
        let offers = vec![(peer, offer(&[RECORDING_BASE_PREFIX], true), 10)];
        let origin_over = |root: &std::path::Path, sha: &str| {
            let offers = offers.clone();
            GridPeerAdapter::new(transport.clone(), Arc::new(move || offers.clone()), Arc::new(|| 10u64), true, pull_at(transport.clone(), root, sha))
        };
        let adapter = origin_over(genome.path(), &base_sha);
        let request = lora_request();
        let attempt = Uuid::new_v4();
        let binding = JobBinding { attempt: Some(attempt), dispatch: None, destination: Destination::Peer(peer) };
        let handle = adapter.create_bound(request.clone(), &binding).await.expect("the peer took the lease"); // JUSTIFIED: test asserts the Ok arm
        let job_dir = crate::genome::fine_tuning::job_dir::job_dir_under(genome.path(), &request.persona_name, &request.trait_kind, attempt);
        assert!(job_dir.join("request.json").is_file(), "the request was recorded before the lease went out");

        *transport.drop_range_at.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(u64::from(MAX_ARTIFACT_CHUNK)); // unwrap_or_else: a test fixture's poisoned flag still reads
        let TrainingStatus::Completed { artifact } = settled(&adapter, &handle).await else {
            panic!("the adapter came back"); // JUSTIFIED: test asserts the arm
        };
        let placed = job_dir.join("returned").join("adapter.gguf");
        assert_eq!(artifact.local_path.as_deref(), Some(placed.as_path()));
        assert_eq!(artifact.format, ArtifactFormat::GgufLora);
        assert_eq!(std::fs::read(&placed).expect("placed"), bytes, "verified bytes, whole"); // JUSTIFIED: test asserts the bytes
        let offsets = transport.ranges.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone(); // unwrap_or_else: a test fixture's poisoned log still reads
        assert_eq!(offsets.iter().filter(|&&o| o == 0).count(), 1, "the lost reply resumed from the partial file: {offsets:?}");

        let asked = offsets.len();
        let restarted = origin_over(genome.path(), &base_sha);
        assert!(matches!(settled(&restarted, &handle).await, TrainingStatus::Completed { .. }));
        assert_eq!(transport.ranges.lock().unwrap_or_else(std::sync::PoisonError::into_inner).len(), asked, "a placed, verified file is not pulled again"); // unwrap_or_else: a test fixture's poisoned log still reads

        let elsewhere = tempfile::tempdir().expect("another origin root"); // JUSTIFIED: test fixture
        let other_base = origin_over(elsewhere.path(), "a-different-base");
        let second = Uuid::new_v4();
        let other_binding = JobBinding { attempt: Some(second), dispatch: None, destination: Destination::Peer(peer) };
        let other_handle = other_base.create_bound(request, &other_binding).await.expect("the peer took the lease"); // JUSTIFIED: test asserts the Ok arm
        assert!(matches!(settled(&other_base, &other_handle).await, TrainingStatus::Failed { error } if error.contains("not this node's")));
    }
}
