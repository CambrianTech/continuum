//! Artifact-key constants + bus publishing helpers for genome
//! events. PR-4 of working-set-manager.
//!
//! Background: PR-1 (#1346) shipped the typed `PageFault`,
//! `EvictionRecord`, and `AccessDenied` events. PR-2 (#1353) named
//! them on the trait surface. PR-3 (#1355) impl returns them through
//! its `Result` arms (PageFault) and direct method returns
//! (AccessDenied). What's been missing is the wire — what
//! ArtifactKey + payload shape downstream subscribers (audit-recorder
//! #1344, sentinel-observer, demand-aligned-recall) bind to.
//!
//! This module fills that gap with three building blocks:
//!
//! 1. **Canonical `ArtifactKey` constants** — every genome event has
//!    one stable key. Subscribers refer to the constant, not a string
//!    literal, so the wire stays consistent across renames.
//!
//! 2. **Publishing helpers** — `publish_page_fault`, etc. Each takes
//!    the bus + registry + the typed event, serializes the payload,
//!    and publishes through the artifact dispatch path I shipped in
//!    #1339 + #1343. Callers don't construct keys / serialize / route
//!    by hand.
//!
//! 3. **Subscriber convenience** — `subscribe_to_genome_events` wires
//!    a module to all three keys at once via `bus.subscribe_artifact`
//!    (the path #1343 added).
//!
//! ## What PR-4 does NOT do (PR-5)
//!
//! Wiring the helpers INTO `LocalWorkingSetManager` so its
//! `page_in`/`page_out`/`audit_access` auto-publish after each call.
//! That decorator/extension lands in PR-5. PR-4 ships the wire
//! definitions + helpers so that PR-5 only needs to plumb the bus +
//! registry references; the keys + payloads are already canonical.
//!
//! Why split: the wire shape is the coordination point with codex's
//! audit-recorder (#1344, subscribes to `AccessDenied`) + sentinel-
//! observer (subscribes to `PageFault`). Naming the keys + publishing
//! helpers in their own PR locks the contract first, lets downstream
//! subscribers wire to it BEFORE the LocalWorkingSetManager
//! integration (PR-5) plumbs them in.

use crate::runtime::artifact_handle::{ArtifactKey, ArtifactSelector};
use crate::runtime::message_bus::MessageBus;
use crate::runtime::registry::ModuleRegistry;

use super::tier::EvictionRecord;
use super::working_set::{AccessDenied, PageFault};

// ─── Canonical ArtifactKey constants ─────────────────────────────

/// ArtifactKey for `PageFault` events. Published every time the
/// working-set manager services a page-fault (true cold miss OR tier
/// promotion). Subscribers: sentinel-observer (learns persona access
/// patterns from these), demand-aligned-recall (caches ResidencyHint
/// based on which pages a persona keeps faulting on).
pub const PAGE_FAULT_KEY: &str = "genome/working_set.page_fault";

/// ArtifactKey for `EvictionRecord` events. Published every time a
/// tier evicts a page. Subscribers: sentinel-observer (recurring
/// evictions on the same page = signal to upgrade the page's tier
/// policy), audit-recorder (governor-driven evictions become a
/// `GovernorOverride` audit entry).
pub const EVICTION_RECORD_KEY: &str = "genome/working_set.eviction";

/// ArtifactKey for `AccessDenied` events from the MMU-style audit.
/// Published every time `audit_access` denies a cross-persona read.
/// Subscribers: audit-recorder (#1344, this is one of its four
/// canonical audit-entry inputs).
pub const ACCESS_DENIED_KEY: &str = "genome/working_set.access_denied";

// ─── Publishing helpers ─────────────────────────────────────────

/// Publish a `PageFault` to the trace bus under the canonical key.
/// Async — uses `MessageBus::publish` (the path that walks the
/// artifact-subscription list I shipped in #1343).
///
/// Serialization failures fall back to `Value::Null` rather than
/// panicking — the `PageFault` shape is serde-derived and known to
/// serialize cleanly, so a failure here would indicate substrate
/// corruption, not a user-visible bug. The trace bus still fires
/// (with empty payload) so subscribers see something happened.
pub async fn publish_page_fault(
    bus: &MessageBus,
    registry: &ModuleRegistry,
    fault: &PageFault,
) {
    let payload = serde_json::to_value(fault).unwrap_or(serde_json::Value::Null);
    bus.publish(PAGE_FAULT_KEY, payload, registry).await;
}

/// Publish an `EvictionRecord` to the trace bus under the canonical
/// key. Same async + serde semantics as `publish_page_fault`.
pub async fn publish_eviction_record(
    bus: &MessageBus,
    registry: &ModuleRegistry,
    record: &EvictionRecord,
) {
    let payload = serde_json::to_value(record).unwrap_or(serde_json::Value::Null);
    bus.publish(EVICTION_RECORD_KEY, payload, registry).await;
}

/// Publish an `AccessDenied` to the trace bus under the canonical
/// key. Async — `audit_access` is sync on the trait but PR-5's
/// integration will spawn the publish into a tokio task so the sync
/// caller doesn't block. Standalone callers (e.g. testing or
/// manually-publishing code) `.await` directly.
pub async fn publish_access_denied(
    bus: &MessageBus,
    registry: &ModuleRegistry,
    denied: &AccessDenied,
) {
    let payload = serde_json::to_value(denied).unwrap_or(serde_json::Value::Null);
    bus.publish(ACCESS_DENIED_KEY, payload, registry).await;
}

// ─── Subscriber convenience ─────────────────────────────────────

/// Wire a module to ALL three genome event types at once via the
/// artifact-subscription path (#1343). Convenience for modules that
/// want the full firehose — sentinel-observer, audit-recorder
/// extensions, performance harness observers.
///
/// Modules that only want one event type call `bus.subscribe_artifact`
/// directly with the specific key constant. This helper exists for
/// the common case + to anchor the per-module ServiceModule
/// `artifact_subscriptions()` return values:
///
/// ```ignore
/// fn artifact_subscriptions(&self) -> Vec<ArtifactSelector> {
///     all_genome_artifact_selectors()
/// }
/// ```
pub fn subscribe_to_genome_events(bus: &MessageBus, module_name: &'static str) {
    for selector in all_genome_artifact_selectors() {
        bus.subscribe_artifact(selector, module_name);
    }
}

/// Return the full set of genome `ArtifactSelector::Exact` entries.
/// Useful for `ServiceModule::artifact_subscriptions()` returns and
/// for unit tests that want to enumerate the canonical event surface
/// without duplicating the key list.
pub fn all_genome_artifact_selectors() -> Vec<ArtifactSelector> {
    vec![
        ArtifactSelector::Exact(ArtifactKey::from(PAGE_FAULT_KEY)),
        ArtifactSelector::Exact(ArtifactKey::from(EVICTION_RECORD_KEY)),
        ArtifactSelector::Exact(ArtifactKey::from(ACCESS_DENIED_KEY)),
    ]
}

#[cfg(test)]
mod tests {
    //! End-to-end tests: a recording ServiceModule subscribes via the
    //! convenience helper, the publishing helpers fire, the subscriber
    //! sees the right key + payload. This wires the whole #1339+#1343
    //! dispatch path end-to-end for genome events.
    use super::*;
    use crate::genome::tier::{EvictionPolicy, TierRole};
    use crate::genome::working_set::{
        ArtifactId, PageKind, PageOffset, PageRef, PersonaId,
    };
    use crate::runtime::runtime::Runtime;
    use crate::runtime::service_module::{
        CommandResult, ModuleConfig, ModulePriority, ServiceModule,
    };
    use async_trait::async_trait;
    use parking_lot::Mutex;
    use std::any::Any;
    use std::sync::Arc;
    use uuid::Uuid;

    /// Recording module: subscribes to all three genome keys, captures
    /// every (key, payload) pair. Tests assert which fired + the
    /// payload round-trips through serde.
    struct RecordingModule {
        name: &'static str,
        captured: Arc<Mutex<Vec<(String, serde_json::Value)>>>,
    }

    impl RecordingModule {
        fn new(name: &'static str) -> (Arc<Self>, Arc<Mutex<Vec<(String, serde_json::Value)>>>) {
            let captured = Arc::new(Mutex::new(Vec::new()));
            let module = Arc::new(Self {
                name,
                captured: captured.clone(),
            });
            (module, captured)
        }
    }

    #[async_trait]
    impl ServiceModule for RecordingModule {
        fn config(&self) -> ModuleConfig {
            ModuleConfig {
                name: self.name,
                priority: ModulePriority::Normal,
                command_prefixes: &[],
                event_subscriptions: &[],
                needs_dedicated_thread: false,
                max_concurrency: 0,
                tick_interval: None,
            }
        }
        async fn initialize(
            &self,
            _ctx: &crate::runtime::ModuleContext,
        ) -> Result<(), String> {
            Ok(())
        }
        async fn handle_command(
            &self,
            _: &str,
            _: serde_json::Value,
        ) -> Result<CommandResult, String> {
            Err("not handled".to_string())
        }
        fn artifact_subscriptions(&self) -> Vec<ArtifactSelector> {
            all_genome_artifact_selectors()
        }
        async fn on_artifact_available(
            &self,
            key: &ArtifactKey,
            payload: serde_json::Value,
        ) -> Result<(), String> {
            self.captured.lock().push((key.as_str().to_string(), payload));
            Ok(())
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    fn sample_persona(low_bits: u128) -> PersonaId {
        PersonaId::new(Uuid::from_u128(low_bits))
    }

    fn sample_page() -> PageRef {
        PageRef {
            kind: PageKind::LoRALayer,
            artifact: ArtifactId::new(Uuid::nil()),
            offset: PageOffset::Whole,
        }
    }

    /// What this catches: the three artifact-key constants don't
    /// silently drift. Subscribers in other modules (audit-recorder,
    /// sentinel-observer) refer to these constants; if a future PR
    /// renames a string, this test pins the canonical wire value so
    /// the rename is deliberate.
    #[test]
    fn artifact_keys_have_canonical_string_values() {
        assert_eq!(PAGE_FAULT_KEY, "genome/working_set.page_fault");
        assert_eq!(EVICTION_RECORD_KEY, "genome/working_set.eviction");
        assert_eq!(ACCESS_DENIED_KEY, "genome/working_set.access_denied");
    }

    /// What this catches: `all_genome_artifact_selectors` returns
    /// every key as `ArtifactSelector::Exact` — never `Prefix` (which
    /// has different match semantics) and never missing a key. If a
    /// future PR adds a fourth event type, this test should fail (to
    /// force the author to add it here + verify the wire contract).
    #[test]
    fn all_genome_selectors_cover_every_key_as_exact() {
        let selectors = all_genome_artifact_selectors();
        assert_eq!(selectors.len(), 3);

        let exact_keys: Vec<String> = selectors
            .iter()
            .filter_map(|s| match s {
                ArtifactSelector::Exact(k) => Some(k.as_str().to_string()),
                ArtifactSelector::Prefix(_) => None,
            })
            .collect();
        assert_eq!(exact_keys.len(), 3, "all entries must be Exact");
        assert!(exact_keys.contains(&PAGE_FAULT_KEY.to_string()));
        assert!(exact_keys.contains(&EVICTION_RECORD_KEY.to_string()));
        assert!(exact_keys.contains(&ACCESS_DENIED_KEY.to_string()));
    }

    /// What this catches: `publish_page_fault` lands on the
    /// PAGE_FAULT_KEY artifact key with the serialized PageFault
    /// payload. End-to-end test for the #1339+#1343 dispatch path
    /// applied to genome events.
    #[tokio::test]
    async fn publish_page_fault_routes_to_subscribed_module() {
        let runtime = Runtime::new();
        let (module, captured) = RecordingModule::new("recorder-fault");
        runtime.register(module);

        let fault = PageFault {
            page: sample_page(),
            from_role: Some(TierRole::Cold),
            to_role: TierRole::Fast,
            persona: sample_persona(1),
            elapsed_us: 123,
            eviction_cost: None,
        };
        publish_page_fault(runtime.bus(), runtime.registry(), &fault).await;

        let events = captured.lock().clone();
        let fault_events: Vec<_> = events
            .iter()
            .filter(|(k, _)| k == PAGE_FAULT_KEY)
            .collect();
        assert_eq!(fault_events.len(), 1);
        let (_, payload) = fault_events[0];
        // Payload round-trips back into PageFault — the serde shape
        // is wire-stable for the subscriber.
        let back: PageFault = serde_json::from_value(payload.clone()).unwrap();
        assert_eq!(back, fault);
    }

    /// What this catches: `publish_eviction_record` lands on the
    /// EVICTION_RECORD_KEY. Different key from page_fault — a
    /// subscriber that only subscribed to PAGE_FAULT_KEY doesn't see
    /// eviction events.
    #[tokio::test]
    async fn publish_eviction_record_routes_to_correct_key() {
        let runtime = Runtime::new();
        let (module, captured) = RecordingModule::new("recorder-evict");
        runtime.register(module);

        let record = EvictionRecord {
            page: sample_page(),
            from_role: TierRole::Fast,
            to_role: Some(TierRole::Bench),
            policy_fired: EvictionPolicy::LruWithinTurn,
            elapsed_us: 42,
        };
        publish_eviction_record(runtime.bus(), runtime.registry(), &record).await;

        let events = captured.lock().clone();
        let evict_events: Vec<_> = events
            .iter()
            .filter(|(k, _)| k == EVICTION_RECORD_KEY)
            .collect();
        assert_eq!(evict_events.len(), 1);
        let back: EvictionRecord =
            serde_json::from_value(evict_events[0].1.clone()).unwrap();
        assert_eq!(back, record);
    }

    /// What this catches: `publish_access_denied` lands on the
    /// ACCESS_DENIED_KEY. This is the audit-recorder (#1344)
    /// integration point — audit-recorder subscribes to this key as
    /// one of its four canonical audit inputs.
    #[tokio::test]
    async fn publish_access_denied_routes_to_audit_input_key() {
        let runtime = Runtime::new();
        let (module, captured) = RecordingModule::new("recorder-denied");
        runtime.register(module);

        let denied = AccessDenied {
            actor: sample_persona(1),
            page: sample_page(),
            owner: Some(sample_persona(2)),
            reason: "cross-persona read blocked".to_string(),
        };
        publish_access_denied(runtime.bus(), runtime.registry(), &denied).await;

        let events = captured.lock().clone();
        let denied_events: Vec<_> = events
            .iter()
            .filter(|(k, _)| k == ACCESS_DENIED_KEY)
            .collect();
        assert_eq!(denied_events.len(), 1);
        let back: AccessDenied =
            serde_json::from_value(denied_events[0].1.clone()).unwrap();
        assert_eq!(back, denied);
    }

    /// What this catches: a module subscribing via the convenience
    /// helper sees all THREE events when each fires. The helper IS
    /// the bridge between the canonical key set + the
    /// `bus.subscribe_artifact` API I shipped in #1343.
    #[tokio::test]
    async fn convenience_helper_subscribes_to_all_three_event_types() {
        let runtime = Runtime::new();
        let (module, captured) = RecordingModule::new("recorder-all");
        runtime.register(module);

        // Fire all three event types.
        let fault = PageFault {
            page: sample_page(),
            from_role: None,
            to_role: TierRole::Fast,
            persona: sample_persona(1),
            elapsed_us: 0,
            eviction_cost: None,
        };
        let evict = EvictionRecord {
            page: sample_page(),
            from_role: TierRole::Fast,
            to_role: None,
            policy_fired: EvictionPolicy::AppendOnlyGcOnSleep,
            elapsed_us: 0,
        };
        let denied = AccessDenied {
            actor: sample_persona(1),
            page: sample_page(),
            owner: None,
            reason: "test".into(),
        };
        publish_page_fault(runtime.bus(), runtime.registry(), &fault).await;
        publish_eviction_record(runtime.bus(), runtime.registry(), &evict).await;
        publish_access_denied(runtime.bus(), runtime.registry(), &denied).await;

        let events = captured.lock().clone();
        let keys: Vec<String> = events.iter().map(|(k, _)| k.clone()).collect();
        assert!(keys.contains(&PAGE_FAULT_KEY.to_string()));
        assert!(keys.contains(&EVICTION_RECORD_KEY.to_string()));
        assert!(keys.contains(&ACCESS_DENIED_KEY.to_string()));
        assert_eq!(events.len(), 3, "exactly one of each event delivered");
    }

    /// What this catches: a module subscribing ONLY to PAGE_FAULT_KEY
    /// (via direct `bus.subscribe_artifact` call, not the convenience
    /// helper) sees PageFault events but NOT EvictionRecord. This
    /// proves the keys are independent — sentinel-observer that wants
    /// only page-faults isn't forced to filter every event.
    #[tokio::test]
    async fn selective_subscriber_only_sees_its_subscribed_key() {
        let runtime = Runtime::new();

        // Module subscribes only to PAGE_FAULT_KEY.
        struct PageFaultOnly {
            captured: Arc<Mutex<Vec<String>>>,
        }
        #[async_trait]
        impl ServiceModule for PageFaultOnly {
            fn config(&self) -> ModuleConfig {
                ModuleConfig {
                    name: "page-fault-only",
                    priority: ModulePriority::Normal,
                    command_prefixes: &[],
                    event_subscriptions: &[],
                    needs_dedicated_thread: false,
                    max_concurrency: 0,
                    tick_interval: None,
                }
            }
            async fn initialize(
                &self,
                _: &crate::runtime::ModuleContext,
            ) -> Result<(), String> {
                Ok(())
            }
            async fn handle_command(
                &self,
                _: &str,
                _: serde_json::Value,
            ) -> Result<CommandResult, String> {
                Err("not handled".to_string())
            }
            fn artifact_subscriptions(&self) -> Vec<ArtifactSelector> {
                vec![ArtifactSelector::Exact(ArtifactKey::from(PAGE_FAULT_KEY))]
            }
            async fn on_artifact_available(
                &self,
                key: &ArtifactKey,
                _: serde_json::Value,
            ) -> Result<(), String> {
                self.captured.lock().push(key.as_str().to_string());
                Ok(())
            }
            fn as_any(&self) -> &dyn Any {
                self
            }
        }

        let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let module = Arc::new(PageFaultOnly {
            captured: captured.clone(),
        });
        runtime.register(module);

        let fault = PageFault {
            page: sample_page(),
            from_role: None,
            to_role: TierRole::Fast,
            persona: sample_persona(1),
            elapsed_us: 0,
            eviction_cost: None,
        };
        let evict = EvictionRecord {
            page: sample_page(),
            from_role: TierRole::Fast,
            to_role: None,
            policy_fired: EvictionPolicy::AppendOnlyGcOnSleep,
            elapsed_us: 0,
        };
        publish_page_fault(runtime.bus(), runtime.registry(), &fault).await;
        publish_eviction_record(runtime.bus(), runtime.registry(), &evict).await;

        let events = captured.lock().clone();
        assert_eq!(events.len(), 1, "only one event delivered to selective subscriber");
        assert_eq!(events[0], PAGE_FAULT_KEY);
    }
}
