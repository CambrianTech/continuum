//! The dispatcher: one task owns the scheduler and wakes only on events (a submission, a release,
//! a resize) or the next aging instant. Callers hold no lock while they wait or while they work:
//! `submit` returns a grant when the scheduler starts it; dropping the grant (or an unstarted
//! submission) is the release event.

use crate::scheduler::{Aging, Decision, Dropped, Scheduler, Ticket};
use crate::Qos;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::Instant;

enum Event<K> {
    Submit { ticket: Ticket, owner: K, qos: Qos, start: oneshot::Sender<Result<watch::Receiver<bool>, Dropped>> },
    Release(Ticket),
    Resize(usize),
}

/// A handle to one capacity's dispatcher. Cheap to clone; every clone feeds the same queue.
pub struct Flow<K> {
    events: mpsc::UnboundedSender<Event<K>>,
    tickets: Arc<AtomicU64>,
}

impl<K> Clone for Flow<K> {
    fn clone(&self) -> Self {
        Self { events: self.events.clone(), tickets: Arc::clone(&self.tickets) }
    }
}

/// Why a submission did not start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// It left the queue without running (see [`Dropped`]).
    Dropped(Dropped),
    /// The dispatcher has stopped.
    Stopped,
}

/// Permission to use one unit of capacity, held while working. Dropping it frees the unit.
pub struct Grant {
    ticket: Ticket,
    events: Box<dyn Fn(Ticket) + Send + Sync>,
    yield_asked: watch::Receiver<bool>,
}

impl Grant {
    /// Resolves when a higher class is waiting and nothing is free: yield at the next safe
    /// point (for a sequence, a decode-step boundary) by dropping this grant.
    pub async fn yield_requested(&mut self) {
        while !*self.yield_asked.borrow_and_update() {
            if self.yield_asked.changed().await.is_err() {
                return std::future::pending().await;
            }
        }
    }

    pub fn yield_is_requested(&self) -> bool {
        *self.yield_asked.borrow()
    }
}

impl Drop for Grant {
    fn drop(&mut self) {
        (self.events)(self.ticket);
    }
}

/// Releases an unstarted submission if its caller stops waiting.
struct Pending<K> {
    ticket: Ticket,
    events: mpsc::UnboundedSender<Event<K>>,
    armed: bool,
}

impl<K> Drop for Pending<K> {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.events.send(Event::Release(self.ticket)); // a stopped dispatcher has nothing to release
        }
    }
}

impl<K: Clone + Eq + Hash + Send + 'static> Flow<K> {
    /// Start the dispatcher for a capacity of `capacity` units.
    pub fn spawn(capacity: usize, aging: Aging) -> Self {
        let (events, inbox) = mpsc::unbounded_channel();
        tokio::spawn(dispatch(Scheduler::new(capacity, aging), inbox));
        Self { events, tickets: Arc::new(AtomicU64::new(0)) }
    }

    /// Queue work for `owner` at `qos`; resolves when it may start, or at once with why it left
    /// the queue without running (`Refused::Dropped`: superseded by the owner's newer work, or
    /// shed from a full utility/background queue), or `Refused::Stopped` if the dispatcher has.
    pub async fn submit(&self, owner: K, qos: Qos) -> Result<Grant, Refused> {
        let ticket = Ticket(self.tickets.fetch_add(1, Ordering::Relaxed));
        let (start, started) = oneshot::channel();
        self.events.send(Event::Submit { ticket, owner, qos, start }).map_err(|_| Refused::Stopped)?;
        let mut pending = Pending { ticket, events: self.events.clone(), armed: true };
        let outcome = started.await.map_err(|_| Refused::Stopped)?;
        pending.armed = false;
        let yield_asked = outcome.map_err(Refused::Dropped)?;
        let events = self.events.clone();
        Ok(Grant {
            ticket,
            events: Box::new(move |t| {
                let _ = events.send(Event::Release(t)); // a stopped dispatcher has nothing to release
            }),
            yield_asked,
        })
    }

    /// The capacity changed (an engine relaunched with another slot count).
    pub fn resize(&self, capacity: usize) {
        let _ = self.events.send(Event::Resize(capacity)); // a stopped dispatcher has no capacity
    }
}

async fn dispatch<K: Clone + Eq + Hash>(mut scheduler: Scheduler<K>, mut inbox: mpsc::UnboundedReceiver<Event<K>>) {
    let mut starting: HashMap<Ticket, oneshot::Sender<watch::Receiver<bool>>> = HashMap::new();
    let mut yielding: HashMap<Ticket, watch::Sender<bool>> = HashMap::new();
    loop {
        let now = Instant::now();
        for decision in scheduler.decide(now.into_std()) {
            match decision {
                Decision::Run(ticket) => {
                    let (asked, yield_asked) = watch::channel(false);
                    match starting.remove(&ticket).map(|start| start.send(Ok(yield_asked))) {
                        Some(Ok(())) => {
                            yielding.insert(ticket, asked);
                        }
                        // the submitter stopped waiting before its start: free the unit again
                        _ => scheduler.release(ticket),
                    }
                }
                Decision::Yield(ticket) => {
                    if let Some(asked) = yielding.get(&ticket) {
                        let _ = asked.send(true); // a grant already dropped has yielded
                    }
                }
            }
        }
        let wake = scheduler.next_wakeup(now.into_std()).map(Instant::from_std);
        let event = tokio::select! {
            event = inbox.recv() => event,
            () = async { match wake { Some(at) => tokio::time::sleep_until(at).await, None => std::future::pending().await } } => continue,
        };
        match event {
            None => return,
            Some(Event::Submit { ticket, owner, qos, start }) => {
                starting.insert(ticket, start);
                for (gone, why) in scheduler.submit(ticket, owner, qos, now.into_std()) {
                    if let Some(start) = starting.remove(&gone) {
                        let _ = start.send(Err(why)); // a submitter that stopped waiting needs no reason
                    }
                }
            }
            Some(Event::Release(ticket)) => {
                starting.remove(&ticket);
                yielding.remove(&ticket);
                scheduler.release(ticket);
            }
            Some(Event::Resize(capacity)) => scheduler.resize(capacity),
        }
    }
}
