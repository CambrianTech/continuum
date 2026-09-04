//! `WorkingSetManager` trait — the top-level paging interface every
//! persona's cognition path calls. Per GENOME-FOUNDRY-SENTINEL Parts
//! 3 (paging) and 4 (compartmentalization).
//!
//! PR-2 of working-set-manager ships the **trait surface only**. The
//! per-persona implementation that holds the `Box<dyn TierStore>`
//! per role, services `page_in` by walking the tier chain, and
//! publishes `PageFault` / `EvictionRecord` events through the
//! artifact dispatch path (#1339+#1343) lands in PR-3.
//!
//! ## What the trait promises
//!
//! - `page_in` — promote a page into the persona's working set. May
//!   trigger eviction. On miss-with-no-eviction-candidate returns
//!   `PageFault` (used by sentinel to learn the persona's access
//!   pattern), not a generic error.
//! - `page_out` — demote a page out of the working set toward a
//!   named tier role. Used by the eviction policy + composition layer
//!   when it's done with a page.
//! - `working_set` — read-only snapshot of the persona's current
//!   resident pages. The hot path uses this to decide "do I need to
//!   page in or is it already there." Returns `&WorkingSet` (no
//!   clone) because the call is hot.
//! - `audit_access` — MMU-style permission check. Returns
//!   `AccessDenied` if the page is private to another persona. This
//!   is one of the four typed events audit-recorder (#1344)
//!   subscribes to.
//!
//! ## What's deliberately deferred
//!
//! `check_permission(actor, region, op)` from GENOME-FOUNDRY-
//! SENTINEL Part 4 lands in PR-3 alongside the GenomeRegion + Op
//! type definitions and the per-region permission matrix. PR-2 only
//! ships the four methods that don't need those types — keeping
//! the surface tight so this PR is reviewable on its own.

use async_trait::async_trait;

use super::tier::{TierError, TierRole};
use super::working_set::{AccessDenied, PageFault, PageHandle, PageRef, WorkingSet};
use crate::identity::PeerId;

/// The single trait every working-set implementation satisfies. The
/// PR-3 implementor will be a per-substrate-process singleton holding
/// the tier chain + per-persona `WorkingSet` state.
///
/// `Send + Sync` because every persona task calls into it
/// concurrently from the tokio runtime.
#[async_trait]
pub trait WorkingSetManager: Send + Sync {
    /// Promote a page into this persona's working set. May trigger
    /// eviction of other pages within the same working set.
    ///
    /// Returns `Ok(PageHandle)` when the page is now resident. The
    /// handle's `tier_role` tells the caller which tier the page
    /// lives in — the caller decides whether to pin it or stream it.
    ///
    /// Returns `Err(PageFault)` when the page wasn't already resident
    /// AND the manager had to do work to make it so. The PageFault
    /// is NOT an error in the failure sense — it's a typed signal
    /// for sentinel + composition observability. The caller treats it
    /// as success-with-trace-event. A future PR may relax this
    /// signature (e.g. return `Result<(PageHandle, Option<PageFault>),
    /// TierError>`) if downstream feedback wants both.
    async fn page_in(&self, persona: PeerId, page: PageRef) -> Result<PageHandle, PageFault>;

    /// Demote a page out of the working set toward the named tier
    /// role. Used by composition when it's done with a page (e.g.
    /// after a turn completes), and by the eviction policy when a
    /// higher tier needs the bytes.
    ///
    /// Returns `Err(TierError)` if the target tier can't accept the
    /// page (over-budget, role-not-configured, backing-store I/O).
    /// The pinned-page case is NOT a TierError — page_out skips
    /// pinned pages silently; the caller (composition) is responsible
    /// for unpinning before demoting.
    async fn page_out(&self, persona: PeerId, page: PageRef, to: TierRole)
        -> Result<(), TierError>;

    /// Read-only snapshot of the persona's current working set. The
    /// hot path uses this to decide "is the page I need already
    /// resident?" without paying the page_in cost.
    ///
    /// Returns `Option<&WorkingSet>` instead of `&WorkingSet`: a
    /// persona that has never been registered with this manager has
    /// no working set yet — returning `None` is cleaner than
    /// fabricating an empty one (which would mask "wrong persona id"
    /// bugs). The Part-3 spec uses `&WorkingSet` without the option;
    /// PR-2's narrower contract is a pragmatic refinement that catches
    /// the misuse case earlier.
    fn working_set(&self, persona: PeerId) -> Option<&WorkingSet>;

    /// MMU-style audit: the named persona is asking for the named
    /// page. Returns `Err(AccessDenied)` if the page is private to a
    /// different persona (cross-persona read attempt).
    ///
    /// This is one of the four typed events audit-recorder (#1344)
    /// subscribes to — every AccessDenied gets pinned to the audit
    /// log, regardless of whether the calling persona caught + logged
    /// it itself. Compartmentalization audit trail per
    /// GENOME-FOUNDRY-SENTINEL Part 4.
    fn audit_access(&self, persona: PeerId, page: PageRef) -> Result<(), AccessDenied>;
}

#[cfg(test)]
mod tests {
    //! Trait-shape tests: prove the trait is object-safe (usable as
    //! `Box<dyn WorkingSetManager>` / `Arc<dyn WorkingSetManager>`)
    //! and that a minimal implementor compiles + dispatches through
    //! the trait object. PR-3 will add the per-persona impl tested
    //! against real semantics; PR-2 only proves the seam.

    use super::*;
    use crate::genome::working_set::{ArtifactId, PageKind, PageOffset, WorkingSetCapacity};
    use std::collections::HashMap;
    use std::sync::Arc;
    use uuid::Uuid;

    /// Minimal stub manager for trait-shape tests. Backing storage:
    /// per-persona HashMap of "pages this persona owns" the audit_access
    /// check uses.
    struct StubManager {
        working_sets: HashMap<PeerId, WorkingSet>,
        /// (page, owner) — audit_access denies if `persona != owner`.
        page_owners: HashMap<PageRef, PeerId>,
    }

    #[async_trait]
    impl WorkingSetManager for StubManager {
        async fn page_in(&self, _persona: PeerId, page: PageRef) -> Result<PageHandle, PageFault> {
            // Stub: every page_in succeeds with a fresh handle. The
            // contract being tested is the signature shape, not the
            // page-resolution logic (PR-3's territory).
            Ok(PageHandle {
                page,
                tier_role: TierRole::Fast,
                size_bytes: 0,
            })
        }

        async fn page_out(
            &self,
            _persona: PeerId,
            _page: PageRef,
            _to: TierRole,
        ) -> Result<(), TierError> {
            Ok(())
        }

        fn working_set(&self, persona: PeerId) -> Option<&WorkingSet> {
            self.working_sets.get(&persona)
        }

        fn audit_access(&self, persona: PeerId, page: PageRef) -> Result<(), AccessDenied> {
            match self.page_owners.get(&page) {
                Some(owner) if *owner != persona => Err(AccessDenied {
                    actor: persona,
                    page,
                    owner: Some(*owner),
                    reason: format!("cross-persona read attempt blocked by working-set MMU"),
                }),
                _ => Ok(()),
            }
        }
    }

    fn sample_persona(low_bits: u128) -> PeerId {
        // Build a deterministic UUID from the low bits so tests can
        // construct distinct personas without depending on randomness.
        PeerId::from_uuid(Uuid::from_u128(low_bits))
    }

    fn sample_page() -> PageRef {
        PageRef {
            kind: PageKind::LoRALayer,
            artifact: ArtifactId::new(Uuid::nil()),
            offset: PageOffset::Whole,
        }
    }

    /// What this catches: WorkingSetManager is object-safe. If a
    /// future PR adds a generic method or a non-dyn-safe signature,
    /// this construction fails to compile. Load-bearing because the
    /// substrate holds a single `Arc<dyn WorkingSetManager>` and the
    /// persona-cognition module dispatches through it.
    #[tokio::test]
    async fn working_set_manager_is_object_safe() {
        let mgr: Arc<dyn WorkingSetManager> = Arc::new(StubManager {
            working_sets: HashMap::new(),
            page_owners: HashMap::new(),
        });
        let p = sample_persona(1);
        let handle = mgr.page_in(p, sample_page()).await.unwrap();
        assert_eq!(handle.tier_role, TierRole::Fast);
    }

    /// What this catches: working_set returns `None` for an
    /// unregistered persona. If the contract changes to fabricate
    /// an empty WorkingSet, callers lose the early-fail signal for
    /// "wrong persona id."
    #[tokio::test]
    async fn working_set_returns_none_for_unregistered_persona() {
        let mgr: Box<dyn WorkingSetManager> = Box::new(StubManager {
            working_sets: HashMap::new(),
            page_owners: HashMap::new(),
        });
        assert!(mgr.working_set(sample_persona(42)).is_none());
    }

    /// What this catches: working_set returns a borrow (not a clone)
    /// — the contract is `Option<&WorkingSet>`. The hot path can't
    /// afford a HashMap-clone per check.
    #[tokio::test]
    async fn working_set_returns_borrow_not_clone() {
        let persona = sample_persona(7);
        let ws = WorkingSet::new(
            persona,
            WorkingSetCapacity {
                fast_bytes: 1_000_000,
                warm_bytes: 0,
                max_pinned_bytes: 500_000,
            },
        );
        let mut working_sets = HashMap::new();
        working_sets.insert(persona, ws);
        let mgr: Box<dyn WorkingSetManager> = Box::new(StubManager {
            working_sets,
            page_owners: HashMap::new(),
        });
        let got = mgr.working_set(persona).unwrap();
        assert_eq!(got.persona, persona);
        assert!(got.pages.is_empty());
    }

    /// What this catches: audit_access returns Ok when the page has
    /// no owner OR the persona IS the owner. Same-persona access is
    /// always allowed at this layer (composition-layer concerns like
    /// pinning are separate).
    #[tokio::test]
    async fn audit_access_allows_own_pages_and_orphan_pages() {
        let owner = sample_persona(10);
        let mut page_owners = HashMap::new();
        page_owners.insert(sample_page(), owner);
        let mgr: Box<dyn WorkingSetManager> = Box::new(StubManager {
            working_sets: HashMap::new(),
            page_owners,
        });
        // Owner accessing own page: OK
        assert!(mgr.audit_access(owner, sample_page()).is_ok());
        // Different page (no recorded owner): OK
        let other_page = PageRef {
            kind: PageKind::Engram,
            artifact: ArtifactId::new(Uuid::from_u128(99)),
            offset: PageOffset::Whole,
        };
        assert!(mgr.audit_access(owner, other_page).is_ok());
    }

    /// What this catches: audit_access returns `AccessDenied` (the
    /// typed event) — NOT a generic error — when a persona tries to
    /// read a page another persona owns. PR-1 ships AccessDenied as
    /// the typed shape; PR-2 pins that the trait returns it.
    #[tokio::test]
    async fn audit_access_denies_cross_persona_read() {
        let owner = sample_persona(10);
        let intruder = sample_persona(20);
        let mut page_owners = HashMap::new();
        page_owners.insert(sample_page(), owner);
        let mgr: Box<dyn WorkingSetManager> = Box::new(StubManager {
            working_sets: HashMap::new(),
            page_owners,
        });
        let result = mgr.audit_access(intruder, sample_page());
        match result {
            Err(denied) => {
                assert_eq!(denied.actor, intruder);
                assert_eq!(denied.owner, Some(owner));
                assert!(denied.reason.contains("cross-persona"));
            }
            Ok(()) => panic!("expected AccessDenied, got Ok"),
        }
    }
}
