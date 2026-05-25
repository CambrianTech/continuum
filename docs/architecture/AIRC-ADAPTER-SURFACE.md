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

ts-rs already exports every wire type to `src/shared/generated/airc/` (23 `.ts` files). The TS side has the bindings; the chat consumer landed in continuum#1432 / #1433 (`AircChatPublisher` → shell-out to `airc msg` then `airc publish` JSON-CLI). The in-process `Vec` in `realtime_store.rs` is reachable from the `airc/realtime-publish` IPC command but is not on the live chat path — chat goes pure-TS shell-out today, not through the Rust adapter. The other typed envelopes (events, lifecycle, signaling) have no TS consumer yet.

The ServiceModule at `src/workers/continuum-core/src/modules/airc.rs` exposes three IPC commands:

  - `airc/queue-scan` → `AircQueueClient::list_queue` → today: shells out.
  - `airc/realtime-publish` → `AircRealtimeStore::publish` → today: writes to an **in-process** `Vec`. Other Continuum scopes do not see it; airc peers do not see it. **Not on the live chat path.**
  - `airc/realtime-replay` → `AircRealtimeStore::replay` → today: reads from the same in-process `Vec`. **Not on the live chat path.**

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

    /// Subscribe to a channel; returns a stream of verified envelopes.
    /// Implementations MUST populate `VerifiedEnvelope::verification` on every
    /// item emitted from this stream; consumers MUST NOT bypass it.
    async fn subscribe(&self, channel: &str) -> Result<BoxStream<VerifiedEnvelope>, AdapterError>;

    /// Publish a typed envelope. Crosses the substrate; other peers see it.
    async fn publish(&self, envelope: AircRealtimeEnvelope) -> Result<AircReceipt, AdapterError>;

    /// Replay durable events from a cursor (for catch-up and persona admission).
    /// Like `subscribe`, items carry the verification result.
    async fn replay(&self, channel: &str, since: AircReplayCursor) -> Result<Vec<VerifiedEnvelope>, AdapterError>;
}

/// Receive-side envelope wrapper that pairs the typed body with the substrate's
/// signature-verification outcome. This is the type-system enforcement of the
/// trust-on-receive contract: `subscribe` and `replay` always return verified
/// envelopes, so consumers cannot accidentally process an unverified one.
pub struct VerifiedEnvelope {
    pub envelope: AircRealtimeEnvelope,
    pub verification: VerificationResult,
}

pub enum VerificationResult {
    /// Signature verified against a known peer in the trust registry.
    Verified { signer: PeerId },
    /// Signature missing or signer unknown. `persona::airc_admission` must
    /// reject these unless explicitly running under a dev policy.
    Unverified { reason: VerificationFailure },
}
```

  - **`CliAircSubstrate`** (today's path, kept for the transition): wraps `TokioAircCommandRunner`. Publish/replay degrade to the in-memory store (current behavior).
  - **`LibAircSubstrate`** (the C2 target): wraps `Arc<airc_lib::Airc>`. `subscribe` returns a `FilteredEventStream` adapted to `AircRealtimeEnvelope`; `publish` calls into the SDK's send path; `replay` uses `Subscription::subscription_cursor` + the SDK event store.

The existing `AircRealtimeStore` trait stays, but its only future impl is the lib-backed one. The in-memory impl moves under `#[cfg(test)]`.

### 3.2 `AircWorkSource` — work queue + PR observation

```rust
#[async_trait]
pub trait AircWorkSource: Send + Sync {
    /// Returns the canonical typed envelope today's `airc/queue-scan` IPC
    /// command emits. `AircQueueScanResult` carries `ok: bool` + a structured
    /// `error: Option<AircQueueScanError>` — failures stay inside the success
    /// type so the IPC contract surface to TS callers does not change. The
    /// other methods on this trait return `Result<_, AdapterError>` because
    /// they map 1:1 to airc-lib calls that already use `Result`; the adapter
    /// converts those into the `AircQueueScanResult`-shaped envelope at the
    /// ServiceModule boundary when forwarding to TS.
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

The ServiceModule keeps its three current commands, adds three more once `LibAircSubstrate` lands. Naming reflects the actual semantics — these are not synchronous request/response commands, they hand back a stream handle and the underlying envelopes arrive on the event bus:

  - `airc/channel-stream-start` — start a streaming subscription against a channel; events fan out via the existing event bus; returns a stream handle.
  - `airc/work-observe-prs` — wraps `LibAircWorkSource::observe_pull_requests`. Same start-stream-return-handle shape.
  - `airc/lifecycle-stream-start` — start the presence + subscription event stream. Same shape.

All three return the same typed envelopes already in `realtime.rs`; the TS side already has bindings under `src/shared/generated/airc/`.

**Event-bus semantics (resolved):** the bus is bounded-channel-per-subscriber, not broadcast-all-or-nothing. A slow TS subscriber back-pressures its own channel and may drop ordered events past the high-water mark, but cannot head-of-line block other subscribers. The drop behavior is observable (the dropped count surfaces on the stream handle) so the consumer learns it fell behind rather than silently losing data.

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

**Parallelism note:** Steps 1 (chat) and 3 (room membership) both build on `LibAircSubstrate` + `LibAircLifecycleSource`, which are independent traits. Two tabs can land them in parallel. Step 4 (signaling) and Step 5 (live/media) both gate on Step 1's substrate handle, so they wait. Two-tab pattern works best when Step 1 and Step 3 split between agents.

---

## 5. What this PR does and does not do

**Does (this slice):** ships the proposal as a design doc that codex, the other claude-tabs, and Joel can review and call holes in. Zero code changes; the trait names and IPC command names above are claims, not commits.

**Does not (this slice):** add an `airc-lib` Cargo dependency to continuum-core. The wiring lands in a follow-up PR. Both gates are now met or have a concrete plan:

  - **(a) airc-lib import contract:** workspace path dep pinned to a specific commit SHA, rebased weekly, never auto-updated. Concretely, `Cargo.toml` adds `airc_lib = { path = "../../../../airc/crates/airc-lib", rev = "<SHA>" }` (path during local dev, git SHA in CI). Bump cadence: deliberate, never on every airc PR. This is the smallest contract that lets the wiring PR open without dragging an unsolved versioning debate.
  - **(b) reviewer pass:** satisfied (this revision incorporates Joel's review of the v1 doc).

**Resolved (post-review):**

  1. **Lifecycle of the `Airc` handle.** Single process-wide `Arc<airc_lib::Airc>` owned by the ServiceModule. `Airc::join(name)` returns a `Room` that the substrate keeps in a per-channel `HashMap<ChannelId, Room>`. Avoids handle-construction cost per call.
  2. **TS↔Rust event delivery.** Event-bus fan-out, not long-lived IPC streams. Bounded channel per subscriber (see §3.4 event-bus semantics).
  3. **Trust on receive.** Lifted from open question to trait constraint — `subscribe` and `replay` return `VerifiedEnvelope` not `AircRealtimeEnvelope`, so verification cannot be bypassed by accident. See §3.1.
  4. **Errors.** Resolved: `list_queue` keeps the structured-failure shape (`AircQueueScanResult`) as the IPC contract to TS; other methods use `Result<_, AdapterError>` because they map 1:1 to airc-lib's `Result` form. The adapter converts at the ServiceModule boundary. See §3.2 doc comment on `list_queue`.

**Tracked as follow-up (not in this doc):**

  - Parallelism opportunity: split Step 1 (chat) and Step 3 (room membership) across two tabs (see §4 parallelism note).
  - Cargo-dep concrete bump cadence policy needs a one-line commit in continuum-core's `Cargo.toml` when the wiring PR opens.

---

## 6. Coordination

  - **C2 owner:** claude-tab-1, in `~/.airc/worktrees/continuum-c2-adapter` (branch `feat/lane-c2-airc-adapter-surface`).
  - **C1 (docs/planning):** open. Should fold this doc's migration table into `AGENT-BACKBONE-INTEGRATION.md` once a reviewer pass settles the trait names.
  - **C3 (live-room fixture):** open. Will consume the `Arc<airc_lib::Airc>` plumbing this lane lands in Step 1; nothing in C3 needs airc-lib/webrtc.rs source edits, only the handle.
  - **airc substrate (codex):** continues on the rust-rewrite side. C2 follows codex's SDK surface; codex pings if a trait above doesn't fit cleanly onto something airc-lib already does (e.g. `Airc::work_board_projection` vs the proposed `AircWorkSource::list_queue`).

Reply on `#cambriantech` before opening the first wiring PR after this doc lands.
