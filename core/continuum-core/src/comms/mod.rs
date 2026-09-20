//! Shared Rust communication contracts.
//!
//! This module is intentionally transport-neutral. IPC, AIRC, grid routing,
//! live media, and future GPU-frame paths can wrap their existing payloads in
//! the same envelope and budget model before adapter-specific rewrites begin.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Arc;
use ts_rs::TS;
use uuid::Uuid;

use crate::identity::PeerId;

/// A message's identity. A UUID, never a string: an id the substrate MINTS has no
/// business being free text, and `MessageId::new("msg-1")` let any caller invent a
/// namespace that collides with every other caller's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/comms/MessageId.ts")]
#[serde(transparent)]
pub struct MessageId(#[ts(type = "string")] pub Uuid);

impl MessageId {
    /// Mint a fresh message identity. There is no caller-supplied form — the
    /// substrate owns this id.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl fmt::Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Ties a reply back to the exchange that provoked it. Distinct TYPE from
/// [`MessageId`] even though the root of an exchange carries the same UUID — the
/// compiler, not a naming convention, is what stops one being passed as the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/comms/CorrelationId.ts"
)]
#[serde(transparent)]
pub struct CorrelationId(#[ts(type = "string")] pub Uuid);

impl CorrelationId {
    /// The correlation an exchange ROOTED at `id` carries.
    pub fn of_exchange(id: MessageId) -> Self {
        Self(id.0)
    }
}

impl fmt::Display for CorrelationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/comms/Causality.ts")]
pub struct Causality {
    pub parent_id: Option<MessageId>,
    pub sequence: u64,
    pub replay_nonce: Option<String>,
}

impl Causality {
    pub fn root(sequence: u64) -> Self {
        Self {
            parent_id: None,
            sequence,
            replay_nonce: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/comms/PayloadClass.ts"
)]
pub enum PayloadClass {
    Control,
    Command,
    Event,
    Transcript,
    ArtifactManifest,
    AudioFrame,
    VideoFrame,
    GpuFrameHandle,
}

impl PayloadClass {
    pub fn is_bulk(self) -> bool {
        matches!(
            self,
            Self::AudioFrame | Self::VideoFrame | Self::GpuFrameHandle
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/comms/RetentionPolicy.ts"
)]
pub enum RetentionPolicy {
    Ephemeral,
    Transcript,
    Audit,
    Durable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/comms/CommsCopyBudget.ts"
)]
pub struct CommsCopyBudget {
    pub max_cpu_copies: u32,
    pub max_gpu_copies: u32,
}

impl CommsCopyBudget {
    pub const fn zero_cpu() -> Self {
        Self {
            max_cpu_copies: 0,
            max_gpu_copies: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/comms/CommsMemoryBudget.ts"
)]
pub struct CommsMemoryBudget {
    pub max_heap_bytes: u64,
    pub max_external_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/comms/CommsGpuBudget.ts"
)]
pub struct CommsGpuBudget {
    pub requires_gpu_residency: bool,
    pub max_gpu_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/comms/CommsRetryBudget.ts"
)]
pub struct CommsRetryBudget {
    pub max_attempts: u32,
    pub retry_window_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/comms/ResourceBudget.ts"
)]
pub struct ResourceBudget {
    pub max_bytes: u64,
    pub deadline_ms: u64,
    pub max_queue_depth: u32,
    pub cpu_copy_budget: CommsCopyBudget,
    pub memory_budget: CommsMemoryBudget,
    pub gpu_budget: CommsGpuBudget,
    pub retry_budget: CommsRetryBudget,
    pub retention: RetentionPolicy,
}

impl ResourceBudget {
    pub fn control(deadline_ms: u64) -> Self {
        Self {
            max_bytes: 64 * 1024,
            deadline_ms,
            max_queue_depth: 128,
            cpu_copy_budget: CommsCopyBudget {
                max_cpu_copies: 1,
                max_gpu_copies: 0,
            },
            memory_budget: CommsMemoryBudget {
                max_heap_bytes: 64 * 1024,
                max_external_bytes: 0,
            },
            gpu_budget: CommsGpuBudget {
                requires_gpu_residency: false,
                max_gpu_bytes: 0,
            },
            retry_budget: CommsRetryBudget {
                max_attempts: 1,
                retry_window_ms: deadline_ms,
            },
            retention: RetentionPolicy::Ephemeral,
        }
    }

    pub fn zero_copy_media(deadline_ms: u64, max_gpu_bytes: u64) -> Self {
        Self {
            max_bytes: 512,
            deadline_ms,
            max_queue_depth: 3,
            cpu_copy_budget: CommsCopyBudget::zero_cpu(),
            memory_budget: CommsMemoryBudget {
                max_heap_bytes: 512,
                max_external_bytes: 0,
            },
            gpu_budget: CommsGpuBudget {
                requires_gpu_residency: true,
                max_gpu_bytes,
            },
            retry_budget: CommsRetryBudget {
                max_attempts: 0,
                retry_window_ms: 0,
            },
            retention: RetentionPolicy::Ephemeral,
        }
    }

    pub fn validate(&self, cost: &ResourceCost) -> Result<(), BudgetViolation> {
        if cost.bytes > self.max_bytes {
            return Err(BudgetViolation::Bytes {
                actual: cost.bytes,
                limit: self.max_bytes,
            });
        }
        if cost.heap_bytes > self.memory_budget.max_heap_bytes {
            return Err(BudgetViolation::HeapBytes {
                actual: cost.heap_bytes,
                limit: self.memory_budget.max_heap_bytes,
            });
        }
        if cost.external_bytes > self.memory_budget.max_external_bytes {
            return Err(BudgetViolation::ExternalBytes {
                actual: cost.external_bytes,
                limit: self.memory_budget.max_external_bytes,
            });
        }
        if cost.gpu_bytes > self.gpu_budget.max_gpu_bytes {
            return Err(BudgetViolation::GpuBytes {
                actual: cost.gpu_bytes,
                limit: self.gpu_budget.max_gpu_bytes,
            });
        }
        if cost.cpu_copies > self.cpu_copy_budget.max_cpu_copies {
            return Err(BudgetViolation::CpuCopies {
                actual: cost.cpu_copies,
                limit: self.cpu_copy_budget.max_cpu_copies,
            });
        }
        if cost.gpu_copies > self.cpu_copy_budget.max_gpu_copies {
            return Err(BudgetViolation::GpuCopies {
                actual: cost.gpu_copies,
                limit: self.cpu_copy_budget.max_gpu_copies,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/comms/IntegrityHint.ts"
)]
pub struct IntegrityHint {
    pub content_sha256: Option<String>,
    pub merkle_parent: Option<String>,
}

impl IntegrityHint {
    pub fn unchecked() -> Self {
        Self {
            content_sha256: None,
            merkle_parent: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/comms/ResourceCost.ts"
)]
pub struct ResourceCost {
    pub bytes: u64,
    pub heap_bytes: u64,
    pub external_bytes: u64,
    pub gpu_bytes: u64,
    pub cpu_copies: u32,
    pub gpu_copies: u32,
}

impl ResourceCost {
    pub fn control_bytes(bytes: u64) -> Self {
        Self {
            bytes,
            heap_bytes: bytes,
            external_bytes: 0,
            gpu_bytes: 0,
            cpu_copies: 1,
            gpu_copies: 0,
        }
    }

    pub fn gpu_handle(bytes: u64) -> Self {
        Self {
            bytes: 0,
            heap_bytes: 0,
            external_bytes: 0,
            gpu_bytes: bytes,
            cpu_copies: 0,
            gpu_copies: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BudgetViolation {
    Bytes { actual: u64, limit: u64 },
    HeapBytes { actual: u64, limit: u64 },
    ExternalBytes { actual: u64, limit: u64 },
    GpuBytes { actual: u64, limit: u64 },
    CpuCopies { actual: u32, limit: u32 },
    GpuCopies { actual: u32, limit: u32 },
}

impl fmt::Display for BudgetViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bytes { actual, limit } => write!(f, "bytes {actual} exceeds budget {limit}"),
            Self::HeapBytes { actual, limit } => {
                write!(f, "heap bytes {actual} exceeds budget {limit}")
            }
            Self::ExternalBytes { actual, limit } => {
                write!(f, "external bytes {actual} exceeds budget {limit}")
            }
            Self::GpuBytes { actual, limit } => {
                write!(f, "gpu bytes {actual} exceeds budget {limit}")
            }
            Self::CpuCopies { actual, limit } => {
                write!(f, "cpu copies {actual} exceeds budget {limit}")
            }
            Self::GpuCopies { actual, limit } => {
                write!(f, "gpu copies {actual} exceeds budget {limit}")
            }
        }
    }
}

impl std::error::Error for BudgetViolation {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/comms/ExternalBufferRef.ts"
)]
pub struct ExternalBufferRef {
    pub provider: String,
    pub handle: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/comms/GpuBufferRef.ts"
)]
pub struct GpuBufferRef {
    pub device: String,
    pub handle: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/comms/BufferLeaseKind.ts"
)]
pub enum BufferLeaseKind {
    Borrowed,
    Owned,
    Shared,
    External,
    Gpu,
}

#[derive(Debug, Clone)]
pub enum BufferLease<T> {
    Borrowed(T),
    Owned(T),
    Shared(Arc<T>),
    External(ExternalBufferRef),
    Gpu(GpuBufferRef),
}

impl<T> BufferLease<T> {
    pub fn kind(&self) -> BufferLeaseKind {
        match self {
            Self::Borrowed(_) => BufferLeaseKind::Borrowed,
            Self::Owned(_) => BufferLeaseKind::Owned,
            Self::Shared(_) => BufferLeaseKind::Shared,
            Self::External(_) => BufferLeaseKind::External,
            Self::Gpu(_) => BufferLeaseKind::Gpu,
        }
    }

    pub fn zero_copy_eligible(&self) -> bool {
        matches!(self, Self::Shared(_) | Self::External(_) | Self::Gpu(_))
    }

    pub fn measured_cost(&self, payload_bytes: u64) -> ResourceCost {
        match self {
            Self::Borrowed(_) | Self::Owned(_) => ResourceCost::control_bytes(payload_bytes),
            Self::Shared(_) => ResourceCost {
                bytes: payload_bytes,
                heap_bytes: payload_bytes,
                external_bytes: 0,
                gpu_bytes: 0,
                cpu_copies: 0,
                gpu_copies: 0,
            },
            Self::External(reference) => ResourceCost {
                bytes: 0,
                heap_bytes: 0,
                external_bytes: reference.bytes,
                gpu_bytes: 0,
                cpu_copies: 0,
                gpu_copies: 0,
            },
            Self::Gpu(reference) => ResourceCost::gpu_handle(reference.bytes),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/comms/TransportEnvelope.ts"
)]
pub struct TransportEnvelope<T> {
    pub id: MessageId,
    pub correlation_id: CorrelationId,
    pub causality: Causality,
    /// WHO sent this — the substrate's one actor identity ([`PeerId`]), not a
    /// client-kind label. `EndpointId::new("browser")` encoded the assumption that
    /// the web client is a distinguished endpoint; it is one client among many
    /// (mobile, SDK, TUI, another node's core), and every one of them addresses as
    /// a peer.
    #[ts(type = "string")]
    pub source: PeerId,
    /// WHO this is for. Same rule as [`Self::source`].
    #[ts(type = "string")]
    pub target: PeerId,
    pub class: PayloadClass,
    pub budget: ResourceBudget,
    pub integrity: IntegrityHint,
    pub payload: T,
}

impl<T> TransportEnvelope<T> {
    pub fn new(
        id: MessageId,
        source: PeerId,
        target: PeerId,
        class: PayloadClass,
        budget: ResourceBudget,
        payload: T,
    ) -> Self {
        Self {
            correlation_id: CorrelationId::of_exchange(id),
            id,
            causality: Causality::root(0),
            source,
            target,
            class,
            budget,
            integrity: IntegrityHint::unchecked(),
            payload,
        }
    }
}

pub trait ResourceAccounted {
    fn declared_budget(&self) -> &ResourceBudget;
    fn measured_cost(&self) -> ResourceCost;

    fn assert_within_budget(&self) -> Result<(), BudgetViolation> {
        self.declared_budget().validate(&self.measured_cost())
    }
}

pub trait ZeroCopyEligible {
    fn copy_count(&self) -> u32;
    fn can_share_zero_copy(&self) -> bool;
    fn external_ref(&self) -> Option<&ExternalBufferRef>;
    fn gpu_ref(&self) -> Option<&GpuBufferRef>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_budget_accepts_small_control_payload() {
        let budget = ResourceBudget::control(250);
        let cost = ResourceCost::control_bytes(128);

        assert!(budget.validate(&cost).is_ok());
    }

    #[test]
    fn control_budget_rejects_excess_cpu_copies() {
        let budget = ResourceBudget::control(250);
        let cost = ResourceCost {
            cpu_copies: 2,
            ..ResourceCost::control_bytes(128)
        };

        assert_eq!(
            budget.validate(&cost),
            Err(BudgetViolation::CpuCopies {
                actual: 2,
                limit: 1
            })
        );
    }

    #[test]
    fn zero_copy_media_budget_accepts_gpu_handle() {
        let budget = ResourceBudget::zero_copy_media(33, 8_294_400);
        let lease: BufferLease<Vec<u8>> = BufferLease::Gpu(GpuBufferRef {
            device: "metal:0".into(),
            handle: "texture-42".into(),
            bytes: 8_294_400,
        });

        assert_eq!(lease.kind(), BufferLeaseKind::Gpu);
        assert!(lease.zero_copy_eligible());
        assert!(budget.validate(&lease.measured_cost(0)).is_ok());
    }

    #[test]
    fn zero_copy_media_budget_rejects_cpu_bytes() {
        let budget = ResourceBudget::zero_copy_media(33, 8_294_400);
        let lease = BufferLease::Owned(vec![0_u8; 1024]);

        assert_eq!(
            budget.validate(&lease.measured_cost(1024)),
            Err(BudgetViolation::Bytes {
                actual: 1024,
                limit: 512
            })
        );
    }

    // what this catches: the envelope's wire shape — ids serialize as plain UUID
    // strings (transparent newtypes), and an exchange's correlation equals the id
    // of the message that rooted it.
    #[test]
    fn envelope_serializes_stable_shape() {
        let id = MessageId::new();
        let source = PeerId::new();
        let target = PeerId::new();
        let envelope = TransportEnvelope::new(
            id,
            source,
            target,
            PayloadClass::Command,
            ResourceBudget::control(500),
            serde_json::json!({"command": "ping"}),
        );

        let value = serde_json::to_value(&envelope).unwrap();
        assert_eq!(value["id"], id.to_string());
        assert_eq!(value["correlation_id"], id.to_string());
        assert_eq!(value["source"], source.to_string());
        assert_eq!(value["target"], target.to_string());
        assert_eq!(value["class"], "command");
        assert_eq!(value["payload"]["command"], "ping");
    }

    // what this catches (Joel, 2026-08-13 — "UUIDs are NOT strings"): every minted
    // id is unique by construction. The old `MessageId::new("msg-1")` made two
    // unrelated messages collide the moment two callers picked the same label.
    #[test]
    fn minted_message_ids_are_unique() {
        assert_ne!(MessageId::new(), MessageId::new());
    }

    #[test]
    fn payload_class_marks_bulk_hot_paths() {
        assert!(PayloadClass::VideoFrame.is_bulk());
        assert!(PayloadClass::GpuFrameHandle.is_bulk());
        assert!(!PayloadClass::Command.is_bulk());
    }
}
