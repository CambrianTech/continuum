//! One part's verbs run in the order they were admitted.
//!
//! Each part, at each generation, has a [`Sequence`]. Its owner issues a ticket for every verb it
//! admits, under the owner's lock, so tickets are in admission order. A task waits for its ticket's
//! turn before running and releases it when its verb has settled, or when it is dropped. Turns
//! advance only through tickets released in order, so a late ticket never runs ahead of an earlier
//! one still running, and a dropped task never stalls the ones behind it.

use std::collections::BTreeSet;
use std::sync::Arc;

use event_listener::Event;
use parking_lot::Mutex;

#[derive(Default)]
struct Tickets {
    issued: u64,
    /// The ticket whose turn it is.
    serving: u64,
    /// Tickets released while an earlier one was still running.
    released: BTreeSet<u64>,
}

/// One part's admission order.
#[derive(Default)]
pub(super) struct Sequence {
    tickets: Mutex<Tickets>,
    turn: Event,
}

impl Sequence {
    /// The next turn on this part. Taken under the owner's lock, so turns follow admission order.
    pub(super) fn ticket(self: &Arc<Self>) -> Turn {
        let ticket = {
            let mut tickets = self.tickets.lock();
            tickets.issued += 1;
            tickets.issued - 1
        };
        Turn { sequence: Arc::clone(self), ticket }
    }

    fn release(&self, ticket: u64) {
        {
            let mut tickets = self.tickets.lock();
            tickets.released.insert(ticket);
            loop {
                let serving = tickets.serving;
                if !tickets.released.remove(&serving) {
                    break;
                }
                tickets.serving += 1;
            }
        }
        self.turn.notify(usize::MAX);
    }
}

/// One verb's place in its part's order. Dropping it gives the place to the next verb.
pub(super) struct Turn {
    sequence: Arc<Sequence>,
    ticket: u64,
}

impl Turn {
    /// Wait until every verb admitted on this part before this one has settled.
    pub(super) async fn arrive(&self) {
        loop {
            let listener = self.sequence.turn.listen();
            if self.sequence.tickets.lock().serving == self.ticket {
                return;
            }
            listener.await;
        }
    }
}

impl Drop for Turn {
    fn drop(&mut self) {
        self.sequence.release(self.ticket);
    }
}
