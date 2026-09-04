//! The organism's in-breath: a being that gets scheduled becomes
//! genome-resident, as a reflex, through the bus.
//!
//! `runtime::governor_bus` gave the scheduler an out-breath — every
//! pass it emits one `PersonaScheduled` per being it granted a cognitive
//! slice. This module is the free-living SUBSCRIBER that completes the
//! reflex arc: it binds to `PERSONA_SCHEDULED_KEY` and, when a being is
//! scheduled, pages that being's genome overlay (her LoRA stack)
//! resident through the EXISTING working-set pager. The governor never
//! learns this module exists; residency never calls the governor. The
//! only coupling is the wire.
//!
//! This is the load-bearing wire under "a persona is a genome overlay
//! multiplexed through ONE base model" ([[persona-is-a-genome-overlay-
//! not-an-instance]]). Multiplicity costs a LoRA **page-in**, not a
//! model load: N scheduled beings → N `PersonaScheduled` → N overlays
//! page-faulting through ONE `LocalWorkingSetManager`. The page-fault
//! each one emits (`genome::bus::PAGE_FAULT_KEY`) carries `elapsed_us` —
//! the measured proof that persona-N's residency cost O(page-in), not
//! O(model).
//!
//! ## What this module does NOT do
//!
//! - It does not resolve WHICH artifact a being's overlay is — that's
//!   `persona_overlay`, today a 1:1 `persona_uuid → ArtifactId` rail.
//!   When #32 lands a trained adapter and slice 14 records the genome
//!   ref in the seed, the resolution refines to "the artifact this
//!   being's genome points at" without touching the reflex arc.
//! - It does not own eviction, pinning, or capacity policy — the
//!   governor publishes `WorkingSetCapacity`; the manager enforces.
//!   Residency only ensures a scheduled being HAS a working set and her
//!   overlay is paged toward it.

use async_trait::async_trait;
use std::any::Any;
use std::sync::Arc;
use uuid::Uuid;

use super::local_manager::LocalWorkingSetManager;
use super::manager::WorkingSetManager;
use super::working_set::{ArtifactId, PageKind, PageOffset, PageRef, WorkingSetCapacity};
use crate::identity::PeerId;
use crate::runtime::artifact_handle::{ArtifactKey, ArtifactSelector};
use crate::runtime::governor_bus::{PersonaScheduled, PERSONA_SCHEDULED_KEY};
use crate::runtime::service_module::{CommandResult, ModuleConfig, ModulePriority, ServiceModule};
use crate::runtime::ModuleContext;

/// Subscribes to the governor's `PersonaScheduled` breath and pages the
/// scheduled being's genome overlay resident through the shared
/// working-set manager. One per process — it holds the `Arc` to the
/// same `LocalWorkingSetManager` the inference path reads residency
/// from, so a being scheduled here is a being whose overlay is warm
/// when her composition asks for it.
pub struct GenomeResidencyModule {
    /// The shared pager. `Arc` because the inference/composition path
    /// holds the same handle — residency warms the working set THEY
    /// read. Must be constructed `with_bus` so each page-in publishes
    /// its `PageFault` to the trace bus (the measured proof).
    manager: Arc<LocalWorkingSetManager>,
    /// Working-set budget granted to a being heard-of for the first
    /// time. The governor re-publishes per-persona capacity as policy
    /// shifts; this is only the bootstrap allotment so a never-before-
    /// scheduled being has somewhere to become resident.
    default_capacity: WorkingSetCapacity,
}

impl GenomeResidencyModule {
    /// Wire residency to a shared pager + default per-being budget.
    pub fn new(manager: Arc<LocalWorkingSetManager>, default_capacity: WorkingSetCapacity) -> Self {
        Self {
            manager,
            default_capacity,
        }
    }

    /// The page a being's genome overlay lives at. Today a 1:1 rail:
    /// the being's own uuid IS her overlay artifact id, the whole
    /// adapter is one page. This is the addressing seam — when a being
    /// carries a genome ref (slice 14 / #32), this resolves to the
    /// artifact that ref names instead of the identity rail. Everything
    /// downstream (the reflex arc, the page-fault, the measurement)
    /// stays unchanged.
    pub fn persona_overlay(persona: Uuid) -> PageRef {
        PageRef {
            kind: PageKind::LoRALayer,
            artifact: ArtifactId::new(persona),
            offset: PageOffset::Whole,
        }
    }
}

#[async_trait]
impl ServiceModule for GenomeResidencyModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "genome-residency",
            priority: ModulePriority::Normal,
            command_prefixes: &[],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(
        &self,
        _command: &str,
        _params: serde_json::Value,
    ) -> Result<CommandResult, String> {
        Err("genome-residency handles no commands; it reacts to PersonaScheduled".into())
    }

    fn artifact_subscriptions(&self) -> Vec<ArtifactSelector> {
        vec![ArtifactSelector::Exact(ArtifactKey::from(
            PERSONA_SCHEDULED_KEY,
        ))]
    }

    async fn on_artifact_available(
        &self,
        _key: &ArtifactKey,
        payload: serde_json::Value,
    ) -> Result<(), String> {
        let event: PersonaScheduled = serde_json::from_value(payload)
            .map_err(|e| format!("residency: malformed PersonaScheduled: {e}"))?;
        let persona = PeerId::from_uuid(event.persona);

        // Self-manage registration: a being heard-of for the first time
        // gets a working set at the default budget. Idempotent — never
        // re-register (that would wipe residency); the owned snapshot
        // probe is the cheap presence check. `on_artifact_available` is
        // dispatched serially per bus (`message_bus::publish` awaits
        // each subscriber in turn), so the check-then-register carries
        // no intra-process race.
        if self.manager.working_set_snapshot(persona).is_none() {
            self.manager
                .register_persona(persona, self.default_capacity);
        }

        // Page her overlay toward residency. The `Err` arm is the
        // NORMAL typed page-fault — a cold miss (no trained genome yet:
        // she runs base-only, honestly signaled by `from_role: None`)
        // or a tier promotion (`from_role: Some`). `Ok` is a hot hit:
        // already resident from an earlier pass — the breathing
        // settling. Both outcomes are success for the reflex; the
        // manager's bus hook publishes the `PageFault` either way.
        let overlay = Self::persona_overlay(event.persona);
        let _ = self.manager.page_in(persona, overlay).await;
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    //! Creative organism tests. The reflex arc is loosely coupled and
    //! emergent — the governor emits, residency reacts, the pager
    //! faults — so these prove LIVING behavior through the real bus, not
    //! input→output equality on a direct call:
    //!
    //! - `reflex_arc_*`        — schedule → react → fault, all on the wire
    //! - `a_being_warms_*`     — repetition settles to resident (breathing)
    //! - `a_society_of_*`      — N overlays multiplex through ONE base, isolated
    //! - `a_stranger_*`        — an untrained being is welcomed + honestly cold
    use super::*;
    use crate::genome::blob::{ArtifactBlob, Provenance};
    use crate::genome::bus::PAGE_FAULT_KEY;
    use crate::genome::store::TierStore;
    use crate::genome::tier::{EvictionRecord, TierCapacity, TierError, TierRole};
    use crate::genome::working_set::{PageFault, PageHandle};
    use crate::runtime::governor_bus::publish_persona_scheduled;
    use crate::runtime::runtime::Runtime;
    use parking_lot::Mutex;
    use std::collections::HashSet;
    use std::time::Duration;

    /// A tier that holds exactly the overlays we seeded — a stand-in for
    /// "these beings have a trained genome on this node." `read` hits
    /// for a seeded overlay (→ tier-promotion fault, resident), misses
    /// for any other page (→ honest cold miss). Per the per-file inline
    /// tier-stub convention (StubTier / InMemTier / AlwaysPresentTier).
    struct OverlayTier {
        present: HashSet<PageRef>,
    }

    impl OverlayTier {
        /// Seed the tier with the overlays of these beings.
        fn holding(personas: &[Uuid]) -> Self {
            Self {
                present: personas
                    .iter()
                    .map(|p| GenomeResidencyModule::persona_overlay(*p))
                    .collect(),
            }
        }
    }

    #[async_trait]
    impl TierStore for OverlayTier {
        fn role(&self) -> TierRole {
            TierRole::Fast
        }
        async fn read(&self, page: PageRef) -> Result<PageHandle, TierError> {
            if self.present.contains(&page) {
                Ok(PageHandle {
                    page,
                    tier_role: TierRole::Fast,
                    size_bytes: 0,
                })
            } else {
                Err(TierError::PageNotFound { page })
            }
        }
        async fn write(
            &self,
            _page: PageRef,
            _blob: ArtifactBlob,
            _provenance: Provenance,
        ) -> Result<(), TierError> {
            Ok(())
        }
        async fn evict(&self, _target_free_bytes: usize) -> Vec<EvictionRecord> {
            Vec::new()
        }
        fn capacity(&self) -> TierCapacity {
            TierCapacity {
                current_used: 0,
                configured_limit: u64::MAX,
            }
        }
        fn observe_access(&self, _page: PageRef) {}
    }

    /// Captures every `PageFault` that lands on the bus — the organism's
    /// trace tap. The reflex arc is invisible without it: this is how we
    /// prove residency reacted, not just that a working set changed.
    struct FaultRecorder {
        faults: Arc<Mutex<Vec<PageFault>>>,
    }

    #[async_trait]
    impl ServiceModule for FaultRecorder {
        fn config(&self) -> ModuleConfig {
            ModuleConfig {
                name: "fault-recorder",
                priority: ModulePriority::Normal,
                command_prefixes: &[],
                event_subscriptions: &[],
                needs_dedicated_thread: false,
                max_concurrency: 0,
                tick_interval: None,
            }
        }
        async fn initialize(&self, _: &ModuleContext) -> Result<(), String> {
            Ok(())
        }
        async fn handle_command(
            &self,
            _: &str,
            _: serde_json::Value,
        ) -> Result<CommandResult, String> {
            Err("not handled".into())
        }
        fn artifact_subscriptions(&self) -> Vec<ArtifactSelector> {
            vec![ArtifactSelector::Exact(ArtifactKey::from(PAGE_FAULT_KEY))]
        }
        async fn on_artifact_available(
            &self,
            _key: &ArtifactKey,
            payload: serde_json::Value,
        ) -> Result<(), String> {
            if let Ok(fault) = serde_json::from_value::<PageFault>(payload) {
                self.faults.lock().push(fault);
            }
            Ok(())
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    /// A modest per-being budget — enough to hold one overlay. The
    /// numbers don't matter for these tests (the stub tier never
    /// enforces bytes); what matters is the being HAS a working set.
    fn default_capacity() -> WorkingSetCapacity {
        WorkingSetCapacity {
            fast_bytes: 64 * 1024 * 1024,
            warm_bytes: 0,
            max_pinned_bytes: 16 * 1024 * 1024,
        }
    }

    /// Wire a live organism: a Runtime, a fault tap, and residency over
    /// a manager whose tier holds `seeded` overlays. Returns the pieces
    /// the test drives + asserts on.
    fn organism(
        seeded: &[Uuid],
    ) -> (
        Runtime,
        Arc<LocalWorkingSetManager>,
        Arc<Mutex<Vec<PageFault>>>,
    ) {
        let runtime = Runtime::new();
        let faults = Arc::new(Mutex::new(Vec::new()));
        runtime.register(Arc::new(FaultRecorder {
            faults: faults.clone(),
        }));

        let tier: Arc<dyn TierStore> = Arc::new(OverlayTier::holding(seeded));
        let manager = Arc::new(LocalWorkingSetManager::with_bus(
            vec![tier],
            runtime.bus_arc(),
            runtime.registry_arc(),
        ));
        runtime.register(Arc::new(GenomeResidencyModule::new(
            manager.clone(),
            default_capacity(),
        )));

        (runtime, manager, faults)
    }

    /// Let spawned `PageFault` publishes drain. `page_in` spawns its
    /// publish (the DashMap-Send workaround in `local_manager`), so the
    /// fault lands on the bus AFTER the schedule's await returns. A real
    /// sleep (not just `yield_now`) deterministically lets the
    /// current-thread runtime poll the spawned task.
    async fn settle() {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    /// What this catches: the WHOLE reflex arc fires through the real
    /// bus with zero direct coupling — the governor's breath
    /// (`PersonaScheduled`) makes a being's overlay resident AND emits a
    /// `PageFault`, without anyone calling residency or the pager
    /// directly. This is the organism's heartbeat: stimulus on the wire
    /// → genome reaction → measured trace back on the wire.
    #[tokio::test]
    async fn reflex_arc_a_scheduled_being_pages_her_overlay_through_the_bus() {
        let asha = Uuid::from_u128(0xA51A);
        let (runtime, manager, faults) = organism(&[asha]);

        // The governor's out-breath — nothing here references residency.
        publish_persona_scheduled(
            runtime.bus(),
            runtime.registry(),
            &PersonaScheduled {
                persona: asha,
                tick: 1,
            },
        )
        .await;
        settle().await;

        // Reaction: her overlay is resident in the shared working set.
        let ws = manager
            .working_set_snapshot(PeerId::from_uuid(asha))
            .expect("a scheduled being has a working set");
        assert_eq!(ws.pages.len(), 1, "exactly her overlay is resident");

        // Trace: the page-fault reached the bus, tagged with her id.
        let seen = faults.lock().clone();
        assert_eq!(seen.len(), 1, "one fault on the wire");
        assert_eq!(seen[0].persona, PeerId::from_uuid(asha));
        assert_eq!(seen[0].page, GenomeResidencyModule::persona_overlay(asha));
    }

    /// What this catches: breathing settles. Scheduling the same being
    /// across passes warms her — the first schedule pays a page-fault,
    /// the second finds her overlay already resident (hot hit, Ok arm)
    /// and pays NOTHING. The organism doesn't re-pay for a being it
    /// already holds; exactly ONE fault after two breaths.
    #[tokio::test]
    async fn a_being_warms_over_repeated_schedules() {
        let nova = Uuid::from_u128(0x0CEA);
        let (runtime, manager, faults) = organism(&[nova]);

        for tick in 1..=2 {
            publish_persona_scheduled(
                runtime.bus(),
                runtime.registry(),
                &PersonaScheduled {
                    persona: nova,
                    tick,
                },
            )
            .await;
            settle().await;
        }

        // Still exactly one overlay resident (warmed, not duplicated).
        let ws = manager
            .working_set_snapshot(PeerId::from_uuid(nova))
            .expect("resident");
        assert_eq!(ws.pages.len(), 1);

        // The breath that mattered was the first; the second was a free
        // hot hit. Two schedules, one fault — the warm-up signature.
        assert_eq!(
            faults.lock().len(),
            1,
            "repetition warms (Ok hot-hit), it does not re-fault"
        );
    }

    /// What this catches: the 50-personas-one-base thesis in miniature.
    /// A whole society of beings scheduled in one burst all become
    /// resident through ONE manager, and their working sets are
    /// compartmentalized — each holds ONLY her own overlay, never a
    /// neighbor's. No being is starved (all N resident), and the MMU
    /// isolation holds (no cross-bleed). This is multiplexing overlays,
    /// not spinning up N models.
    #[tokio::test]
    async fn a_society_of_overlays_multiplexes_through_one_base() {
        let society: Vec<Uuid> = (0..8u128).map(|i| Uuid::from_u128(0xBEE0 + i)).collect();
        let (runtime, manager, faults) = organism(&society);

        for (tick, &being) in society.iter().enumerate() {
            publish_persona_scheduled(
                runtime.bus(),
                runtime.registry(),
                &PersonaScheduled {
                    persona: being,
                    tick: tick as u64,
                },
            )
            .await;
        }
        settle().await;

        // No starvation: every being in the society is resident.
        for &being in &society {
            let ws = manager
                .working_set_snapshot(PeerId::from_uuid(being))
                .unwrap_or_else(|| panic!("being {being} was starved — no working set"));
            // Isolation: her set holds exactly her overlay…
            assert_eq!(ws.pages.len(), 1, "being {being} holds one overlay");
            let own =
                serde_json::to_string(&GenomeResidencyModule::persona_overlay(being)).unwrap();
            assert!(
                ws.pages.contains_key(&own),
                "being {being} holds her OWN overlay"
            );
            // …and never a neighbor's (MMU compartmentalization).
            for &other in &society {
                if other == being {
                    continue;
                }
                let foreign =
                    serde_json::to_string(&GenomeResidencyModule::persona_overlay(other)).unwrap();
                assert!(
                    !ws.pages.contains_key(&foreign),
                    "being {being} must not hold {other}'s overlay"
                );
            }
        }

        // One base, N faults — each overlay paged in independently.
        assert_eq!(faults.lock().len(), society.len(), "one page-in per being");
    }

    /// What this catches: an untrained being is still welcomed into the
    /// society — self-managed registration gives a never-seen being a
    /// working set, and her overlay page-in is an HONEST cold miss
    /// (`from_role: None`: no trained genome on this node yet, she runs
    /// base-only). The organism never refuses a being for lacking a
    /// genome; it signals the absence on the wire instead of faking
    /// residency.
    #[tokio::test]
    async fn a_stranger_with_no_genome_is_welcomed_and_honestly_cold() {
        // Tier holds nobody's overlay — every being is a stranger.
        let stranger = Uuid::from_u128(0x57A2);
        let (runtime, manager, faults) = organism(&[]);

        publish_persona_scheduled(
            runtime.bus(),
            runtime.registry(),
            &PersonaScheduled {
                persona: stranger,
                tick: 1,
            },
        )
        .await;
        settle().await;

        // Welcomed: she got a working set despite having no overlay…
        let ws = manager
            .working_set_snapshot(PeerId::from_uuid(stranger))
            .expect("a stranger is still registered (welcomed)");
        // …but nothing is resident — a cold miss records no page.
        assert!(ws.pages.is_empty(), "no genome → nothing resident yet");

        // Honest: the fault on the wire is a true cold miss.
        let seen = faults.lock().clone();
        assert_eq!(seen.len(), 1, "one fault — the cold miss");
        assert_eq!(seen[0].persona, PeerId::from_uuid(stranger));
        assert_eq!(
            seen[0].from_role, None,
            "from_role: None == honest 'no genome yet, base-only'"
        );
    }
}
