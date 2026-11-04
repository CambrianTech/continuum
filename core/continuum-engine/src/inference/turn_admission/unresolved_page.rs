//! A page switch the engine is still working on, with no receipt yet: the request in flight AND
//! the physical custody of its slot, which travel together and are released together.
//!
//! Codex on #5772: an unresolved switch that kept only its HTTP future released the slot's
//! permit, pin and endpoint reader when the turn returned its error, so a later turn could
//! reuse the slot while the old restore was still queued in the engine (a late restore then
//! overwrites another activity's KV), and a lifecycle transition no longer waited on it. So the
//! only way to consume an [`UnresolvedPage`] is [`UnresolvedPage::hold`], which moves the
//! custody (a turn's whole admission: lane permit, slot permit, pin, endpoint reader) into the
//! endpoint's paging task beside the request. The compiler then forbids reusing the slot while
//! it lives, and a transition's writer waits on its reader as it waits on any act in flight.
//!
//! The custody is released only by:
//! - the receipt: the engine answered the request (or its socket closed under it);
//! - the engine's own process, fenced by pid and start time: gone, or no CPU spent while the
//!   request is unanswered. Paging is then unverified, so the endpoint is quarantined first;
//! - a condemnation: the serving daemon decided to replace this engine, and its relaunch kills
//!   the process that holds the request before any successor serves (the replacement's fence),
//!   while its drain waits for every reader, this one included, to let go.

use std::time::Duration;

use super::page_liveness::{PageLiveness, PageReply, Pending};
use crate::inference::llama_server::EngineProbe;

/// The request of a page switch that outlived its measured bound while the engine's process
/// kept spending CPU, with what it needs to judge the engine until the receipt arrives.
#[must_use = "an unresolved page switch is held with its slot's custody until its receipt (UnresolvedPage::hold)"]
pub(crate) struct UnresolvedPage {
    pub(super) root: String,
    pub(super) action: String,
    pub(super) slot: u32,
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

/// What released a held custody.
enum Released {
    /// The engine answered: the HTTP status, 0 for a transport failure.
    Receipt(u16),
    /// The engine's process is gone or stopped working with the request unanswered.
    Stopped,
    /// The serving daemon is replacing the engine.
    Condemned,
}

impl UnresolvedPage {
    /// The CPU the engine's process spent over the checkpoint that left this switch unresolved.
    pub(crate) fn cpu_spent(&self) -> Duration {
        self.cpu_spent
    }

    /// Move `custody` into the endpoint's paging task beside the request; both are released
    /// together by the receipt, a stopped engine or a condemnation (module docs). `custody` is
    /// whatever holds the slot: a turn's admission, or `()` where the caller already holds the
    /// whole endpoint (a checkpoint inside a lifecycle writer that retires this engine next).
    pub(crate) fn hold<C: Send + 'static>(self, custody: C) {
        let Self { root, action, slot, cpu_spent: _, checkpoint, mut pending, liveness } = self;
        let endpoint = crate::inference::slots::directory().endpoint(&root);
        let condemnations = crate::inference::llama_server::condemnations();
        let condemned_already = crate::inference::llama_server::engine_condemned();
        let owner = std::sync::Arc::clone(&endpoint);
        owner.retain_paging(async move {
            let held = std::time::Instant::now();
            let released = if condemned_already {
                Released::Condemned
            } else {
                let condemned = crate::inference::llama_server::until_condemned_on(condemnations);
                tokio::pin!(condemned);
                let mut checkpoints = tokio::time::interval_at(tokio::time::Instant::now() + checkpoint, checkpoint);
                checkpoints.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                loop {
                    tokio::select! {
                        biased;
                        reply = &mut pending => break Released::Receipt(reply.map_or(0, |(status, _)| status)), // map_or: status zero records a transport failure, never success
                        () = &mut condemned => break Released::Condemned,
                        _ = checkpoints.tick() => {
                            // `/slots` adds nothing here: the request itself is the engine's
                            // answer. Its process says whether it still works.
                            if liveness.observe(EngineProbe::Busy) == EngineProbe::Stopped {
                                break Released::Stopped;
                            }
                        }
                    }
                }
            };
            if matches!(released, Released::Stopped) {
                // The request may still land on whatever process holds the slot next, or never:
                // paging is unverified until a verified replacement, as for an uncertain switch.
                endpoint.quarantine_paging();
            }
            continuum_base::probe!(
                class = "inference.kv_page.unresolved_released",
                endpoint = %root,
                action = %action,
                slot = slot as u64,
                held_ms = held.elapsed().as_millis() as u64,
                released_by = match released {
                    Released::Receipt(_) => "receipt",
                    Released::Stopped => "engine_stopped",
                    Released::Condemned => "condemned",
                },
                status = match released { Released::Receipt(status) => u64::from(status), _ => 0 },
                "an unresolved page switch let its slot go: the page stays held as the unresolved switch left it"
            );
            drop(custody);
        });
    }
}
