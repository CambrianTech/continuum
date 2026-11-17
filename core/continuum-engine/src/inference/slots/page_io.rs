//! Page IO as a stop participant: a stop never exits with a page save it started still
//! unconfirmed (card 47151d36).
//!
//! The ledger makes a page restorable only after the engine confirms its save
//! ([`KvSlotPool::note_saved`]), and forgets it before any save overwrites it. That is
//! right, and it means a save cut by the process exiting leaves a good page on disk that
//! the next core will not trust. On the M5, 2026-10-27 19:43Z: after the admitted turns
//! settled, an admission paged Benchy out of her slot; the engine wrote her page about
//! 3 s later, after the core had exited, and her next turn re-prefilled 163,476 tokens
//! (800 s, `cause="page_on_disk_unconfirmed"`).
//!
//! One gate, the kernel's [`AdmissionGate`] (the turn gate and the log queue use the same
//! one): every pinned admission takes a permit before it plans paging, and a turn that
//! pages carries it into its retained page IO until the ledger is updated. This
//! participant runs after the admitted turns have settled: it closes the gate, so nothing
//! pages a resident out once the node is stopping, and waits for page IO already started
//! to reach its terminal receipt: the gate's drain event, never a poll or a timer (RTOS rule
//! 10, #5986). A page action that never answers ends through its own error events (engine
//! exit, a stopped engine: `kv_page_action`), never through a stopwatch here.
//!
//! Then every resident a slot still holds is saved, through the checkpoint a relaunch
//! already uses (confirmed saves, quarantine on an abandoned one). Without it a resident
//! returns from her last page-out and re-prefills everything since: Benchy, the M5
//! 20:46Z, 48,584 tokens (173 s) after a stop with her in slot 0.

use std::time::Duration;

use continuum_base::probed::Probed;

use crate::runtime::admission_gate::{AdmissionGate, Permit};

static PAGE_IO: AdmissionGate = AdmissionGate::new();

/// The node's page-IO gate. Admission paths take it as an argument so a test drives the
/// real admission and the real close on a gate of its own, never the process's.
pub(crate) fn gate() -> &'static AdmissionGate {
    &PAGE_IO
}

/// The refusal a stopping node gives anything that would page a resident out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PageIoClosed;

impl std::fmt::Display for PageIoClosed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the node is stopping: page admission is closed")
    }
}

/// Admit an admission that may page, or refuse because the node is stopping.
#[must_use = "the permit holds page IO open until its ledger update"]
pub(crate) fn admit(gate: &'static AdmissionGate) -> Result<Permit<'static>, PageIoClosed> {
    gate.admit().ok_or(PageIoClosed)
}

/// Page IO as a [`SettleBeforeStop`](crate::runtime::extensions::SettleBeforeStop)
/// participant. Installed after the admitted turns, so it settles once they have.
pub struct PageIoSettle;

impl crate::runtime::extensions::SettleBeforeStop for PageIoSettle {
    fn name(&self) -> &'static str {
        "page_io"
    }

    /// Ends on events, never on `cap`: the drain of started page IO, then every resident's save
    /// receipt. A deterministic step is never decided by a stopwatch (RTOS rule 10, #5986).
    /// Returns, per engine process, what the stop left in custody: the evidence `stop --engine`
    /// needs before it may end that process (CONTINUUM-ENGINE 4.9, I1).
    fn settle(&self, _cap: Duration) -> std::pin::Pin<Box<dyn std::future::Future<Output = Vec<crate::runtime::HeldCustody>> + Send + 'static>> {
        Box::pin(async move {
            let started = std::time::Instant::now();
            let s = settle(&PAGE_IO).await;
            continuum_base::probe!(
                class = "inference.kv_page.settled",
                in_flight_at_close = s.in_flight_at_close,
                waited_ms = started.elapsed().as_millis() as u64,
                "the stop closed page admission and every started page IO reached its ledger"
            );
            checkpoint_every_endpoint().await
        })
    }

    /// Recover exactly the retained entries given, one result each, in order (Codex's API on #6097). Admission
    /// stays closed: the page gate this stop closed is never reopened here.
    fn recover(
        &self,
        retained: Vec<crate::runtime::HeldCustody>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<crate::runtime::HeldCustody>, crate::runtime::extensions::CustodyRecoveryError>> + Send + 'static>> {
        Box::pin(async move {
            let client = reqwest::Client::new();
            let mut recovered = Vec::with_capacity(retained.len());
            for held in retained {
                recovered.push(recover_one(&client, held).await?);
            }
            Ok(recovered)
        })
    }
}

/// Save every resident of every endpoint, launched here or adopted, through the one checkpoint
/// owner for each (`checkpoint_endpoint`; `checkpoint_adopted_before_replacement` with a `Stop`
/// intent). Endpoints are independent, so each is checkpointed at once and the stop joins on
/// their receipts. Each outcome is returned bound to the engine process its checkpoint sealed; an
/// outcome with no such identity still counts against the stop's durability and authorizes ending
/// nothing.
async fn checkpoint_every_endpoint() -> Vec<crate::runtime::HeldCustody> {
    let client = reqwest::Client::new();
    let endpoints: Vec<(String, std::sync::Arc<super::EndpointSlots>)> =
        super::directory().endpoints.iter().map(|e| (e.key().clone(), e.value().clone())).collect();
    let client = &client;
    futures::future::join_all(
        endpoints.into_iter().map(|(root, endpoint)| checkpoint_one(client, root, endpoint, OwnedPass::First)),
    )
    .await
}

/// What a stop does to an endpoint's OWNED server before its checkpoint: the first pass condemns it (its turns
/// were handed back); a recovery pass finds it already condemned and only checkpoints again.
#[derive(Debug, Clone, Copy)]
enum OwnedPass {
    First,
    Recovery,
}

/// One endpoint's stop checkpoint and the custody it leaves: the first pass over every endpoint, and the
/// recovery of one retained endpoint, are the same step.
async fn checkpoint_one(
    client: &reqwest::Client,
    root: String,
    endpoint: std::sync::Arc<super::EndpointSlots>,
    pass: OwnedPass,
) -> crate::runtime::HeldCustody {
    let started = std::time::Instant::now();
    let (route, pool) = {
        let state = endpoint.state.lock();
        (StopRoute::of(&state), state.pool().cloned())
    };
    // Counted outside the endpoint's state lock (no new lock order); an endpoint never asked
    // or without paging keeps saying so instead of counting as zero residents.
    let residents = match pool {
        Probed::Known(pool) => Probed::Known(pool.holders.lock().len() as u64),
        Probed::NotAsked => Probed::NotAsked,
        Probed::Absent(why) => Probed::Absent(why),
    };
    // An adopted engine's checkpoint needs the live process recorded at this endpoint; the
    // checkpoint itself verifies it against the engine's ledger.
    let lane = crate::inference::lane_registry::live_lane()
        .filter(|lane| lane.root_url() == root)
        .map(|lane| lane.incarnation());
    // Before the drain: a stop cut during its checkpoint still leaves whom it was saving.
    continuum_base::probe!(class = "inference.kv_page.stop_checkpoint_started", endpoint = %root,
        route = ?route, residents = ?residents, lane = ?lane, pass = ?pass,
        "the stop drains this endpoint's acts, then saves every resident a slot holds, all at once");
    let state = match (route, lane, pass) {
        (StopRoute::Owned, _, OwnedPass::First) => {
            let condemn = || crate::inference::llama_server::condemn("the node is stopping: its turns were handed back, so their acts end before the stop's checkpoint");
            StopCheckpoint::Owned(checkpoint_endpoint(client, &root, &endpoint, condemn).await)
        }
        // condemned by the first pass already: only the checkpoint runs again
        (StopRoute::Owned, _, OwnedPass::Recovery) => StopCheckpoint::Owned(checkpoint_endpoint(client, &root, &endpoint, || ()).await),
        (StopRoute::Adopted, Some(lane), _) => StopCheckpoint::Adopted(checkpoint_adopted_for_stop(client, &root, &endpoint, lane).await),
        // An adopted pool with no live lane recorded here: no identity to bind a save to.
        (StopRoute::Adopted, None, _) => StopCheckpoint::Adopted(Err(super::AdoptedCheckpointRefusal::IdentityUnverified)),
        (StopRoute::NoPaging, _, _) => StopCheckpoint::NoPaging,
    };
    // The one process this evidence speaks for, sealed inside the checkpoint while it held the
    // endpoint's writer: the incarnation stamped on the pool it saved (owned), or the live
    // incarnation the adopted checkpoint verified against the ledger. Never a reread of the
    // endpoint after the writer is released, which a replacement could change (Codex on #6042).
    let process = state.process();
    let held = state.custody();
    continuum_base::probe!(
        class = "inference.kv_page.stop_checkpoint",
        endpoint = %root,
        outcome = ?state,
        failure = ?state.owned_failure(),
        custody = ?held,
        process = ?process,
        residents = ?residents,
        pass = ?pass,
        ms = started.elapsed().as_millis() as u64,
        "every resident a slot holds saved before the stop, or named as still held by the live engine"
    );
    // Retention with no identity still counts against the stop's durability; it only cannot
    // authorize ending a process.
    crate::runtime::HeldCustody {
        endpoint: root,
        process: process.map(|(pid, started_s)| crate::runtime::SettledProcess { pid, started_s }),
        state: held,
    }
}

/// Recover ONE retained endpoint (Codex's approved shape on #6097): a stamped process whose incarnation is gone
/// is Lost; otherwise only this endpoint checkpoints again (admission stays closed; an owned server is not
/// condemned twice). A stamped original never changes incarnation: a different process is Lost, and a retry
/// that names no process keeps the original identity, still Retained. An unknown original may bind to the
/// process the checkpoint verified.
async fn recover_one(
    client: &reqwest::Client,
    held: crate::runtime::HeldCustody,
) -> Result<crate::runtime::HeldCustody, crate::runtime::extensions::CustodyRecoveryError> {
    use crate::runtime::CustodyState;
    let invalid = |reason: &str| crate::runtime::extensions::CustodyRecoveryError::InvalidEvidence {
        participant: "page_io".into(),
        reason: reason.into(),
    };
    if held.state != CustodyState::Retained {
        return Err(invalid("only retained custody is recovered"));
    }
    let Some(endpoint) = super::directory().endpoints.get(&held.endpoint).map(|e| e.value().clone()) else {
        return Err(invalid("the retained endpoint is not one this process pages"));
    };
    if let Some(original) = held.process {
        if crate::inference::engine_residency::process_start_s(original.pid) != Some(original.started_s) {
            return Ok(crate::runtime::HeldCustody { state: CustodyState::Lost, ..held });
        }
    }
    let retried = checkpoint_one(client, held.endpoint.clone(), endpoint, OwnedPass::Recovery).await;
    Ok(match (held.process, retried.process) {
        (Some(original), Some(now)) if original != now => crate::runtime::HeldCustody { state: CustodyState::Lost, ..held },
        (Some(_), None) => held, // the retry named no process: the original identity stays, still Retained
        _ => retried,
    })
}

/// Drain an adopted endpoint's acts and save its residents through the adopted checkpoint owner,
/// suspended whatever the outcome: the node is stopping.
async fn checkpoint_adopted_for_stop(
    client: &reqwest::Client,
    root: &str,
    endpoint: &std::sync::Arc<super::EndpointSlots>,
    process: crate::inference::engine_residency::EngineIncarnation,
) -> Result<(u32, u64), super::AdoptedCheckpointRefusal> {
    let condemn = || crate::inference::llama_server::condemn("the node is stopping: its turns were handed back, so their acts end before the stop's checkpoint");
    let drained = endpoint
        .drain_for_replacement(None, None, || Ok::<(), std::convert::Infallible>(()), condemn, &|| true)
        .await;
    let Ok(Ok(mut transition)) = drained else {
        return Err(super::AdoptedCheckpointRefusal::IntentChanged);
    };
    transition
        .checkpoint_adopted_before_replacement(client, root, super::AdoptedCheckpoint { incarnation: process, intent: super::AdoptedCheckpointIntent::Stop }, &|| true)
        .await
        .map(|()| (process.pid, process.started_s))
}

/// Which checkpoint owner an endpoint's engine stops through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StopRoute {
    /// An engine this core launched: its generation's checkpoint.
    Owned,
    /// A running engine this core did not launch, its pool discovered or still to discover: the
    /// adopted checkpoint, which verifies the live incarnation and the ledger.
    Adopted,
    /// Probed and latched without paging: no resident KV to save.
    NoPaging,
}

impl StopRoute {
    fn of(state: &super::kv_slot_pool::EndpointState) -> Self {
        if state.generation().is_some() {
            return Self::Owned;
        }
        match state.pool() {
            Probed::Known(_) | Probed::NotAsked => Self::Adopted,
            Probed::Absent(_) => Self::NoPaging,
        }
    }
}

/// What the stop's checkpoint of one endpoint came to, by route.
#[derive(Debug)]
pub(crate) enum StopCheckpoint {
    Owned(OwnedStopCheckpoint),
    /// The incarnation the adopted checkpoint verified and saved, or why it refused.
    Adopted(Result<(u32, u64), super::AdoptedCheckpointRefusal>),
    NoPaging,
}

impl StopCheckpoint {
    /// The custody it leaves: saved only on a confirmed save of every resident. Anything else,
    /// including an engine without paging or a generation that ended under the stop, is no proof
    /// that the live process holds nothing, so the live engine still holds it (Codex on #6042).
    fn custody(&self) -> crate::runtime::CustodyState {
        use crate::runtime::CustodyState;
        match self {
            Self::Owned(OwnedStopCheckpoint::Saved { .. }) | Self::Adopted(Ok(_)) => CustodyState::Saved,
            Self::Owned(OwnedStopCheckpoint::Ended | OwnedStopCheckpoint::EngineChanged | OwnedStopCheckpoint::Failed(_))
            | Self::Adopted(Err(_))
            | Self::NoPaging => CustodyState::Retained,
        }
    }

    /// An owned checkpoint's typed failure, for its probe (an adopted refusal is in the outcome).
    fn owned_failure(&self) -> Option<&super::CheckpointFailure<super::OwnedCheckpointRefusal>> {
        match self {
            Self::Owned(owned) => owned.failure(),
            Self::Adopted(_) | Self::NoPaging => None,
        }
    }

    /// The process a confirmed save speaks for, as the checkpoint sealed it; `None` when nothing
    /// was saved or the saved pool carried no stamped incarnation.
    fn process(&self) -> Option<(u32, u64)> {
        match self {
            Self::Owned(OwnedStopCheckpoint::Saved { process }) => *process,
            Self::Adopted(Ok(process)) => Some(*process),
            Self::Owned(_) | Self::Adopted(Err(_)) | Self::NoPaging => None,
        }
    }
}

/// What the stop's checkpoint of an owned endpoint came to.
#[derive(Debug)]
pub(crate) enum OwnedStopCheckpoint {
    /// No generation of this core is on the endpoint: never launched here, or it ended.
    Ended,
    /// Another engine took the endpoint before the checkpoint held it.
    EngineChanged,
    /// Every resident's save confirmed, with the incarnation stamped on the pool it saved, read
    /// while the checkpoint held the endpoint's writer.
    Saved { process: Option<(u32, u64)> },
    Failed(super::CheckpointFailure<super::OwnedCheckpointRefusal>),
}

impl OwnedStopCheckpoint {
    /// Why the checkpoint did not save, when it did not: the typed failure, for its probe.
    fn failure(&self) -> Option<&super::CheckpointFailure<super::OwnedCheckpointRefusal>> {
        match self {
            Self::Failed(failure) => Some(failure),
            Self::Ended | Self::EngineChanged | Self::Saved { .. } => None,
        }
    }
}

/// Save every resident `endpoint`'s slots hold through the relaunch checkpoint (the drained
/// writer, confirmed saves). An adopted or exited engine refuses, and it is named, never
/// retried. The endpoint stays suspended afterwards: the node is stopping.
///
/// CONDEMNED FIRST, through the one drain a replacement uses (`drain_for_replacement`): the
/// writer waits for every admitted act's guard, and the turn settle before this has already
/// handed those turns back, so nothing else ends them. Without it the stop waited on their
/// decodes: the M5 2026-11-13/14 (card b5b9bef3), 4 turns handed back at 00:51:56Z and no save
/// until a manual SIGTERM at 00:53:34Z; 23:40Z the same, 7 min at 19:19Z.
pub(crate) async fn checkpoint_endpoint(
    client: &reqwest::Client,
    root: &str,
    endpoint: &std::sync::Arc<super::EndpointSlots>,
    condemn: impl FnOnce(),
) -> OwnedStopCheckpoint {
    let Some(generation) = endpoint.state.lock().generation().cloned() else {
        return OwnedStopCheckpoint::Ended;
    };
    let drained = endpoint
        .drain_for_replacement(Some(&generation), Some(&generation), || Ok::<(), std::convert::Infallible>(()), condemn, &|| true)
        .await;
    let Ok(Ok(mut transition)) = drained else {
        return OwnedStopCheckpoint::EngineChanged;
    };
    match transition.checkpoint_residents(client, root).await {
        Ok(saved) => OwnedStopCheckpoint::Saved { process: saved.pool.incarnation() },
        Err(failure) => OwnedStopCheckpoint::Failed(failure),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PageIoSettled {
    pub(crate) in_flight_at_close: u64,
}

/// Close `gate`, then wait for its drain event: every page IO started before the close has
/// released its permit (its terminal receipt and ledger update).
pub(crate) async fn settle(gate: &AdmissionGate) -> PageIoSettled {
    gate.close();
    let in_flight_at_close = gate.in_flight();
    gate.drained().await;
    PageIoSettled { in_flight_at_close }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a stop that exits while a page save it started is still running
    // (Benchy's 800 s re-prefill, card 47151d36), a closed gate that still admits, and a drain
    // that ends on anything but the started save's own release (no stopwatch decides it).
    #[tokio::test]
    async fn a_stop_waits_for_started_page_io_and_admits_no_more() {
        let gate = AdmissionGate::new();
        let permit = gate.admit().expect("an open gate admits");
        let settling = settle(&gate);
        tokio::pin!(settling);
        assert!(futures::poll!(&mut settling).is_pending(), "the stop waits for the started save");
        assert!(gate.admit().is_none(), "nothing pages once the node is stopping");
        drop(permit); // the save's terminal receipt and ledger update
        assert_eq!(settling.await, PageIoSettled { in_flight_at_close: 1 });
    }

    // what this catches: a recovery that re-saves or rebinds what it must not (Codex's recovery API on #6097):
    // only Retained custody is recovered, only for an endpoint this process pages, and a stamped incarnation that
    // is gone becomes Lost with its original endpoint and process, never retried against a replacement.
    #[tokio::test]
    async fn recovery_takes_only_retained_custody_and_a_gone_incarnation_is_lost() {
        use crate::runtime::{CustodyState, HeldCustody, SettledProcess};
        let root = format!("http://127.0.0.1:1/{}", uuid::Uuid::new_v4());
        let _paged = super::super::directory().endpoint(&root); // this process pages the endpoint
        let client = reqwest::Client::new();
        let gone = SettledProcess { pid: u32::MAX - 7, started_s: 1 }; // no such process
        let held = |state| HeldCustody { endpoint: root.clone(), process: Some(gone), state };

        assert!(recover_one(&client, held(CustodyState::Saved)).await.is_err(), "Saved is immutable, never recovered");
        assert!(recover_one(&client, held(CustodyState::Lost)).await.is_err(), "Lost is immutable, never recovered");
        let unknown = HeldCustody { endpoint: format!("http://127.0.0.1:1/{}", uuid::Uuid::new_v4()), ..held(CustodyState::Retained) };
        assert!(recover_one(&client, unknown).await.is_err(), "an endpoint this process does not page is a protocol failure");

        let recovered = recover_one(&client, held(CustodyState::Retained)).await.expect("a retained entry recovers");
        assert_eq!(recovered, held(CustodyState::Lost), "a gone incarnation is Lost, its endpoint and process preserved");
    }

    // what this catches: a drain that waits when nothing was in flight.
    #[tokio::test]
    async fn a_stop_with_no_page_io_settles_at_once() {
        let gate = AdmissionGate::new();
        let settling = settle(&gate);
        tokio::pin!(settling);
        assert_eq!(futures::poll!(&mut settling), std::task::Poll::Ready(PageIoSettled { in_flight_at_close: 0 }));
    }
}
