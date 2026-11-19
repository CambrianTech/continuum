//! The producer side of a stage that never waits: a thread that must not block (a frame loop, an
//! engine thread) hands items over with [`Outbox::send`] and goes on. When the stage is full, items
//! wait here, in order, and go over before anything newer at the next [`Outbox::flush`]. Every
//! producer with that rule uses this one object instead of keeping its own backlog.

use std::collections::VecDeque;

use crate::{PushError, StageInput};

pub struct Outbox<T> {
    input: StageInput<T>,
    backlog: VecDeque<T>,
}

impl<T> Outbox<T> {
    pub fn new(input: StageInput<T>) -> Self {
        Self { input, backlog: VecDeque::new() }
    }

    /// Hand `item` over without waiting; behind anything already waiting here.
    pub fn send(&mut self, item: T) {
        if !self.backlog.is_empty() {
            self.backlog.push_back(item);
            return;
        }
        match self.input.push(item) {
            Ok(_) => {}
            Err(PushError::Full(item)) => self.backlog.push_back(item),
            Err(PushError::Stopped(_)) => {} // the stage ended: nothing will take it
        }
    }

    /// Hand over what is waiting, in order, until it is empty or the stage is full again.
    pub fn flush(&mut self) {
        hand_over(&self.input, &mut self.backlog);
    }

    /// Hand over everything waiting, awaiting the stage's space between pushes: for a producer with
    /// nothing else to do, woken when the stage takes what it holds, never by a poll. Cancel-safe: an
    /// item leaves the backlog only when the stage has taken it, so a producer that stops waiting (a
    /// submission woke it first) loses nothing and keeps the order.
    pub async fn drain(&mut self) {
        loop {
            // Registered before the flush, so a take between the flush and the wait is never missed.
            let space = self.input.space();
            tokio::pin!(space);
            space.as_mut().enable();
            hand_over(&self.input, &mut self.backlog);
            if self.backlog.is_empty() {
                return;
            }
            space.await;
        }
    }

    /// Items still waiting here because the stage was full.
    pub fn waiting(&self) -> usize {
        self.backlog.len()
    }
}

/// Push what waits, in order, until it is empty or the stage is full again; a stopped stage takes
/// nothing more, so what waits for it is dropped with it.
fn hand_over<T>(input: &StageInput<T>, backlog: &mut VecDeque<T>) {
    while let Some(item) = backlog.pop_front() {
        match input.push(item) {
            Ok(_) => {}
            Err(PushError::Full(item)) => {
                backlog.push_front(item);
                return;
            }
            Err(PushError::Stopped(_)) => {
                backlog.clear();
                return;
            }
        }
    }
}
