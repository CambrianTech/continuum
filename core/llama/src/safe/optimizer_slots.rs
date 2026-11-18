//! One shadow's optimizer buffers as native code sizes them: a slot per role, holding the bytes its
//! buffer is. Native code never reports an increment. `ggml_opt_static_allocation_allowed`
//! (ggml-opt.cpp:558-565 at the pin) passes the WHOLE size of the buffer it is about to allocate, so
//! a request means "this slot is about to be N bytes": the growth is admitted before native
//! allocates, a shrink is released, and a revisit at the same size charges nothing new.
//!
//! The two roles differ in one fact. `optimizer_static` (gradient accumulators, AdamW moments) is
//! asked only while it does not exist yet (:906) and persists across replies. `optimizer_host` is
//! freed (:627) before it is asked for again on every init (:908), so a refused host request still
//! leaves that slot empty. Every request in one init precedes every allocation in it (:906-908, then
//! :912/:921), so a refusal later in the same init proves the earlier requests were never allocated,
//! and they are rolled back.

use std::cell::Cell;
use std::ffi::CStr;

use super::learn::BufferType;

/// What an optimizer-owned buffer is for, from the name native code gives it. A name this crate does
/// not know is refused, never guessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    /// Gradient accumulators and optimizer moments on the training backend: kept across replies.
    Static,
    /// The optimizer's host parameters: freed and made again on every init.
    Host,
}

impl Role {
    pub(super) fn of(kind: &CStr) -> Option<Self> {
        match kind.to_bytes() {
            b"optimizer_static" => Some(Self::Static),
            b"optimizer_host" => Some(Self::Host),
            _ => None,
        }
    }

    const ALL: [Self; 2] = [Self::Static, Self::Host];

    fn index(self) -> usize {
        match self {
            Self::Static => 0,
            Self::Host => 1,
        }
    }

    /// Native code frees this role's buffer before it asks for it again.
    fn freed_before_asked(self) -> bool {
        matches!(self, Self::Host)
    }
}

/// One optimizer buffer at the boundary: what it is for, and the allocator (device) it lives on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slot {
    pub role: Role,
    pub device: BufferType,
}

/// The memory owner's lease for ONE shadow's optimizer, kept with the shadow (never with a reply)
/// until [`super::Learner::release_shadow`] confirms native code let it go. It is told growth and
/// proven frees, never a total to re-derive.
pub trait OptimizerLease: Send {
    /// Admit `bytes` more for `slot` before native code allocates them. false refuses, and the step
    /// fails by name.
    fn admit(&self, slot: Slot, bytes: u64) -> bool;
    /// Native code proved `bytes` of `slot` freed.
    fn release(&self, slot: Slot, bytes: u64);
}

/// One role's slot: the bytes it holds, the device they are on, and what it held before the init in
/// progress asked for it.
#[derive(Debug, Default)]
struct Held {
    bytes: Cell<u64>,
    device: Cell<Option<BufferType>>,
    before: Cell<Option<u64>>,
}

/// One shadow's slots.
#[derive(Debug, Default)]
pub(super) struct Slots([Held; 2]);

impl Slots {
    pub(super) fn held(&self, role: Role) -> u64 {
        self.0[role.index()].bytes.get()
    }

    /// Native code asks for `slot` to be `bytes`. true: admitted and recorded.
    pub(super) fn ask(&self, lease: &dyn OptimizerLease, slot: Slot, bytes: u64) -> bool {
        let held = &self.0[slot.role.index()];
        let had = held.bytes.get();
        let grow = bytes.saturating_sub(had);
        if grow > 0 && !lease.admit(slot, grow) {
            if slot.role.freed_before_asked() && had > 0 {
                // Its old buffer is already gone: release it, so a refused request holds nothing.
                lease.release(Slot { role: slot.role, device: held.device.get().unwrap_or(slot.device) }, had); // unwrap_or: a slot holding bytes always recorded its device
                held.bytes.set(0);
            }
            self.refused(lease);
            return false;
        }
        let shrink = had.saturating_sub(bytes);
        if shrink > 0 {
            lease.release(Slot { role: slot.role, device: held.device.get().unwrap_or(slot.device) }, shrink); // unwrap_or: as above
        }
        if held.before.get().is_none() {
            held.before.set(Some(had));
        }
        held.bytes.set(bytes);
        held.device.set(Some(slot.device));
        true
    }

    /// The init ended (allocated, or failed for a reason other than a refusal here): what it asked
    /// for stays held until a proven free.
    pub(super) fn settle(&self) {
        for held in &self.0 {
            held.before.set(None);
        }
    }

    /// A request in this init was refused, so nothing this init asked for was allocated: undo it.
    fn refused(&self, lease: &dyn OptimizerLease) {
        for role in Role::ALL {
            let held = &self.0[role.index()];
            let Some(before) = held.before.take() else { continue };
            // A role freed before it is asked holds nothing now; any other holds what it held before.
            let now = if role.freed_before_asked() { 0 } else { before };
            let had = held.bytes.get();
            if let (true, Some(device)) = (had > now, held.device.get()) {
                lease.release(Slot { role, device }, had - now);
            }
            held.bytes.set(now);
        }
    }

    /// Native code confirmed no optimizer references the shadow: release every slot.
    pub(super) fn release_all(&self, lease: &dyn OptimizerLease) {
        for role in Role::ALL {
            let held = &self.0[role.index()];
            let had = held.bytes.replace(0);
            if let (true, Some(device)) = (had > 0, held.device.get()) {
                lease.release(Slot { role, device }, had);
            }
        }
        self.settle();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr::NonNull;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Mutex;

    /// Records every admit and release, refusing an admission past its budget.
    struct Recorded {
        budget: u64,
        charged: AtomicU64,
        log: Mutex<Vec<(Role, i128)>>,
    }

    impl Recorded {
        fn with(budget: u64) -> Self {
            Self { budget, charged: AtomicU64::new(0), log: Mutex::new(Vec::new()) }
        }
        fn charged(&self) -> u64 {
            self.charged.load(Ordering::Relaxed)
        }
        fn entries(&self) -> usize {
            self.log.lock().map(|log| log.len()).unwrap_or_default() // unwrap_or_default: a poisoned test log reads as empty and fails the assertion
        }
    }

    impl OptimizerLease for Recorded {
        fn admit(&self, slot: Slot, bytes: u64) -> bool {
            if self.charged().saturating_add(bytes) > self.budget {
                return false;
            }
            self.charged.fetch_add(bytes, Ordering::Relaxed);
            if let Ok(mut log) = self.log.lock() {
                log.push((slot.role, i128::from(bytes)));
            }
            true
        }
        fn release(&self, slot: Slot, bytes: u64) {
            self.charged.fetch_sub(bytes, Ordering::Relaxed);
            if let Ok(mut log) = self.log.lock() {
                log.push((slot.role, -i128::from(bytes)));
            }
        }
    }

    fn on(role: Role) -> Slot {
        Slot { role, device: BufferType(NonNull::dangling()) }
    }

    // what this catches (Codex's A/B/A trace on #6148 at 56115572a): a revisit asking for the
    // 32-byte host buffer again (native freed it at :627 and makes it at :921) charged as a new 32
    // bytes on top of the old, so a shadow switched back to many times grows a lease for memory that
    // no longer exists; and the persisting static slot charged twice.
    #[test]
    fn a_revisit_at_the_same_sizes_charges_nothing_new() {
        let lease = Recorded::with(u64::MAX);
        let a = Slots::default();
        assert!(a.ask(&lease, on(Role::Static), 270_368) && a.ask(&lease, on(Role::Host), 32));
        a.settle();
        // The revisit: static exists (not asked, :906); host freed and asked again at the same size.
        assert!(a.ask(&lease, on(Role::Host), 32));
        a.settle();
        assert_eq!(lease.charged(), 270_400, "exactly the two buffers, once");
        assert_eq!(lease.entries(), 2, "the revisit admitted and released nothing");
    }

    // what this catches: a device preflight then a host refusal in one init leaving the static
    // charge in place for a buffer native code never allocated (every request precedes every
    // allocation, :906-908 then :912/:921).
    #[test]
    fn a_host_refusal_rolls_back_the_static_charge_of_the_same_init() {
        let lease = Recorded::with(270_368);
        let a = Slots::default();
        assert!(a.ask(&lease, on(Role::Static), 270_368));
        assert!(!a.ask(&lease, on(Role::Host), 32), "over budget");
        assert_eq!((a.held(Role::Static), a.held(Role::Host)), (0, 0));
        assert_eq!(lease.charged(), 0, "nothing was allocated, so nothing stays charged");
    }

    // what this catches: a refused host request on a revisit keeping the old host bytes charged,
    // though native code freed that buffer before asking (:627); and the static slot, which native
    // code keeps, released with it.
    #[test]
    fn a_refused_host_revisit_releases_the_freed_host_and_keeps_the_static() {
        let lease = Recorded::with(270_400);
        let a = Slots::default();
        assert!(a.ask(&lease, on(Role::Static), 270_368) && a.ask(&lease, on(Role::Host), 32));
        a.settle();
        assert!(!a.ask(&lease, on(Role::Host), 64), "the revisit needs 32 more than the budget holds");
        assert_eq!((a.held(Role::Static), a.held(Role::Host)), (270_368, 0));
        assert_eq!(lease.charged(), 270_368);
    }

    // what this catches: a smaller buffer on a revisit keeping the larger charge, or a confirmed
    // release leaving any slot charged.
    #[test]
    fn a_shrink_releases_the_difference_and_a_release_frees_every_slot() {
        let lease = Recorded::with(u64::MAX);
        let a = Slots::default();
        assert!(a.ask(&lease, on(Role::Static), 1_000) && a.ask(&lease, on(Role::Host), 64));
        a.settle();
        assert!(a.ask(&lease, on(Role::Host), 16));
        a.settle();
        assert_eq!(lease.charged(), 1_016);
        a.release_all(&lease);
        assert_eq!(lease.charged(), 0);
    }

    // what this catches: an optimizer buffer name this crate does not know charged under a guessed
    // role instead of refused.
    #[test]
    fn only_the_two_native_names_have_a_role() {
        assert_eq!(Role::of(c"optimizer_static"), Some(Role::Static));
        assert_eq!(Role::of(c"optimizer_host"), Some(Role::Host));
        assert_eq!(Role::of(c"optimizer_moments"), None);
    }
}
