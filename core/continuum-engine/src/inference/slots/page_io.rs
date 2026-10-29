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
//! to reach its terminal receipt. Each page action is bounded by its own watchdog; the
//! stop's cap is only the backstop.
//!
//! Then every resident a slot still holds is saved, through the checkpoint a relaunch
//! already uses (confirmed saves, quarantine on an abandoned one). Without it a resident
//! returns from her last page-out and re-prefills everything since: Benchy, the M5
//! 20:46Z, 48,584 tokens (173 s) after a stop with her in slot 0.

use std::time::Duration;

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

    fn settle(&self, cap: Duration) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
        Box::pin(async move {
            let started = std::time::Instant::now();
            let s = settle(&PAGE_IO, cap).await;
            continuum_base::probe!(
                class = "inference.kv_page.settled",
                in_flight_at_close = s.in_flight_at_close,
                cut = s.cut,
                waited_ms = started.elapsed().as_millis() as u64,
                cap_ms = cap.as_millis() as u64,
                "the stop closed page admission and let started page IO reach its ledger — `cut` is what it still abandoned"
            );
            checkpoint_every_endpoint(cap.saturating_sub(started.elapsed())).await;
        })
    }
}

/// Save every resident of every endpoint whose engine this core owns, within `cap`.
async fn checkpoint_every_endpoint(cap: Duration) {
    if cap.is_zero() {
        // The turns used the stop's whole budget: a save begun now would be cut by the exit.
        // Residents resume from their last page, which is still a prefix of what they hold.
        continuum_base::probe!(class = "inference.kv_page.stop_checkpoint_skipped", reason = "no_budget_left",
            "the stop's budget was spent settling turns; residents resume from their last saved page");
        return;
    }
    let client = reqwest::Client::new();
    let endpoints: Vec<(String, std::sync::Arc<super::EndpointSlots>)> =
        super::directory().endpoints.iter().map(|e| (e.key().clone(), e.value().clone())).collect();
    let deadline = std::time::Instant::now() + cap;
    for (root, endpoint) in endpoints {
        let started = std::time::Instant::now();
        let left = deadline.saturating_duration_since(started);
        let Some(outcome) = checkpoint_endpoint(&client, &root, &endpoint, left).await else {
            continue; // never launched here: nothing resident
        };
        continuum_base::probe!(
            class = "inference.kv_page.stop_checkpoint",
            endpoint = %root,
            ok = outcome.is_ok(),
            error = outcome.as_ref().err().map_or("", String::as_str),
            ms = started.elapsed().as_millis() as u64,
            "every resident a slot holds saved before the stop, so each returns where she left off"
        );
    }
}

/// Save every resident `endpoint`'s slots hold, within `cap`, through the relaunch
/// checkpoint (the drained writer, confirmed saves). `None` when no engine was launched
/// there. An adopted or exited engine refuses, and it is named, never retried. The endpoint
/// stays suspended afterwards: the node is stopping.
pub(crate) async fn checkpoint_endpoint(
    client: &reqwest::Client,
    root: &str,
    endpoint: &std::sync::Arc<super::EndpointSlots>,
    cap: Duration,
) -> Option<Result<(), String>> {
    let generation = endpoint.state.lock().generation.clone()?;
    let outcome = tokio::time::timeout(cap, async {
        let mut transition = endpoint
            .transition_for_generation(&generation)
            .await
            .map_err(|()| "the engine changed before the stop's checkpoint".to_owned())?;
        transition.checkpoint_residents(client, root).await.map(|_| ())
    })
    .await
    .map_err(|_elapsed| format!("the stop's cap ({} ms) ran out", cap.as_millis()))
    .and_then(|saved| saved);
    Some(outcome)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PageIoSettled {
    pub(crate) in_flight_at_close: u64,
    pub(crate) cut: u64,
}

/// Close `gate`, then wait for its in-flight page IO, at most `cap`.
pub(crate) async fn settle(gate: &AdmissionGate, cap: Duration) -> PageIoSettled {
    const POLL: Duration = Duration::from_millis(50);
    gate.close();
    let in_flight_at_close = gate.in_flight();
    let deadline = std::time::Instant::now() + cap;
    let mut poll = tokio::time::interval(POLL);
    while gate.in_flight() > 0 && std::time::Instant::now() < deadline {
        poll.tick().await;
    }
    PageIoSettled { in_flight_at_close, cut: gate.in_flight() }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a stop that exits while a page save it started is still running
    // (Benchy's 800 s re-prefill, card 47151d36), and a closed gate that still admits.
    #[tokio::test]
    async fn a_stop_waits_for_started_page_io_and_admits_no_more() {
        let gate = AdmissionGate::new();
        let permit = gate.admit().expect("an open gate admits");
        let saving = async move {
            tokio::time::sleep(Duration::from_millis(120)).await;
            drop(permit); // the save's terminal receipt and ledger update
        };
        let (settled, ()) = tokio::join!(settle(&gate, Duration::from_secs(5)), saving);
        assert_eq!(settled, PageIoSettled { in_flight_at_close: 1, cut: 0 });
        assert!(gate.admit().is_none(), "nothing pages once the node is stopping");
    }

    // what this catches: a wedged page action holding the stop past its cap.
    #[tokio::test]
    async fn a_page_action_that_never_finishes_is_cut_at_the_cap() {
        let gate = AdmissionGate::new();
        let _stuck = gate.admit().expect("an open gate admits");
        let settled = settle(&gate, Duration::from_millis(100)).await;
        assert_eq!(settled, PageIoSettled { in_flight_at_close: 1, cut: 1 });
    }
}
