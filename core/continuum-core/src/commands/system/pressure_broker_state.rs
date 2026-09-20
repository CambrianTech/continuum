//! `system/pressure-broker-state` — a snapshot of the cross-pool [`PressureBroker`].
//!
//! Dep-holding: captures the [`PressureBrokerModule`](crate::modules::pressure_broker_module)'s
//! live `Arc<PressureBroker>`. The module builds the runtime object in its `commands()`
//! over that broker; the descriptor self-publishes to the registry.
//!
//! Migrated off the module's legacy `handle_command` arm (#62). Returns the same typed
//! [`BrokerSnapshot`](crate::paging::BrokerSnapshot) the legacy handler did — its
//! camelCase serde + ts-rs export (`protocol/typescript/paging/BrokerSnapshot.ts`) is
//! the wire contract the TS mixin and `uu`/status row consume, preserved byte-identical.
//!
//! ## Gating
//!
//! `AiSafe` — a read-only inspection. One probe per call: pressure reads are atomic
//! loads plus a max over the registered pool list; NO eviction is fired (that is the
//! tick's job). No state mutation, no compute spend, no credentials.

use std::sync::Arc;

use crate::paging::{BrokerSnapshot, PressureBroker};

use super::SystemQuery;

crate::action_command! {
    /// Snapshot the cross-pool memory-pressure broker: global pressure + tier, the
    /// per-pool view, and lifetime eviction counters. A pure read — observes pressure
    /// without firing any eviction.
    pub struct SystemPressureBrokerState { broker: Arc<PressureBroker> }
    name: "system/pressure-broker-state",
    access: AiSafe,
    params: SystemQuery,
    output: BrokerSnapshot,
    run(this, _ctx, _p) => {
        Ok(this.broker.snapshot())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paging::BrokerConfig;
    use crate::sdk_codegen::{AccessLevel, ActionCommand, Ctx};

    fn command() -> SystemPressureBrokerState {
        SystemPressureBrokerState {
            broker: Arc::new(PressureBroker::new(BrokerConfig::default())),
        }
    }

    // what this catches: name/access wiring — the broker snapshot is a read-only probe,
    // so it lives on the AiSafe surface (a persona may legitimately inspect pressure).
    #[test]
    fn name_and_access_wired() {
        assert_eq!(
            SystemPressureBrokerState::NAME,
            "system/pressure-broker-state"
        );
        assert!(matches!(
            SystemPressureBrokerState::ACCESS,
            AccessLevel::AiSafe
        ));
    }

    // what this catches: the migrated handler returns a BrokerSnapshot whose serialized
    // form keeps the camelCase keys ts-rs emitted — a drift here would feed the TS mixin
    // / status row stringly-typed garbage. This is the same wire-contract assertion the
    // legacy module test made, preserved through the migration.
    #[tokio::test]
    async fn returns_typed_snapshot_with_camelcase_keys() {
        let out = command()
            .run(&Ctx::default(), SystemQuery {})
            .await
            .expect("snapshot never errors");
        let json = serde_json::to_value(&out).unwrap();
        assert!(json["globalPressure"].is_number(), "globalPressure missing");
        assert!(json["globalTier"].is_string(), "globalTier missing");
        assert!(json["pools"].is_array(), "pools missing");
        assert!(json["evictionsFired"].is_number(), "evictionsFired missing");
        assert!(
            json["bytesFreedTotal"].is_number(),
            "bytesFreedTotal missing"
        );
        // globalTier pins the PressureTier enum's lowercase wire form.
        let tier = json["globalTier"].as_str().unwrap();
        assert!(
            matches!(tier, "normal" | "warning" | "high" | "critical"),
            "globalTier must be normal|warning|high|critical; got: {tier}"
        );
    }
}
