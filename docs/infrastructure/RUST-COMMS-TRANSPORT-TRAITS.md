# Rust Comms Transport Traits

**Status:** design for #1175. Rust is the source of truth; TypeScript consumes
generated edge types through `ts-rs` and should not own transport policy.

## Problem

Continuum has several communication paths with the same hidden shape:

- build an envelope around a command, event, transcript message, media frame, or
  artifact pointer
- track identity, correlation, ordering, and replay safety
- enforce some budget: bytes, latency, queue depth, CPU, memory, GPU residency,
  retry count, or retention
- decide who owns the buffer and whether the next hop may borrow, clone, move,
  spill, or drop it

Today those concerns are repeated across IPC, grid transport, AIRC projection,
live media, and planned remote execution. The repetition is the smell. The fix
is a small Rust-owned trait layer that every transport implements, with a
shared envelope, shared resource accounting, and explicit ownership semantics.

## Existing Surfaces

| Surface | Current role | Payload class | Hot-path risk |
|---|---|---|---|
| `ipc/*` and command runtime | Browser/Node to Rust command execution | JSON command/request/response | unbounded calls, timeout drift, duplicate envelope logic |
| `modules/grid/*` | node-to-node routing over Tailscale/Reticulum-style links | `GridFrame` JSON | transport-specific frames hide common budgets |
| `airc/*` and `modules/airc.rs` | AIRC queue/transcript projection into Continuum | issue/card/transcript JSON | process spawn cost, unclear retention boundaries |
| `live/transport/*` | LiveKit/WebRTC bridge and call server | audio/video tracks, session events | accidental CPU copies, codec-specific duplication |
| `live/avatar/*` and Bevy-facing paths | avatar render output and animation state | GPU textures, frame handles, pose/state events | rasterizing to CPU buffers instead of transferring handles |
| `modules/sentinel/*` | agent workflow execution | steps, logs, tool calls, artifacts | log/event transport policy spread across steps |
| data/entity modules | durable projections and CRUD | typed entities, generated TS | schema drift if TS recreates Rust contracts |

These should stay separate at the product boundary. They should not stay
separate for envelope shape, budget enforcement, observability, or buffer
ownership.

## Non-Negotiables

- Rust defines transport contracts, policy, and resource accounting.
- TypeScript receives generated types or thin adapters; it does not invent
  parallel envelopes.
- Heavy payloads do not cross AIRC. AIRC carries messages, manifests, hashes,
  room ids, job ids, and proof pointers.
- Media and render paths prefer handle transfer over CPU bytes. CPU copy is a
  named fallback with a metric and a test gate.
- Every transport has backpressure. Dropping, retrying, spilling, or refusing is
  explicit.
- Every payload declares a resource budget before it is sent.
- Every envelope has correlation, causality, provenance, and replay fields.

## Core Types

The first code slice should add these types under a neutral Rust module such as
`core/continuum-core/src/comms/`.

```rust
pub struct TransportEnvelope<T> {
    pub id: MessageId,
    pub correlation_id: CorrelationId,
    pub causality: Causality,
    pub source: EndpointId,
    pub target: EndpointId,
    pub class: PayloadClass,
    pub budget: ResourceBudget,
    pub integrity: IntegrityHint,
    pub payload: T,
}

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

pub struct ResourceBudget {
    pub max_bytes: u64,
    pub deadline_ms: u64,
    pub max_queue_depth: u32,
    pub cpu_copy_budget: CopyBudget,
    pub memory_budget: MemoryBudget,
    pub gpu_budget: GpuBudget,
    pub retry_budget: RetryBudget,
    pub retention: RetentionPolicy,
}

pub enum BufferLease<T> {
    Borrowed(T),
    Owned(T),
    Shared(Arc<T>),
    External(ExternalBufferRef),
    Gpu(GpuBufferRef),
}
```

The important part is not the exact names. The important part is that ownership
and accounting are typed, reviewed, and impossible to forget at each callsite.

## Trait Surface

```rust
#[async_trait]
pub trait ContinuumTransport: Send + Sync {
    type Payload: Send + Sync + 'static;
    type Error: std::error::Error + Send + Sync + 'static;

    fn name(&self) -> &'static str;
    fn capabilities(&self) -> TransportCapabilities;
    fn local_endpoint(&self) -> EndpointId;
    fn metrics(&self) -> TransportMetricsSnapshot;

    async fn send(
        &self,
        envelope: TransportEnvelope<BufferLease<Self::Payload>>,
    ) -> Result<DeliveryReceipt, Self::Error>;

    async fn recv(&self) -> Result<TransportEnvelope<BufferLease<Self::Payload>>, Self::Error>;
    async fn flush(&self, fence: FlushFence) -> Result<(), Self::Error>;
    async fn shutdown(&self) -> Result<(), Self::Error>;
}

pub trait ResourceAccounted {
    fn declared_cost(&self) -> ResourceCost;
    fn measured_cost(&self) -> ResourceCost;
    fn assert_within_budget(&self, budget: &ResourceBudget) -> Result<(), BudgetViolation>;
}

pub trait ZeroCopyEligible {
    fn copy_count(&self) -> u32;
    fn can_share_across(&self, boundary: TransportBoundary) -> bool;
    fn external_ref(&self) -> Option<ExternalBufferRef>;
    fn gpu_ref(&self) -> Option<GpuBufferRef>;
}
```

This is intentionally above `GridTransport`. `GridTransport` remains the
node-link implementation detail. `ContinuumTransport` is the common contract for
IPC, AIRC projection, grid routing, media, and artifact/control messaging.

## Transport Adapters

| Adapter | First implementation target | Notes |
|---|---|---|
| `IpcCommandTransport` | Rust IPC command boundary | wraps command/response envelopes and makes timeout/backpressure visible |
| `AircQueueTransport` | `airc/queue-scan` and transcript projection | process cost and retention are measured, AIRC stays lightweight |
| `GridNodeTransport` | existing `GridTransport` | maps `GridFrame` into common envelopes without deleting current tests |
| `LiveMediaTransport` | live audio/session events | track-level budgets, no duplicate audio/video policy |
| `GpuFrameTransport` | Bevy/avatar to LiveKit path | handle-first path; CPU raster bytes require fallback metric |
| `ArtifactManifestTransport` | Forge/proof/data pointers | moves hashes and manifests, not bulky artifacts |

Each adapter can start as a thin wrapper around existing code. The win is that
the wrappers expose common metrics and budget failures immediately.

## Budget Gates

Every merged adapter should add tests or VDD probes for the relevant budget:

- command/control: request timeout propagation, cancellation, queue depth,
  retry count, and response correlation
- AIRC: CLI process latency, bytes emitted, retained transcript rows, and
  explicit skip for heavy payload classes
- grid: frame bytes, connect latency, encryption capability, replay rejection
- audio: frame duration, sample rate, queue depth, drop count, and copy count
- video/render: GPU residency, frame handle transfer, CPU copy count, encode
  latency, and frame pacing
- artifacts: manifest byte size, hash integrity, storage pointer validity, and
  retention policy

A PR that moves a hot path must prove one of these numbers did not regress.
When the number is not yet measurable, the PR adds the probe before changing
the path.

## Migration Plan

1. Add `comms` core types and unit tests for serialization, budget validation,
   and copy-count accounting. Export only TS-safe types with `ts-rs`.
2. Wrap AIRC queue scan and IPC command calls first because they are lower-risk
   JSON/control paths.
3. Wrap `GridTransport` without removing the current trait. This gives remote
   execution shared accounting while preserving Tailscale/Reticulum tests.
4. Wrap live audio session events and add copy-count metrics before touching
   video.
5. Add the GPU frame handle path separately. The acceptance test must fail if a
   Bevy-to-LiveKit path rasterizes through CPU memory without an explicit
   fallback reason.
6. Move repeated envelope/budget helpers out of individual modules as adapters
   land. No parallel TS policy layer.

## Issue Backlog From This Design

- `comms: add TransportEnvelope, ResourceBudget, and BufferLease Rust types`
- `comms: wrap AIRC queue scan with resource-accounted transport adapter`
- `comms: wrap IPC command execution with cancellation/backpressure budgets`
- `comms: add GridTransport adapter for shared envelope/accounting`
- `live: add media copy-count probes before video transport refactor`
- `render: design GPU frame-handle transfer gate for Bevy to LiveKit`

These are deliberately small enough for concurrent AIRC lanes. The design is
only useful if it becomes several mergeable slices rather than one giant
rewrite.

## Acceptance Criteria

- New transport work starts from the Rust `comms` traits unless it documents why
  the shared layer does not apply.
- Generated TypeScript reflects Rust types; no hand-written duplicate
  envelopes.
- Hot-path PRs report latency, bytes, copy counts, or queue depth in evidence.
- AIRC remains a coordination/manifest substrate and never becomes the media or
  artifact bulk path.
- Repeated envelope, budget, and ownership logic is removed as each adapter
  lands.
