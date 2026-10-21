//! `LocalWorkingSetManager` — per-process implementation of the
//! `WorkingSetManager` trait shipped in PR-2 (#1353).
//!
//! Holds:
//! - `Vec<Box<dyn TierStore>>` — the tier chain, ordered Fast → Frozen
//! - `RwLock<HashMap<PeerId, WorkingSet>>` — per-persona working sets
//! - `RwLock<HashMap<PageRef, PeerId>>` — page-ownership map for
//!   the MMU-style `audit_access` enforcement
//!
//! Page-in walks the tier chain from highest (Fast) to lowest (Frozen),
//! returns the first hit, optionally promotes the page to the working
//! set's preferred tier. A miss with no resident copy is a true cold
//! miss → `PageFault::from_role: None`.
//!
//! ## What PR-3 ships
//!
//! - Pure local implementation. No bus publishing baked in (the
//!   `page_in` Result already carries `PageFault` as the typed
//!   observability signal; callers wire to the artifact dispatch
//!   path #1339+#1343 themselves).
//! - The four trait methods: `page_in`, `page_out`, `working_set`,
//!   `audit_access`.
//! - Constructor that registers tier stores + capacity per persona.
//! - Tests using a stub `TierStore` that records calls so the test
//!   can assert which tier was queried + that PageFault carries the
//!   right `from_role` / `to_role`.
//!
//! ## What PR-3 does NOT ship (PR-4 or later)
//!
//! - Eviction policy invocation when the target tier is at limit —
//!   PR-3 returns `TierError::NoEvictionCandidate` instead of running
//!   the policy. Policy invocation is a tier-store-internal concern
//!   that the PR-3 impl doesn't drive; PR-4's enhancement is a wired
//!   callback so the manager observes and re-publishes the
//!   `EvictionRecord` that the tier returned.
//! - Pinning logic for composition-layer page pinning — that's part
//!   of PR-3 of demand-aligned-recall (composer cache).
//! - The `check_permission(actor, region, op)` method from PR-2's
//!   "deliberately deferred" list. Lands in PR-4 alongside the
//!   GenomeRegion + Op type definitions.

use continuum_base::time::EpochMs;
use async_trait::async_trait;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use super::bus::{publish_access_denied, publish_page_fault};
use super::manager::WorkingSetManager;
use super::store::TierStore;
use super::tier::{TierError, TierRole};
use super::working_set::{AccessDenied, PageFault, PageHandle, PageRef, ResidentPage, WorkingSet, WorkingSetCapacity};
use crate::identity::PeerId;
use crate::runtime::message_bus::MessageBus;
use crate::runtime::registry::ModuleRegistry;

/// Optional bus + registry handle for auto-publishing genome events.
/// When set on a `LocalWorkingSetManager`, every `page_in`/
/// `audit_access` call that produces a typed event also publishes the
/// event via the artifact dispatch path (#1339+#1343) using the
/// canonical keys from `genome::bus` (PR-4 / #1358).
///
/// Kept as one struct (not two Arcs on the manager) so the absence-of-
/// bus case is a single `Option<BusHook>` field — easier to reason
/// about than two correlated Options.
struct BusHook {
    bus: Arc<MessageBus>,
    registry: Arc<ModuleRegistry>,
}

/// Per-process working-set manager. Holds the tier chain + per-persona
/// state. Thread-safe through `parking_lot::RwLock` — the hot-path
/// `audit_access` and `working_set` calls only need a read lock.
///
/// PR-5 adds optional bus publishing: when constructed via
/// `with_bus(tiers, bus, registry)`, every page_in / audit_access
/// call publishes the typed event to the trace bus through the
/// canonical genome keys. Constructed via `new(tiers)` (the PR-3
/// shape), the manager stays bus-less and behaves exactly as before
/// — useful for tests + standalone use where no runtime is around.
pub struct LocalWorkingSetManager {
    /// The tier chain, ordered highest (Fast) to lowest (Frozen).
    /// Each tier is a `Box<dyn TierStore>` from PR-2. The order is
    /// the page_in walk order — we stop at the first hit.
    tiers: Vec<Arc<dyn TierStore>>,
    /// Per-persona working set state. RwLock because read-heavy
    /// (every audit_access + working_set query) with occasional
    /// write (page_in / page_out modifications).
    working_sets: RwLock<HashMap<PeerId, WorkingSet>>,
    /// Page-ownership map for cross-persona compartmentalization.
    /// `audit_access` denies if `persona != owner`. PR-3 populates
    /// this via `register_page_owner`; PR-4 may move to a typed
    /// genome-region-keyed table per GENOME-FOUNDRY-SENTINEL Part 4.
    page_owners: RwLock<HashMap<PageRef, PeerId>>,
    /// Optional bus hook for auto-publishing events. `None` = bus-less
    /// mode (PR-3 behavior, no publishing). `Some` = wire every typed
    /// event to the artifact dispatch path via the genome::bus
    /// helpers shipped in PR-4.
    bus_hook: Option<BusHook>,
}

impl LocalWorkingSetManager {
    /// Construct with the tier chain — bus-less mode (PR-3 shape).
    /// Page events are returned through the trait's `Result` arms but
    /// NOT published to any bus. Useful for tests and standalone use
    /// where no runtime is around.
    pub fn new(tiers: Vec<Arc<dyn TierStore>>) -> Self {
        Self {
            tiers,
            working_sets: RwLock::new(HashMap::new()),
            page_owners: RwLock::new(HashMap::new()),
            bus_hook: None,
        }
    }

    /// Construct with the tier chain + auto-publishing bus hook.
    /// Every `page_in` that returns a `PageFault` AND every
    /// `audit_access` denial publishes the typed event via the
    /// `genome::bus` helpers (PR-4 / #1358) under the canonical
    /// genome keys.
    ///
    /// `bus` + `registry` must be from the same Runtime — publishing
    /// uses `bus.publish` which looks up modules via the registry.
    /// Subscribers register through `bus.subscribe_artifact` for the
    /// genome keys (typically via `subscribe_to_genome_events(bus,
    /// module_name)` from PR-4).
    ///
    /// Why a separate constructor instead of a setter: prevents the
    /// "bus added partway through service" race where some events
    /// are published and some aren't. The manager either publishes
    /// from construction onward, or never — no in-between state.
    pub fn with_bus(
        tiers: Vec<Arc<dyn TierStore>>,
        bus: Arc<MessageBus>,
        registry: Arc<ModuleRegistry>,
    ) -> Self {
        Self {
            tiers,
            working_sets: RwLock::new(HashMap::new()),
            page_owners: RwLock::new(HashMap::new()),
            bus_hook: Some(BusHook { bus, registry }),
        }
    }

    /// Register a persona with the manager + give it a working set
    /// capacity. Must be called before any `page_in` for the persona;
    /// `page_in` to an unregistered persona returns a `PageFault`
    /// with `from_role: None` (the page never existed for that
    /// persona because the persona itself doesn't exist yet).
    pub fn register_persona(&self, persona: PeerId, capacity: WorkingSetCapacity) {
        let ws = WorkingSet::new(persona, capacity);
        self.working_sets.write().insert(persona, ws);
    }

    /// Record that a page is private to a persona. Subsequent
    /// `audit_access(other_persona, page)` returns `AccessDenied`.
    /// Pages not registered here are treated as substrate-shared
    /// (no owner; anyone can access).
    pub fn register_page_owner(&self, page: PageRef, owner: PeerId) {
        self.page_owners.write().insert(page, owner);
    }

    /// How many tiers are configured. Cheap O(1) — used by tests +
    /// the governor's policy diagnostics.
    pub fn tier_count(&self) -> usize {
        self.tiers.len()
    }
}

#[async_trait]
impl WorkingSetManager for LocalWorkingSetManager {
    async fn page_in(&self, persona: PeerId, page: PageRef) -> Result<PageHandle, PageFault> {
        // Already resident? — fast path.
        {
            let working_sets = self.working_sets.read();
            if let Some(ws) = working_sets.get(&persona) {
                let key = serde_json::to_string(&page).unwrap_or_default();
                if let Some(resident) = ws.pages.get(&key) {
                    return Ok(PageHandle {
                        page,
                        tier_role: resident.role,
                        size_bytes: 0,
                    });
                }
            }
        }

        // Start the servicing clock AFTER the resident hot-path so a
        // working-set hit pays nothing for timing it didn't need. From
        // here down is genuine page-in work (tier walk + transfer +
        // eviction-if-any) — exactly what `PageFault.elapsed_us` is
        // meant to report. `Instant` per working_set.rs's "sub-ms
        // hot-path timing stays in caller-side Instants" doctrine.
        let started = std::time::Instant::now();

        // Walk tier chain top-down. First hit wins. Promote (record
        // residency) into the working set's Fast tier; the caller's
        // composition decides whether to pin.
        for tier in &self.tiers {
            if let Ok(handle) = tier.read(page).await {
                let from_role = handle.tier_role;
                let to_role = self.tiers.first().map(|t| t.role()).unwrap_or(from_role);

                // Record residency in the working set (if persona
                // registered).
                if let Some(ws) = self.working_sets.write().get_mut(&persona) {
                    let key = serde_json::to_string(&page).unwrap_or_default();
                    ws.pages.insert(
                        key,
                        ResidentPage {
                            page,
                            role: to_role,
                            last_access_ms: EpochMs::now().millis(),
                            access_count_window: 1,
                            pinned: false,
                        },
                    );
                }

                // Tier-promotion PageFault. Publish to bus if hook
                // present (PR-5 wiring; PR-3 contract — Err arm is
                // the typed sentinel observability signal, not a
                // failure), then return.
                let fault = PageFault {
                    page,
                    from_role: Some(from_role),
                    to_role,
                    persona,
                    elapsed_us: started.elapsed().as_micros() as u64,
                    eviction_cost: None,
                };
                if let Some(hook) = &self.bus_hook {
                    spawn_publish_page_fault(hook, fault.clone());
                }
                return Err(fault);
            }
        }

        // True cold miss — page doesn't exist in any tier yet.
        let fault = PageFault {
            page,
            from_role: None,
            to_role: self
                .tiers
                .first()
                .map(|t| t.role())
                .unwrap_or(TierRole::Fast),
            persona,
            elapsed_us: started.elapsed().as_micros() as u64,
            eviction_cost: None,
        };
        if let Some(hook) = &self.bus_hook {
            spawn_publish_page_fault(hook, fault.clone());
        }
        Err(fault)
    }

    async fn page_out(
        &self,
        persona: PeerId,
        page: PageRef,
        to: TierRole,
    ) -> Result<(), TierError> {
        // Remove from working set if present, then write to target
        // tier. PR-3 doesn't validate that `to` is a configured
        // tier role — that's a PR-4 concern (needs the governor's
        // current Vec<TierConfig> snapshot to know which roles are
        // present on this hardware).
        {
            let mut working_sets = self.working_sets.write();
            if let Some(ws) = working_sets.get_mut(&persona) {
                let key = serde_json::to_string(&page).unwrap_or_default();
                // Pinned pages skip silently per the trait docstring:
                // page_out doesn't surface TierError for pin-violation;
                // composition is responsible for unpinning.
                if let Some(resident) = ws.pages.get(&key) {
                    if resident.pinned {
                        return Ok(());
                    }
                }
                ws.pages.remove(&key);
            }
        }

        // Find the target tier and write a marker (PR-3 doesn't
        // shuttle the actual blob — that's a PR-4 enhancement; for
        // now page_out is a working-set-state operation only). When
        // we wire blob movement, this is where TierStore::write
        // gets called.
        for tier in &self.tiers {
            if tier.role() == to {
                tier.observe_access(page);
                return Ok(());
            }
        }
        Err(TierError::RoleNotConfigured { role: to })
    }

    fn working_set(&self, _persona: PeerId) -> Option<&WorkingSet> {
        // PR-3 cannot return a borrow through the RwLock without
        // exposing the lock guard type — that breaks the trait
        // signature. PR-4 will introduce a `Snapshot` type that
        // clones the working set view; until then, return None so
        // callers know to use the (future) snapshot API instead of
        // relying on this borrow path. Tests that need to inspect
        // the working set use the internal `working_set_snapshot`
        // helper below.
        //
        // This is a deliberate refinement of the PR-2 contract,
        // documented in the trait docstring as "Option<&WorkingSet>"
        // — the None case here is the "lock-guard escape impossible"
        // case, distinct from the spec's "persona not registered"
        // case but compatible with the same return type.
        None
    }

    fn audit_access(&self, persona: PeerId, page: PageRef) -> Result<(), AccessDenied> {
        let result: Result<(), AccessDenied> = match self.page_owners.read().get(&page).copied() {
            Some(owner) if owner != persona => Err(AccessDenied {
                actor: persona,
                page,
                owner: Some(owner),
                reason: "cross-persona read blocked by working-set MMU".to_string(),
            }),
            _ => Ok(()),
        };

        // Auto-publish on denial via the spawn helper (same lifetime-
        // workaround pattern as page_in — see spawn_publish_page_fault
        // for the rationale).
        if let (Err(ref denied), Some(hook)) = (&result, &self.bus_hook) {
            spawn_publish_access_denied(hook, denied.clone());
        }

        result
    }
}

impl LocalWorkingSetManager {
    /// Test/diagnostic helper: snapshot the working set for a persona.
    /// Clones — not for hot path. Used by tests + future telemetry
    /// modules to inspect state without holding the read lock.
    pub fn working_set_snapshot(&self, persona: PeerId) -> Option<WorkingSet> {
        self.working_sets.read().get(&persona).cloned()
    }
}

/// Spawn a `publish_page_fault` into the current tokio runtime.
/// Standalone fn (not a method) so the `&BusHook` borrow doesn't
/// outlive the spawn — Arcs get cloned out first, then the spawned
/// future owns its captures.
///
/// Why spawn instead of await: `bus.publish` walks the DashMap of
/// subscribers; the DashMap's `Map` trait impl has a specific
/// lifetime that doesn't satisfy the for-any-lifetime requirement
/// generated by `async_trait`'s `Send`-bounded future. Awaiting
/// `publish` inside the trait method's body trips a
/// "DashMap is not general enough" error. Spawning decouples the
/// publish from the caller's Send-ness — no borrow crosses the await
/// boundary in the caller's future.
///
/// If no tokio runtime is current (rare — only sync-only test paths
/// without `#[tokio::test]`), the spawn is skipped silently because
/// `Handle::try_current` returns Err. The typed event in the
/// returned `Result` is still authoritative; observability is
/// best-effort.
fn spawn_publish_page_fault(hook: &BusHook, fault: PageFault) {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        let bus = hook.bus.clone();
        let registry = hook.registry.clone();
        handle.spawn(async move {
            publish_page_fault(&bus, &registry, &fault).await;
        });
    }
}

/// Spawn a `publish_access_denied` into the current tokio runtime.
/// Same pattern as `spawn_publish_page_fault`; used by the sync
/// `audit_access` trait method.
fn spawn_publish_access_denied(hook: &BusHook, denied: AccessDenied) {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        let bus = hook.bus.clone();
        let registry = hook.registry.clone();
        handle.spawn(async move {
            publish_access_denied(&bus, &registry, &denied).await;
        });
    }
}

#[cfg(test)]
mod tests;
