//! `gpu/register-consumer` + `gpu/unregister-consumer` — a compute consumer
//! announcing (or retracting) its GPU footprint to the authority.
//!
//! These are `Internal`: substrate plumbing invoked in-process by a consumer such
//! as a training run, not a persona/remote tool. VRAM is ONE per-machine authority
//! (continuum task #56) — consumers LEASE by registering here so the authority's
//! pressure and eviction accounting stay accurate.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::gpu::{make_entry, GpuMemoryManager, GpuPriority, GpuSubsystem};

/// Inputs to `gpu/register-consumer`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/gpu/GpuRegisterConsumerParams.ts"
)]
pub struct GpuRegisterConsumerParams {
    /// Stable consumer id, e.g. `training:asha:coding`.
    pub id: String,
    /// Human-readable label, e.g. `Training: asha / coding (Llama-3.2-3B)`.
    pub label: String,
    /// Eviction priority: `realtime`, `interactive`, `background`, or `batch`
    /// (default `batch` — most evictable).
    #[serde(default)]
    #[ts(optional)]
    pub priority: Option<String>,
    /// Footprint in bytes to account against the inference budget (default 0).
    #[serde(default)]
    #[ts(optional)]
    #[ts(type = "number")]
    pub bytes: Option<u64>,
}

/// Result of `gpu/register-consumer`.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/gpu/GpuRegisterConsumerResult.ts"
)]
pub struct GpuRegisterConsumerResult {
    /// Always true on success (the call fails loud otherwise).
    pub registered: bool,
    /// Echo of the registered id.
    pub id: String,
    /// Footprint accounted, in bytes.
    #[ts(type = "number")]
    pub bytes: u64,
    /// GPU pressure after accounting the new consumer, 0.0–1.0.
    #[ts(type = "number")]
    pub pressure: f32,
}

/// Inputs to `gpu/unregister-consumer`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/gpu/GpuUnregisterConsumerParams.ts"
)]
pub struct GpuUnregisterConsumerParams {
    /// The consumer id to retract (must match the registered id).
    pub id: String,
    /// Bytes to release from the inference budget (default 0).
    #[serde(default)]
    #[ts(optional)]
    #[ts(type = "number")]
    pub bytes: Option<u64>,
}

/// Result of `gpu/unregister-consumer`.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/gpu/GpuUnregisterConsumerResult.ts"
)]
pub struct GpuUnregisterConsumerResult {
    /// Always true on success.
    pub unregistered: bool,
    /// Echo of the retracted id.
    pub id: String,
    /// GPU pressure after releasing the consumer, 0.0–1.0.
    #[ts(type = "number")]
    pub pressure: f32,
}

fn parse_priority(p: Option<&str>) -> GpuPriority {
    match p.unwrap_or("batch") {
        "realtime" => GpuPriority::Realtime,
        "interactive" => GpuPriority::Interactive,
        "background" => GpuPriority::Background,
        _ => GpuPriority::Batch,
    }
}

crate::action_command! {
    /// Register a GPU consumer (e.g. a training run) with the memory authority so its
    /// footprint counts toward pressure and it becomes an eviction candidate.
    /// Substrate plumbing — invoked in-process by the consumer, not a persona tool.
    pub struct GpuRegisterConsumer { manager: Arc<GpuMemoryManager> }
    name: "gpu/register-consumer",
    access: Internal,
    params: GpuRegisterConsumerParams,
    output: GpuRegisterConsumerResult,
    run(this, _ctx, p) => {
        let bytes = p.bytes.unwrap_or(0);
        let priority = parse_priority(p.priority.as_deref());
        this.manager
            .eviction_registry
            .register(make_entry(&p.id, &p.label, priority, bytes));
        // Account the footprint against the inference subsystem budget.
        this.manager.account_external(GpuSubsystem::Inference, bytes);
        Ok(GpuRegisterConsumerResult {
            registered: true,
            id: p.id,
            bytes,
            pressure: this.manager.pressure(),
        })
    }
}

crate::action_command! {
    /// Unregister a GPU consumer and release its bytes from the inference budget.
    /// Substrate plumbing — the retract half of `gpu/register-consumer`.
    pub struct GpuUnregisterConsumer { manager: Arc<GpuMemoryManager> }
    name: "gpu/unregister-consumer",
    access: Internal,
    params: GpuUnregisterConsumerParams,
    output: GpuUnregisterConsumerResult,
    run(this, _ctx, p) => {
        let bytes = p.bytes.unwrap_or(0);
        this.manager.eviction_registry.unregister(&p.id);
        this.manager.release(GpuSubsystem::Inference, bytes);
        Ok(GpuUnregisterConsumerResult {
            unregistered: true,
            id: p.id,
            pressure: this.manager.pressure(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: name/access wiring — consumer lifecycle is Internal plumbing,
    // never exposed on the AiSafe/Privileged caller surfaces.
    #[test]
    fn names_and_access_wired() {
        assert_eq!(GpuRegisterConsumer::NAME, "gpu/register-consumer");
        assert_eq!(GpuUnregisterConsumer::NAME, "gpu/unregister-consumer");
        assert!(matches!(
            GpuRegisterConsumer::ACCESS,
            crate::sdk_codegen::AccessLevel::Internal
        ));
        assert!(matches!(
            GpuUnregisterConsumer::ACCESS,
            crate::sdk_codegen::AccessLevel::Internal
        ));
    }

    // what this catches: register → unregister is a clean round-trip — the consumer
    // appears in the registry after register and is gone after unregister, so the
    // authority's accounting doesn't leak a phantom consumer.
    #[tokio::test]
    async fn register_then_unregister_round_trips() {
        let manager = Arc::new(GpuMemoryManager::simulated("Apple M5 Pro", 53_000_000_000));
        let reg = GpuRegisterConsumer {
            manager: manager.clone(),
        };
        let unreg = GpuUnregisterConsumer {
            manager: manager.clone(),
        };

        let r = reg
            .run(
                &Ctx::default(),
                GpuRegisterConsumerParams {
                    id: "training:test:coding".into(),
                    label: "Training: test / coding".into(),
                    priority: Some("batch".into()),
                    bytes: Some(6_000_000_000),
                },
            )
            .await
            .unwrap();
        assert!(r.registered);
        assert_eq!(manager.eviction_registry.snapshot().entries.len(), 1);

        let u = unreg
            .run(
                &Ctx::default(),
                GpuUnregisterConsumerParams {
                    id: "training:test:coding".into(),
                    bytes: Some(6_000_000_000),
                },
            )
            .await
            .unwrap();
        assert!(u.unregistered);
        assert_eq!(manager.eviction_registry.snapshot().entries.len(), 0);
    }
}
