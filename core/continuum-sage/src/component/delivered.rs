//! What reaches a component's worker: an input with the frame it came from, or a request to rest,
//! each stamped with when it entered the intake.

use std::time::Instant;

use crate::dataflow::{Event, InputRevision, Snapshot};
use crate::StageInput;

/// What reaches a component's worker: an input (with the frame it came from: none for an act or a
/// timer), or the engine's request that it rest.
pub struct Delivered<I> {
    pub(super) what: What<I>,
    pub(super) from: Option<InputRevision>,
    /// When it entered the intake: how long it waited there is measured for every component.
    pub(super) queued: Instant,
}

pub(super) enum What<I> {
    Input(I),
    Rest,
}

impl<I> Delivered<I> {
    pub(super) fn act(input: I) -> Self {
        Self { what: What::Input(input), from: None, queued: Instant::now() }
    }

    pub(super) fn rest() -> Self {
        Self { what: What::Rest, from: None, queued: Instant::now() }
    }
}

impl<T, I: From<Event<T>>> From<Event<T>> for Delivered<I> {
    fn from(event: Event<T>) -> Self {
        let from = Some(event.revision);
        Self { what: What::Input(I::from(event)), from, queued: Instant::now() }
    }
}

impl<T, I: From<Snapshot<T>>> From<Snapshot<T>> for Delivered<I> {
    fn from(snapshot: Snapshot<T>) -> Self {
        let from = Some(snapshot.revision);
        Self { what: What::Input(I::from(snapshot)), from, queued: Instant::now() }
    }
}

/// A component's intake: its inputs with the revision each came from.
pub(super) type Intake<I> = StageInput<Delivered<I>>;
