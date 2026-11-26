//! A seat gives its memory back when the law asks (UNIFIED-PAGING.md "Room by priority", step 3 part 6).
//! Every lease a seat holds is elastic at the Background class: a higher wait short of memory asks it to
//! give back, and the seat answers at once that it heard. It never aborts a lesson: the lesson in flight
//! lands, what she learned is checkpointed and committed, and the seat is released, its Released dropping
//! every guard, which is the fence the wait was granted on. The bucket's examples stay pending and the
//! next pass seats her again (Joel's rule: learning is continuous, her serving untouched).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::genome::learner::seat::{Authority, LeasedMemory};
use crate::resources::{Elastic, LeaseError, ReclaimOutcome, ShrinkDue, ShrinkHolder};

/// Learning is Background work: every lease of a seat may give back all of itself, first of all.
const LEARNING: Elastic = Elastic { floor: 0, priority: continuum_sage::Qos::Background as u8 };

/// Whether the law has asked a seat to give back: set by its holder, read at the seat's next act.
#[derive(Clone, Default)]
pub(super) struct AskedBack(Arc<AtomicBool>);

impl AskedBack {
    pub(super) fn asked(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// The holder the authority asks for one seat's leases.
struct Seat(AskedBack);

impl ShrinkHolder for Seat {
    fn shrink(&self, due: &ShrinkDue) -> ReclaimOutcome {
        self.0 .0.store(true, Ordering::Release);
        continuum_base::probe!(class = "training.on_engine.asked_back", lease = %due.lease_id, held = due.held, to = due.to,
            "a higher wait asked a learning seat to give back: it finishes its lesson, saves what it learned and is released");
        ReclaimOutcome::deferred("the seat finishes its lesson, checkpoints and is released")
    }
}

/// Make every lease in `leased` elastic, answered by `asked`.
pub(super) fn yield_when_asked(authority: &Authority, leased: &LeasedMemory, asked: &AskedBack) -> Result<(), LeaseError> {
    for (_, _, guard) in &leased.leases {
        authority.resources.hold_elastic(guard, LEARNING, Arc::new(Seat(asked.clone())))?;
    }
    Ok(())
}
