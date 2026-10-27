//! The governor's out-breath: the per-pass "a being received a
//! cognitive slice" event.
//!
//! Background: `genome/bus.rs` gave the working-set pager an
//! out-breath — every `page_in` that does work publishes a typed
//! `PageFault` under a canonical key, and any module can subscribe
//! without the pager knowing it exists. The governor (the scheduler of
//! the society of minds) had no such breath: it published only a
//! `watch::Sender<GovernorSnapshot>` — a single-current-value channel,
//! not a pub/sub event. Nothing could REACT to "this being just got
//! scheduled."
//!
//! That asymmetry is the seam this module closes. The governor now
//! EMITS one `PersonaScheduled` per being it granted a slice each pass.
//! It does NOT call the genome pager (or anything else). Residency,
//! sentinel-observers, demand-aligned-recall — each subscribes and
//! reacts on its own. Adding a reactor needs zero governor edits: the
//! nervous system is the bus, not a method table.
//!
//! This is the wire that makes "a persona is a genome overlay
//! multiplexed through ONE base model" a living loop: the governor
//! schedules N beings → emits N `PersonaScheduled` → the residency
//! reactor (`genome::GenomeResidencyModule`) pages N overlays through
//! one manager → each emits its own `PageFault`. Schedule → emit →
//! page → fault, all observable on the bus, with zero direct coupling
//! between the scheduler and the pager.
//!
//! Mirror of `genome/bus.rs` (canonical key + typed payload + publish
//! helper), the other direction — emission FROM the governor rather
//! than emission FROM the pager.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use super::message_bus::MessageBus;
use super::registry::ModuleRegistry;

/// ArtifactKey for `PersonaScheduled` events. Published once per being
/// the governor granted a cognitive slice in a scheduling pass.
/// Subscribers: `genome::GenomeResidencyModule` (pages that being's
/// genome overlay resident), sentinel-observers (learn which beings
/// are active when), demand-aligned-recall (pre-warm a scheduled
/// being's frequently-faulted pages). Refer to this constant, never
/// the literal — renames stay deliberate (see the pin test).
pub const PERSONA_SCHEDULED_KEY: &str = "governor/persona.scheduled";

/// A being received a cognitive slice this pass. The governor emits one
/// per scheduled being (deduped — a being admitted for several regions
/// in one pass still breathes once). Carries only identity + tick: the
/// minimal "who is alive right now" signal. Richer per-pass detail
/// (orientation mix, region breakdown) lives in `GovernorSnapshot`;
/// this event is the lightweight fan-out that reactors bind to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/runtime/PersonaScheduled.ts"
)]
pub struct PersonaScheduled {
    /// The being that got a slice.
    #[ts(type = "string")]
    pub persona: Uuid,
    /// The governor tick that scheduled her — lets a subscriber dedup
    /// across the (region × persona) fan-in of a single pass and order
    /// observations.
    #[ts(type = "number")]
    pub tick: u64,
}

/// Publish a `PersonaScheduled` to the trace bus under the canonical
/// key. Async — uses `MessageBus::publish` (the artifact-subscription
/// dispatch path). Serialization failures fall back to `Value::Null`
/// rather than panicking (the payload is serde-derived and known to
/// serialize cleanly; a failure would mean substrate corruption, not a
/// user bug) so subscribers still see that a pass happened.
pub async fn publish_persona_scheduled(
    bus: &MessageBus,
    registry: &ModuleRegistry,
    event: &PersonaScheduled,
) {
    let payload = serde_json::to_value(event).unwrap_or(serde_json::Value::Null);
    bus.publish(PERSONA_SCHEDULED_KEY, payload, registry).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::artifact_handle::{ArtifactKey, ArtifactSelector};
    use crate::runtime::runtime::Runtime;
    use crate::runtime::service_module::{
        CommandResult, ModuleConfig, ModulePriority, ServiceModule,
    };
    use async_trait::async_trait;
    use parking_lot::Mutex;
    use std::any::Any;
    use std::sync::Arc;

    /// Recording module: subscribes to PERSONA_SCHEDULED_KEY, captures
    /// every payload so tests can assert the governor's breath landed
    /// with the right identity.
    struct RecordingModule {
        captured: Arc<Mutex<Vec<serde_json::Value>>>,
    }

    #[async_trait]
    impl ServiceModule for RecordingModule {
        fn config(&self) -> ModuleConfig {
            ModuleConfig {
                name: "recorder-scheduled",
                priority: ModulePriority::Normal,
                command_prefixes: &[],
                event_subscriptions: &[],
                needs_dedicated_thread: false,
                max_concurrency: 0,
                tick_interval: None,
            }
        }
        async fn initialize(&self, _: &crate::runtime::ModuleContext) -> Result<(), String> {
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
            vec![ArtifactSelector::Exact(ArtifactKey::from(
                PERSONA_SCHEDULED_KEY,
            ))]
        }
        async fn on_artifact_available(
            &self,
            _key: &ArtifactKey,
            payload: serde_json::Value,
        ) -> Result<(), String> {
            self.captured.lock().push(payload);
            Ok(())
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    /// What this catches: the canonical wire key doesn't silently
    /// drift. Subscribers in other modules (residency, sentinel) bind
    /// to this constant; a rename must be deliberate.
    #[test]
    fn persona_scheduled_key_is_canonical() {
        assert_eq!(PERSONA_SCHEDULED_KEY, "governor/persona.scheduled");
    }

    /// What this catches: the governor's out-breath reaches a
    /// subscriber end-to-end through the real bus + registry, with the
    /// identity + tick intact (round-trips through serde). This is the
    /// emission half of the reflex arc the residency module completes.
    #[tokio::test]
    async fn publish_persona_scheduled_routes_to_subscriber() {
        let runtime = Runtime::new();
        let captured = Arc::new(Mutex::new(Vec::new()));
        runtime.register(Arc::new(RecordingModule {
            captured: captured.clone(),
        }));

        let event = PersonaScheduled {
            persona: Uuid::from_u128(0xA51A),
            tick: 7,
        };
        publish_persona_scheduled(runtime.bus(), runtime.registry(), &event).await;

        let seen = captured.lock().clone();
        assert_eq!(seen.len(), 1, "exactly one breath delivered");
        let back: PersonaScheduled = serde_json::from_value(seen[0].clone()).unwrap();
        assert_eq!(back, event, "identity + tick survive the wire");
    }
}
