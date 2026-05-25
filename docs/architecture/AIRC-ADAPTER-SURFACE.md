# Continuum ↔ airc-lib Adapter Surface (Lane C2)

**Status:** Design (2026-05-24). First slice of Lane C2 from the post-#963 integration plan.
**Author:** claude-tab-1 (continuum scope), coordinating on `#cambriantech` with codex.
**Scope:** continuum-core's Rust adapter to AIRC. Excludes airc-lib internals (`airc-lib/webrtc.rs` is off-limits per coordination), and excludes Lane C1 (docs roll-up) / Lane C3 (live-room media fixture).

---

## 1. What exists today

continuum-core already has a Rust-native AIRC boundary at `src/workers/continuum-core/src/airc/` (5 files, 1.5K LOC):

| File | Role | Backing |
|---|---|---|
| `mod.rs` | barrel re-exports | — |
| `process.rs` | `AircCommandRunner` trait + `TokioAircCommandRunner` impl | shells out via `tokio::process::Command` |
| `client.rs` | `AircQueueClient` trait + `CliAircQueueClient` impl | runs `airc queue list` and parses stdout JSON |
| `realtime.rs` | typed envelopes (`AircRealtimeEnvelope`, `AircRealtimeSchema`, `AircPresenceEvent`, `AircSubscriptionEvent`, `AircMediaControlEvent`, `AircReceipt`) | data types only |
| `realtime_store.rs` | `AircRealtimeStore` trait + `InMemoryAircRealtimeStore` impl | tokio `RwLock` over `Vec` per channel |
| `types.rs` | queue card / scan envelopes (`AircQueueCardEnvelope`, `AircQueueScanResult`) | data types only |

ts-rs already exports every wire type to `src/shared/generated/airc/` (23 `.ts` files). The TS side has the bindings; nothing on the TS side consumes them yet — they're scaffolded for the migration the other claude-tab named (chat → events → room-membership → signaling → media).

The ServiceModule at `src/workers/continuum-core/src/modules/airc.rs` exposes three IPC commands:

  - `airc/queue-scan` → `AircQueueClient::list_queue` → today: shells out.
  - `airc/realtime-publish` → `AircRealtimeStore::publish` → today: writes to an **in-process** `Vec`. Other Continuum scopes do not see it; airc peers do not see it.
  - `airc/realtime-replay` → `AircRealtimeStore::replay` → today: reads from the same in-process `Vec`.

The two "today" notes above are the C2 gap. The trait shape is right; the implementations are stubs that fake the substrate.

There is one persona-side AIRC consumer: `src/workers/continuum-core/src/persona/airc_admission.rs` converts a signed `AircAdmissionEnvelope` into an `AdmissionCandidate` for the engram store. That site does not yet consume real AIRC traffic — it's a typed protocol edge waiting for the substrate.

No `live/` code references `crate::airc`. The live-room Layer (audio/video/avatar/transport) currently goes through LiveKit; airc-lib's WebRTC media tracks land via Lane C3, not here.

---

## 2. What `airc-lib` now exposes

After PRs #944–963 on `rust-rewrite`, the SDK surface that C2 needs is:

  - `airc_lib::Airc::open(home)` / `Airc::attach(...)` / `Airc::open_with_policy(...)` — top-level handle.
  - `Airc::join(name)` / `Airc::join_default_context(...)` / `Airc::ensure_join_context(JoinContext)` — channel subscription.
  - `airc_lib::Room` + `airc_lib::subscriptions::{Subscription, SubscriptionSet}` — typed event stream per channel.
  - `airc_lib::stream::{EventStream, FilteredEventStream, EventFilter}` — replay / live stream primitives.
  - `airc_lib::command_bus::{PendingCommand, request, reply, await_reply}` — request/reply over the substrate.
  - `airc_lib::work::{create_work_card, claim_work_card, release_work_claim, create_work_lane, change_work_lane_state, observe_pull_requests, ...}` — typed work coordination (covers the queue-scan use case).
  - `airc_lib::lifecycle::*` — PeerArrived/Departed/RoomJoined/Parted typed events (the substrate side of "personas as AIRC peers").
  - `airc_lib::webrtc_media::*` — media tracks. **Owned by Lane C3** — C2 does not touch.

The substrate-vs-semantic split from `AGENT-BACKBONE-INTEGRATION.md §2` applies: airc-lib carries typed envelopes with headers; Continuum's adapter layer projects them onto JTAG / event-bus / persona-engram domain types. The adapter is where those projections live.

---

## 3. Proposed adapter shape

Three new trait families. Each one drops into the existing `crate::airc::*` module, alongside the current types (so callers keep importing from the same place).

### 3.1 `AircSubstrate` — the handle abstraction

```rust
#[async_trait]
pub trait AircSubstrate: Send + Sync {
    /// Opaque peer id; matches `airc_lib::PeerId` when lib-backed.
    fn peer_id(&self) -> PeerId;

    /// Subscribe to a channel; returns a stream of typed envelopes.
    async fn subscribe(&self, channel: &str) -> Result<BoxStream<AircRealtimeEnvelope>, AdapterError>;

    /// Publish a typed envelope. Crosses the substrate; other peers see it.
    async fn publish(&self, envelope: AircRealtimeEnvelope) -> Result<AircReceipt, AdapterError>;

    /// Replay durable events from a cursor (for catch-up and persona admission).
    async fn replay(&self, channel: &str, since: AircReplayCursor) -> Result<Vec<AircRealtimeEnvelope>, AdapterError>;
}
```

  - **`CliAircSubstrate`** (today's path, kept for the transition): wraps `TokioAircCommandRunner`. Publish/replay degrade to the in-memory store (current behavior).
  - **`LibAircSubstrate`** (the C2 target): wraps `Arc<airc_lib::Airc>`. `subscribe` returns a `FilteredEventStream` adapted to `AircRealtimeEnvelope`; `publish` calls into the SDK's send path; `replay` uses `Subscription::subscription_cursor` + the SDK event store.

The existing `AircRealtimeStore` trait stays, but its only future impl is the lib-backed one. The in-memory impl moves under `#[cfg(test)]`.

### 3.2 `AircWorkSource` — work queue + PR observation

```rust
#[async_trait]
pub trait AircWorkSource: Send + Sync {
    async fn list_queue(&self, req: AircQueueListRequest) -> AircQueueScanResult;
    async fn observe_pull_requests<S: PullRequestSource>(&self, src: S) -> Result<PrObservation, AdapterError>;
    async fn create_work_card(&self, ...) -> Result<WorkCardId, AdapterError>;
    async fn claim_work_card(&self, ...) -> Result<ClaimId, AdapterError>;
}
```

  - **`CliAircWorkSource`** (rename of `CliAircQueueClient` once dust settles): keeps the shell-out path for the queue-scan IPC command during transition.
  - **`LibAircWorkSource`** (the C2 target): wraps `Arc<airc_lib::Airc>`. `list_queue` → `Airc::work_board_projection`; `observe_pull_requests` → `airc_lib::work::observe_pull_requests` directly. The PR observation skeleton + GitHub source adapter are already merged in airc (#947, #950); this just wraps them.

### 3.3 `AircLifecycleSource` — peer + room presence

```rust
#[async_trait]
pub trait AircLifecycleSource: Send + Sync {
    async fn subscribe_presence(&self) -> Result<BoxStream<AircPresenceEvent>, AdapterError>;
    async fn subscribe_subscriptions(&self) -> Result<BoxStream<AircSubscriptionEvent>, AdapterError>;
}
```

The wire types (`AircPresenceEvent`, `AircSubscriptionEvent`) already exist in `realtime.rs`. What's missing is the source-of-truth impl that drives them off `airc_lib::lifecycle::*`. This is the "personas as AIRC peers" seam called out in `ALPHA-GAP-ANALYSIS`.

### 3.4 IPC surface additions

The ServiceModule keeps its three current commands, adds three more once `LibAircSubstrate` lands:

  - `airc/subscribe-channel` — start a streaming subscription; emits events through the existing event bus.
  - `airc/work-observe-prs` — wraps `LibAircWorkSource::observe_pull_requests`.
  - `airc/lifecycle-watch` — start the presence + subscription event stream.

All three return the same typed envelopes already in `realtime.rs`; the TS side already has bindings under `src/shared/generated/airc/`.

---

## 4. Migration sequence

The other claude-tab's recommended order maps cleanly onto this shape:

| Step | TS-side change | Rust adapter change |
|---|---|---|
| 1. Chat | `src/services/chat` reads from a `LibAircSubstrate.subscribe("chat")` via IPC | new `LibAircSubstrate`; rewire `airc/realtime-publish` + `airc/realtime-replay` to use it |
| 2. Events | `src/system/events` subscribes to the typed event stream | same substrate handle; new `airc/subscribe-channel` IPC command |
| 3. Room membership | `room-membership-daemon` projection over presence + subscription events | new `LibAircLifecycleSource`; new `airc/lifecycle-watch` IPC command |
| 4. Signaling | `scripts/signaling` uses `AircRealtimeSchema::SignalingMessage` envelope already in `realtime.rs` | reuse Step 1's substrate handle |
| 5. Live/media | LiveKit hybrid → AIRC media tracks for direct calls | **Lane C3**. C2 leaves `webrtc_media` alone; the adapter just hands off the `Airc` handle. |

Steps 1–4 are pure C2 land. Step 5 is C3's, but the handle plumbing in Step 1 is the prerequisite — C3 receives the substrate handle, not raw airc-lib.

---

## 5. What this PR does and does not do

**Does (this slice):** ships the proposal as a design doc that codex, the other claude-tabs, and Joel can review and call holes in. Zero code changes; the trait names and IPC command names above are claims, not commits.

**Does not (this slice):** add an `airc-lib` Cargo dependency to continuum-core. The SDK is still moving (PRs #944–963 in the last few days); pinning a path/git dep now risks weekly version bumps for non-content reasons. The wiring lands in a follow-up PR once: (a) airc-lib publishes a versioned crate or stabilizes a path import contract; (b) this design has at least one reviewer pass that doesn't reveal a missing seam.

**Open questions to resolve before Step 1 (chat):**

  1. **Lifecycle of the `Airc` handle.** Single process-wide `Arc<airc_lib::Airc>` owned by the ServiceModule, or per-room handle? The SDK's `Airc::join` returns a `Room`; that suggests per-channel handle inside a shared `Airc`.
  2. **TS↔Rust event delivery.** The current ServiceModule answers one IPC request per call. For `subscribe-channel` we need either: a long-lived IPC stream, or a fan-out where events publish onto Continuum's existing event bus and TS subscribes there. The latter is closer to today's architecture.
  3. **Trust on receive.** `persona::airc_admission` already validates `AircAdmissionEnvelope` signatures via `airc_lib::mesh_identity`'s rotation log. The substrate-side `LibAircSubstrate::subscribe` must surface the same verification result on every envelope; the adapter must not strip it.
  4. **Errors.** `AircQueueScanResult` returns a typed failure today (no `Result`). The new traits use `Result<_, AdapterError>` because airc-lib already does — but the IPC envelopes need to keep the structured-failure shape to avoid breaking TS callers. The mapping is local to the ServiceModule.

---

## 6. Coordination

  - **C2 owner:** claude-tab-1, in `~/.airc/worktrees/continuum-c2-adapter` (branch `feat/lane-c2-airc-adapter-surface`).
  - **C1 (docs/planning):** open. Should fold this doc's migration table into `AGENT-BACKBONE-INTEGRATION.md` once a reviewer pass settles the trait names.
  - **C3 (live-room fixture):** open. Will consume the `Arc<airc_lib::Airc>` plumbing this lane lands in Step 1; nothing in C3 needs airc-lib/webrtc.rs source edits, only the handle.
  - **airc substrate (codex):** continues on the rust-rewrite side. C2 follows codex's SDK surface; codex pings if a trait above doesn't fit cleanly onto something airc-lib already does (e.g. `Airc::work_board_projection` vs the proposed `AircWorkSource::list_queue`).

Reply on `#cambriantech` before opening the first wiring PR after this doc lands.
