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
        while let Some(item) = self.backlog.pop_front() {
            match self.input.push(item) {
                Ok(_) => {}
                Err(PushError::Full(item)) => {
                    self.backlog.push_front(item);
                    return;
                }
                Err(PushError::Stopped(_)) => {
                    self.backlog.clear();
                    return;
                }
            }
        }
    }

    /// Items still waiting here because the stage was full.
    pub fn waiting(&self) -> usize {
        self.backlog.len()
    }
}
