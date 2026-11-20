//! One shadow's optimizer buffers as native code sizes them: a slot per role, holding what the memory
//! owner has charged for it. Native code never reports an increment.
//! `ggml_opt_static_allocation_allowed` (ggml-opt.cpp:558-565 at the pin) passes the WHOLE size of the
//! buffer it is about to allocate, so a request means "this slot is about to be N bytes": only the
//! growth past what is charged is admitted, and a revisit at the same size charges nothing new.
//!
//! A charge is held until native code proves what survived. Requests come from `llama_opt_init` and
//! from the graph builds inside learn steps; a refusal can leave earlier requests of the same group
//! unallocated, and `optimizer_host` is freed (:627) before it is asked for again (:908). So nothing
//! is released on inference: after each optimizer operation the context's owner visits the buffers
//! native code actually owns (`llama_opt_visit_memory`, from `ggml_opt_visit_owned_buffers`
//! :1021-1026, which names only `optimizer_static` and `optimizer_host`), and each slot is
//! reconciled to that. Charged but not resident is released; resident but never charged is a loud
//! defect.

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

    pub(super) const ALL: [Self; 2] = [Self::Static, Self::Host];

    fn index(self) -> usize {
        match self {
            Self::Static => 0,
            Self::Host => 1,
        }
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

/// Native code owns more of a role than was ever admitted for it: accounting is wrong, never quietly
/// absorbed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Unadmitted {
    pub(super) role: Role,
    pub(super) bytes: u64,
}

/// One role's slot: the bytes the lease admitted for it, and the device they were charged on. What
/// native code owns beyond them was never admitted: each visit reports it, and it is never released.
#[derive(Debug, Default)]
struct Charged {
    bytes: Cell<u64>,
    device: Cell<Option<BufferType>>,
}

/// One shadow's slots.
#[derive(Debug, Default)]
pub(super) struct Slots([Charged; 2]);

impl Slots {
    /// What one role is charged now; read by the tests that pin each role's accounting.
    #[cfg(test)]
    pub(super) fn charged(&self, role: Role) -> u64 {
        self.0[role.index()].bytes.get()
    }

    /// Native code asks for `slot` to be `bytes`: admit only the growth past what is charged. true:
    /// admitted (or nothing new to admit). A smaller request releases nothing yet: the visit proves it.
    pub(super) fn ask(&self, lease: &dyn OptimizerLease, slot: Slot, bytes: u64) -> bool {
        let charged = &self.0[slot.role.index()];
        let grow = bytes.saturating_sub(charged.bytes.get());
        if grow > 0 {
            if !lease.admit(slot, grow) {
                return false;
            }
            charged.bytes.set(bytes);
        }
        charged.device.set(Some(slot.device));
        true
    }

    /// Native code owns `resident` bytes of `role` now (from the owner's visit): release what was
    /// admitted but did not survive. More resident than admitted is a defect, reported on each visit
    /// and never added to the charge: it was never granted, so a release never exceeds what was
    /// admitted.
    pub(super) fn reconcile(&self, lease: &dyn OptimizerLease, role: Role, resident: u64) -> Option<Unadmitted> {
        let charged = &self.0[role.index()];
        let admitted = charged.bytes.get();
        if admitted > resident {
            if let Some(device) = charged.device.get() {
                lease.release(Slot { role, device }, admitted - resident);
            }
            charged.bytes.set(resident);
        }
        let excess = resident.saturating_sub(charged.bytes.get());
        (excess > 0).then_some(Unadmitted { role, bytes: excess })
    }

    /// Native code confirmed no optimizer references the shadow: release every admitted byte (an
    /// excess was never granted, so it is dropped, not released).
    pub(super) fn release_all(&self, lease: &dyn OptimizerLease) {
        for role in Role::ALL {
            let charged = &self.0[role.index()];
            let had = charged.bytes.replace(0);
            if let (true, Some(device)) = (had > 0, charged.device.get()) {
                lease.release(Slot { role, device }, had);
            }
        }
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

    /// What the owner's visit found resident: (static, host).
    fn visited(lease: &Recorded, slots: &Slots, resident: (u64, u64)) -> Vec<Unadmitted> {
        [(Role::Static, resident.0), (Role::Host, resident.1)]
            .into_iter()
            .filter_map(|(role, bytes)| slots.reconcile(lease, role, bytes))
            .collect()
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
        assert!(visited(&lease, &a, (270_368, 32)).is_empty());
        // The revisit: static exists (not asked, :906); host freed and asked again at the same size.
        assert!(a.ask(&lease, on(Role::Host), 32));
        assert!(visited(&lease, &a, (270_368, 32)).is_empty());
        assert_eq!(lease.charged(), 270_400, "exactly the two buffers, once");
        assert_eq!(lease.entries(), 2, "the revisit admitted and released nothing");
    }

    // what this catches: a device preflight then a host refusal in one group leaving the static
    // charge in place for a buffer native code never allocated, or releasing it before native code
    // proved it absent.
    #[test]
    fn a_refused_group_is_released_only_when_the_visit_proves_nothing_survived() {
        let lease = Recorded::with(270_368);
        let a = Slots::default();
        assert!(a.ask(&lease, on(Role::Static), 270_368));
        assert!(!a.ask(&lease, on(Role::Host), 32), "over budget");
        assert_eq!(lease.charged(), 270_368, "held until native code proves what survived");
        assert!(visited(&lease, &a, (0, 0)).is_empty());
        assert_eq!((a.charged(Role::Static), a.charged(Role::Host), lease.charged()), (0, 0, 0));
    }

    // what this catches: a refused host request on a revisit keeping the old host bytes charged,
    // though native code freed that buffer before asking (:627); and the static slot, which native
    // code keeps, released with it.
    #[test]
    fn a_refused_host_revisit_releases_the_freed_host_and_keeps_the_static() {
        let lease = Recorded::with(270_400);
        let a = Slots::default();
        assert!(a.ask(&lease, on(Role::Static), 270_368) && a.ask(&lease, on(Role::Host), 32));
        assert!(visited(&lease, &a, (270_368, 32)).is_empty());
        assert!(!a.ask(&lease, on(Role::Host), 64), "the revisit needs 32 more than the budget holds");
        assert!(visited(&lease, &a, (270_368, 0)).is_empty());
        assert_eq!((a.charged(Role::Static), a.charged(Role::Host), lease.charged()), (270_368, 0, 270_368));
    }

    // what this catches: a smaller buffer keeping the larger charge after native code proved it
    // smaller, or a confirmed release leaving any slot charged.
    #[test]
    fn a_proven_shrink_releases_the_difference_and_a_release_frees_every_slot() {
        let lease = Recorded::with(u64::MAX);
        let a = Slots::default();
        assert!(a.ask(&lease, on(Role::Static), 1_000) && a.ask(&lease, on(Role::Host), 64));
        assert!(a.ask(&lease, on(Role::Host), 16), "a smaller request releases nothing yet");
        assert_eq!(lease.charged(), 1_064);
        assert!(visited(&lease, &a, (1_000, 16)).is_empty());
        assert_eq!(lease.charged(), 1_016);
        a.release_all(&lease);
        assert_eq!(lease.charged(), 0);
    }

    // what this catches (Codex on #6149 at 69c07dd05): native code owning bytes nobody admitted
    // absorbed quietly, or folded into the charge so a later free releases bytes never granted
    // (admit 32, observe 64, free to 0 released 64).
    #[test]
    fn resident_bytes_never_admitted_are_a_defect_and_never_released() {
        let lease = Recorded::with(u64::MAX);
        let a = Slots::default();
        assert!(a.ask(&lease, on(Role::Host), 32));
        assert_eq!(visited(&lease, &a, (0, 64)), vec![Unadmitted { role: Role::Host, bytes: 32 }]);
        assert_eq!(a.charged(Role::Host), 32, "the charge stays what was admitted");
        assert!(visited(&lease, &a, (0, 0)).is_empty());
        assert_eq!(lease.charged(), 0, "released exactly the 32 admitted, never the 64 observed");

        // The same through a confirmed release.
        let b = Slots::default();
        assert!(b.ask(&lease, on(Role::Host), 32));
        assert_eq!(visited(&lease, &b, (0, 64)).len(), 1);
        b.release_all(&lease);
        assert_eq!(lease.charged(), 0, "a release never exceeds what was admitted");
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
