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
//! `training_lease_return`, which no build emits until S3 (the artifact pull and held-out acceptance at the origin) gives a leased job's
//! adapter a way back. Until then it is wired and serves no base, so no peer spends its lane.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;

use super::lease::{LeaseCancelOutcome, LeasePollOutcome, LeaseStatusParams, LeaseTrainOutcome, LeaseTrainParams};
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

pub(crate) struct GridPeerAdapter {
    transport: Arc<dyn LeaseTransport>,
    offers: OfferSource,
    now_ms: Arc<dyn Fn() -> u64 + Send + Sync>,
    /// Whether THIS node can take a leased job's adapter back (it emits
    /// `training_lease_return` itself). A lease needs the return on BOTH ends: an origin
    /// without it never leases, whatever a newer peer advertises (Codex on #5417).
    returns_here: bool,
}

impl GridPeerAdapter {
    /// The production trainer: the executor's airc route and the gossip ledger's heard offers.
    pub(crate) fn on_grid(executor: Arc<crate::runtime::LateBound<crate::runtime::CommandExecutor>>) -> Self {
        Self::new(
            Arc::new(ExecutorLeaseTransport(executor)),
            Arc::new(|| crate::capacity::gossip::global_ledger().foreign_offers_with_age()),
            Arc::new(|| continuum_base::time::EpochMs::now().millis()),
            crate::capacity::gossip::this_build_capabilities().contains(&CapabilityId::TrainingLeaseReturn),
        )
    }

    pub(crate) fn new(
        transport: Arc<dyn LeaseTransport>,
        offers: OfferSource,
        now_ms: Arc<dyn Fn() -> u64 + Send + Sync>,
        returns_here: bool,
    ) -> Self {
        Self { transport, offers, now_ms, returns_here }
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
        let params = serde_json::to_value(LeaseTrainParams { attempt, dispatch: binding.dispatch, request }) // lease wire boundary: the request crosses the grid as JSON
            .map_err(|e| FineTuningError::InvalidRequest(e.to_string()))?;
        match decode::<LeaseTrainOutcome>("genome/lease/train", self.ask(*peer, "genome/lease/train", params).await?)? {
            LeaseTrainOutcome::Created { .. } => Ok(lease_handle(*peer, attempt)),
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
            // Pulling and verifying the files lands with the origin's import (S3-2); until then
            // a finished job stays pending, never failed: its artifact is kept on the peer.
            LeasePollOutcome::Completed { .. } => {
                Err(FineTuningError::Transient(format!("peer {peer} finished; the origin's artifact import is not wired yet")))
            }
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
    use crate::commands::genome::lease::{GenomeLeaseCancel, GenomeLeasePoll, GenomeLeaseStatus, GenomeLeaseTrain};
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
        drop_reply: AtomicBool,
        /// Every peer a call was addressed to, so a call to the wrong peer is caught.
        called: std::sync::Mutex<Vec<Uuid>>,
    }

    impl InProcessPeer {
        fn new(origin: Uuid, attempts: Arc<AttemptLedger>) -> Self {
            let registry = Arc::new(FineTuningRegistry::new());
            registry.register(Arc::new(RecordingFineTuningAdapter::new()));
            Self {
                origin,
                train: GenomeLeaseTrain { coordinator: Arc::new(FineTuningCoordinator::new(registry.clone())), attempts: attempts.clone() },
                status: GenomeLeaseStatus { attempts: attempts.clone() },
                poll: GenomeLeasePoll { attempts: attempts.clone(), registry: registry.clone(), bases: crate::commands::genome::lease::BaseGguf::registry() },
                cancel: GenomeLeaseCancel { attempts, registry },
                drop_reply: AtomicBool::new(false),
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
            let ctx = self.as_origin(self.origin);
            let err = |e: crate::sdk_codegen::CommandError| format!("{e:?}");
            let reply = match command {
                "genome/lease/train" => serde_json::to_value(self.train.run(&ctx, serde_json::from_value(params).map_err(|e| e.to_string())?).await.map_err(err)?),
                "genome/lease/status" => serde_json::to_value(self.status.run(&ctx, serde_json::from_value(params).map_err(|e| e.to_string())?).await.map_err(err)?),
                "genome/lease/poll" => serde_json::to_value(self.poll.run(&ctx, serde_json::from_value(params).map_err(|e| e.to_string())?).await.map_err(err)?),
                "genome/lease/cancel" => serde_json::to_value(self.cancel.run(&ctx, serde_json::from_value(params).map_err(|e| e.to_string())?).await.map_err(err)?),
                other => return Err(format!("no such lease verb {other}")),
            }
            .map_err(|e| e.to_string())?;
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
        }
    }

    fn lora_request() -> TrainingJobRequest {
        let mut request = crate::commands::genome::test_support::request_for(RECORDING_BASE_PREFIX);
        request.lora = Some(crate::genome::fine_tuning::LoRAHyperparams { rank: 8, alpha: 16, dropout: 0.0, target_modules: vec![], top_layers: None });
        request
    }

    fn adapter_over(transport: Arc<dyn LeaseTransport>, offers: Vec<(Uuid, CapacityOffer, u64)>, now: u64) -> GridPeerAdapter {
        GridPeerAdapter::new(transport, Arc::new(move || offers.clone()), Arc::new(move || now), true)
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
        let adapter = adapter_over(transport.clone(), vec![
            (Uuid::from_u128(0), no_return, now),
            (high, offer(&["qwen"], true), now),
            (low, offer(&["qwen"], true), now - window),
            (stale, offer(&["qwen"], true), now - window - 1),
            (blind, offer(&["qwen"], false), now),
            (other, offer(&["llama"], true), now),
        ], now);
        assert_eq!(adapter.choose_peer("qwen"), Some(low), "fresh, both capabilities, lowest id; the stale, the blind and the one whose adapter cannot return are not candidates");
        assert_eq!(adapter.choose_peer("mistral"), None);
        // An origin that cannot take an adapter back never leases, whatever peers advertise.
        let offers_now = vec![(low, offer(&["qwen"], true), now)];
        let no_return_here = GridPeerAdapter::new(transport.clone(), Arc::new(move || offers_now.clone()), Arc::new(move || now), false);
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
        let adapter = adapter_over(transport.clone(), vec![(peer, offer(&[RECORDING_BASE_PREFIX], true), 10)], 10);
        let attempt = Uuid::new_v4();
        let binding = JobBinding { attempt: Some(attempt), dispatch: None, destination: Destination::Peer(peer) };

        let handle = adapter.create_bound(lora_request(), &binding).await.expect("the peer took the lease"); // JUSTIFIED: test asserts the Ok arm
        assert_eq!(handle, lease_handle(peer, attempt));
        // The fixture's trainer keeps no local weights: the peer reports it unreturnable, final.
        assert!(matches!(adapter.poll(&handle).await, Ok(TrainingStatus::Failed { error }) if error.contains("no local gene")));
        assert!(adapter.cancel(&handle).await.is_ok(), "accepted by the peer");

        let mut changed = lora_request();
        changed.persona_name = "someone-else".into();
        assert!(matches!(adapter.create_bound(changed, &binding).await, Err(FineTuningError::ProviderRejected(e)) if e.contains("payload_changed")));

        // A new adapter instance (the origin's in-memory state gone), over the same stores.
        let restarted = adapter_over(transport.clone(), Vec::new(), 10);
        assert!(matches!(restarted.poll(&handle).await, Ok(TrainingStatus::Failed { .. })), "the handle alone reaches the job");
        assert!(
            transport.called.lock().unwrap_or_else(std::sync::PoisonError::into_inner).iter().all(|to| *to == peer), // unwrap_or_else: a test fixture's poisoned log still reads
            "create, poll and cancel went only to the bound peer"
        );

        let forger = adapter_over(Arc::new(InProcessPeer::new(Uuid::new_v4(), peer_ledger)), Vec::new(), 10);
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
        let adapter = adapter_over(transport.clone(), vec![(peer, offer(&[RECORDING_BASE_PREFIX], true), 10)], 10);
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
}
