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
    fn settle(&self, _cap: Duration) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
        Box::pin(async move {
            let started = std::time::Instant::now();
            let s = settle(&PAGE_IO).await;
            continuum_base::probe!(
                class = "inference.kv_page.settled",
                in_flight_at_close = s.in_flight_at_close,
                waited_ms = started.elapsed().as_millis() as u64,
                "the stop closed page admission and every started page IO reached its ledger"
            );
            checkpoint_every_endpoint().await;
        })
    }
}

/// Save every resident of every endpoint whose engine this core owns. Endpoints are independent,
/// so each is checkpointed at once and the stop joins on their receipts.
async fn checkpoint_every_endpoint() {
    let client = reqwest::Client::new();
    let endpoints: Vec<(String, std::sync::Arc<super::EndpointSlots>)> =
        super::directory().endpoints.iter().map(|e| (e.key().clone(), e.value().clone())).collect();
    let client = &client;
    futures::future::join_all(endpoints.into_iter().map(|(root, endpoint)| async move {
        let started = std::time::Instant::now();
        let (skipped, pool) = {
            let state = endpoint.state.lock();
            (StopCheckpointSkip::of(&state), state.pool().cloned())
        };
        // Counted outside the endpoint's state lock (no new lock order); an endpoint never asked
        // or without paging keeps saying so instead of counting as zero residents.
        let residents = match pool {
            Probed::Known(pool) => Probed::Known(pool.holders.lock().len() as u64),
            Probed::NotAsked => Probed::NotAsked,
            Probed::Absent(why) => Probed::Absent(why),
        };
        if let Some(skip) = skipped {
            // Named, never silent (the M5, 2026-11-13 23:40Z: a stop with no checkpoint receipt and
            // a next boot that restored nothing): residents of an engine this core did not launch
            // are not saved by this stop, and the next boot says so per mind (`cold_turn`).
            continuum_base::probe!(class = "inference.kv_page.stop_checkpoint_skipped", endpoint = %root,
                reason = ?skip, residents = ?residents,
                "the stop saved no resident of this endpoint: each re-prefills from her last confirmed page, or cold");
            return;
        }
        // Before the drain: a stop cut during its checkpoint still leaves whom it was saving.
        continuum_base::probe!(class = "inference.kv_page.stop_checkpoint_started", endpoint = %root,
            residents = ?residents,
            "the stop drains this endpoint's acts, then saves every resident a slot holds, all at once");
        let condemn = || crate::inference::llama_server::condemn("the node is stopping: its turns were handed back, so their acts end before the stop's checkpoint");
        let outcome = checkpoint_endpoint(client, &root, &endpoint, condemn).await;
        continuum_base::probe!(
            class = "inference.kv_page.stop_checkpoint",
            endpoint = %root,
            saved = matches!(outcome, StopCheckpoint::Saved),
            outcome = ?outcome,
            failure = ?outcome.failure(),
            residents = ?residents,
            ms = started.elapsed().as_millis() as u64,
            "every resident a slot holds saved before the stop, so each returns where she left off"
        );
    }))
    .await;
}

/// What the stop's checkpoint of one endpoint came to.
#[derive(Debug)]
pub(crate) enum StopCheckpoint {
    /// No generation of this core is on the endpoint: never launched here, or it ended.
    Ended,
    /// Another engine took the endpoint before the checkpoint held it.
    EngineChanged,
    /// Every resident's save confirmed.
    Saved,
    Failed(super::CheckpointFailure<super::OwnedCheckpointRefusal>),
}

impl StopCheckpoint {
    /// Why the checkpoint did not save, when it did not: the typed failure, for its probe.
    fn failure(&self) -> Option<&super::CheckpointFailure<super::OwnedCheckpointRefusal>> {
        match self {
            Self::Failed(failure) => Some(failure),
            Self::Ended | Self::EngineChanged | Self::Saved => None,
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
) -> StopCheckpoint {
    let Some(generation) = endpoint.state.lock().generation().cloned() else {
        return StopCheckpoint::Ended;
    };
    let drained = endpoint
        .drain_for_replacement(Some(&generation), Some(&generation), || Ok::<(), std::convert::Infallible>(()), condemn, &|| true)
        .await;
    let Ok(Ok(mut transition)) = drained else {
        return StopCheckpoint::EngineChanged;
    };
    match transition.checkpoint_residents(client, root).await {
        Ok(_) => StopCheckpoint::Saved,
        Err(failure) => StopCheckpoint::Failed(failure),
    }
}

/// Why the stop's checkpoint passed an endpoint by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StopCheckpointSkip {
    /// A running engine a previous core launched: this core holds no launch contract for it, so
    /// its residents' KV is not saved here. A stop that also ends the engine loses it.
    Adopted,
    /// No engine with paging was launched or adopted here: no resident.
    NoPagedEngine,
}

impl StopCheckpointSkip {
    fn of(state: &super::kv_slot_pool::EndpointState) -> Option<Self> {
        if state.generation().is_some() {
            return None;
        }
        Some(match state.pool() {
            Probed::Known(_) => Self::Adopted,
            Probed::NotAsked | Probed::Absent(_) => Self::NoPagedEngine,
        })
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

    // what this catches: a drain that waits when nothing was in flight.
    #[tokio::test]
    async fn a_stop_with_no_page_io_settles_at_once() {
        let gate = AdmissionGate::new();
        let settling = settle(&gate);
        tokio::pin!(settling);
        assert_eq!(futures::poll!(&mut settling), std::task::Poll::Ready(PageIoSettled { in_flight_at_close: 0 }));
    }
}
