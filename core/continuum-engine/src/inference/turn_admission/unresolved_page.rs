//! A page switch the engine is still working on, with no receipt yet: the request in flight AND
//! the physical custody of its slot, which travel together and are released together.
//!
//! Codex on #5772: an unresolved switch that kept only its HTTP future released the slot's
//! permit, pin and endpoint reader when the turn returned its error, so a later turn could
//! reuse the slot while the old restore was still queued in the engine (a late restore then
//! overwrites another activity's KV), and a lifecycle transition no longer waited on it. So the
//! consuming an [`UnresolvedPage`] through `hold` or `resume` moves the
//! custody (a turn's whole admission: lane permit, slot permit, pin, endpoint reader) into the
//! endpoint's paging task beside the request. The compiler then forbids reusing the slot while
//! it lives, and a transition's writer waits on its reader as it waits on any act in flight.
//!
//! The custody is released only as a [`Released`], and each kind carries its proof:
//! - `Verified`: the late reply is a TERMINAL receipt, judged by the same
//!   [`super::page_receipt::terminal_receipt`] the bounded wait uses (Codex's follow-up on
//!   #5772: a socket closed under the request, or a truncated body, is not proof the engine
//!   ended the switch);
//! - `Fenced`: the endpoint was quarantined, so no turn is admitted until a verified
//!   replacement. Reached by an unverified reply, or by the engine's own process (fenced by pid
//!   and start time) gone or spending no CPU with the request unanswered;
//! - `Condemned`: the serving daemon decided to replace this engine; its relaunch kills the
//!   process that holds the request before any successor serves, while its drain waits for
//!   every reader, this one included, to let go.

use std::time::Duration;

use super::page_liveness::{PageLiveness, PageReply, Pending};
use super::page_receipt::{terminal_receipt, Terminal};
use crate::inference::llama_server::EngineProbe;
use crate::inference::slots::EndpointSlots;

#[cfg(test)]
mod tests;

/// The request of a page switch that outlived its measured bound while the engine's process
/// kept spending CPU, with what it needs to judge the engine until the receipt arrives.
#[must_use = "an unresolved page switch is held with its slot's custody until its receipt (UnresolvedPage::hold)"]
pub(crate) struct UnresolvedPage {
    pub(super) root: String,
    pub(super) action: super::page_receipt::PageAction,
    pub(super) slot: u32,
    /// The page file the switch reads or writes: what a terminal receipt must name.
    pub(super) filename: String,
    /// The CPU the engine's process spent over the checkpoint that ended the bounded wait.
    pub(super) cpu_spent: Duration,
    /// The switch's checkpoint (from this node's measured switch cost): how often the held
    /// custody asks the engine's process whether it still works.
    pub(super) checkpoint: Duration,
    pub(super) pending: Pending<PageReply>,
    pub(super) liveness: PageLiveness,
}

impl std::fmt::Debug for UnresolvedPage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnresolvedPage")
            .field("root", &self.root)
            .field("action", &self.action)
            .field("slot", &self.slot)
            .field("cpu_spent", &self.cpu_spent)
            .finish_non_exhaustive()
    }
}

/// Proof that the endpoint's paging was quarantined: made only by [`quarantined`], so a release
/// without a verified receipt cannot be written without quarantining first.
struct Quarantined(());

fn quarantined(endpoint: &EndpointSlots) -> Quarantined {
    endpoint.quarantine_paging();
    Quarantined(())
}

/// What released a held custody, with its proof.
enum Released {
    /// The engine's terminal receipt, as the bounded wait would have judged it.
    Verified(Terminal),
    /// No verified receipt: the endpoint is quarantined until a verified replacement.
    Fenced(Quarantined),
    /// The serving daemon is replacing the engine.
    Condemned,
}

/// Custody is returned for continuation only after a verified terminal receipt.
/// A fenced or condemned endpoint releases custody without authorizing further IO.
pub(crate) enum PageResolution<C> {
    Completed(C),
    Rejected(C),
    Fenced,
}

impl UnresolvedPage {
    /// The CPU the engine's process spent over the checkpoint that left this switch unresolved.
    pub(crate) fn cpu_spent(&self) -> Duration {
        self.cpu_spent
    }

    /// Move `custody` into the endpoint's paging task beside the request; both are released
    /// together, only as a [`Released`] (module docs). `custody` is whatever holds the slot: a
    /// turn's admission, or `()` where the caller holds the whole endpoint and quarantines it
    /// itself (a checkpoint inside a lifecycle writer).
    pub(crate) fn hold<C: Send + 'static>(self, custody: C) {
        self.retain(custody, |_, custody| drop(custody));
    }

    pub(crate) fn resume<C: Send + 'static>(self, custody: C) -> tokio::sync::oneshot::Receiver<PageResolution<C>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.retain(custody, move |released, custody| {
            let outcome = match released {
                Released::Verified(Terminal::Completed) => PageResolution::Completed(custody),
                Released::Verified(Terminal::Rejected) => PageResolution::Rejected(custody),
                Released::Fenced(_) | Released::Condemned => { drop(custody); PageResolution::Fenced }
            };
            let _ = tx.send(outcome); // dropped continuation releases custody only after its proof
        });
        rx
    }

    fn retain<C: Send + 'static>(self, custody: C, complete: impl FnOnce(Released, C) + Send + 'static) {
        let Self { root, action, slot, filename, cpu_spent: _, checkpoint, mut pending, liveness } = self;
        let endpoint = crate::inference::slots::directory().endpoint(&root);
        // Subscribed before the check, so a condemnation between them is never missed.
        let mut condemned = liveness.condemnation().next();
        let condemned_already = liveness.condemnation().now();
        let owner = std::sync::Arc::clone(&endpoint);
        owner.retain_paging(async move {
            let held = std::time::Instant::now();
            let released = if condemned_already {
                Released::Condemned
            } else {
                let mut checkpoints = tokio::time::interval_at(tokio::time::Instant::now() + checkpoint, checkpoint);
                checkpoints.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                loop {
                    tokio::select! {
                        biased;
                        reply = &mut pending => {
                            let (status, body) = match reply {
                                Ok(reply) => reply,
                                Err(_) => (0, None), // a transport failure: status zero, never a receipt
                            };
                            break match terminal_receipt(action, slot, &filename, status, body.as_ref()) {
                                Some(terminal) => Released::Verified(terminal),
                                None => Released::Fenced(quarantined(&endpoint)),
                            };
                        }
                        () = &mut condemned => break Released::Condemned,
                        _ = checkpoints.tick() => {
                            // `/slots` adds nothing here: the request itself is the engine's
                            // answer. Its process says whether it still works.
                            if liveness.observe(EngineProbe::Busy) == EngineProbe::Stopped {
                                break Released::Fenced(quarantined(&endpoint));
                            }
                        }
                    }
                }
            };
            continuum_base::probe!(
                class = "inference.kv_page.unresolved_released",
                endpoint = %root,
                action = ?action,
                slot = slot as u64,
                held_ms = held.elapsed().as_millis() as u64,
                released_by = match released {
                    Released::Verified(Terminal::Completed) => "receipt_completed",
                    Released::Verified(Terminal::Rejected) => "receipt_rejected",
                    Released::Fenced(Quarantined(())) => "quarantined",
                    Released::Condemned => "condemned",
                },
                "an unresolved page switch let its slot go: the page stays held as the unresolved switch left it"
            );
            complete(released, custody);
        });
    }
}
