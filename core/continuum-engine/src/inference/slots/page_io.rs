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

use std::time::Duration;

use crate::runtime::admission_gate::{AdmissionGate, Permit};

static PAGE_IO: AdmissionGate = AdmissionGate::new();

/// Admit an admission that may page, or refuse because the node is stopping.
#[must_use = "the permit holds page IO open until its ledger update"]
pub(crate) fn admit() -> Option<Permit<'static>> {
    PAGE_IO.admit()
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
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PageIoSettled {
    in_flight_at_close: u64,
    cut: u64,
}

/// Close `gate`, then wait for its in-flight page IO, at most `cap`.
async fn settle(gate: &AdmissionGate, cap: Duration) -> PageIoSettled {
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
