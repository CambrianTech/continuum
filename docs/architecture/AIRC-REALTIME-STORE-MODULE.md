# `airc/realtime_store` — Design

> **Scope**: this doc covers the in-memory realtime store — the Rust-side substrate that handles `airc/realtime-publish` and `airc/realtime-replay` before any external airc transport attaches. The broader airc module (queue scan, daemon transport, file transport) is out of scope here.
>
> **Status**: store shipped pre-session; concurrency stress tests + moment-of-truth precondition doc shipped in PR #1492.
>
> **File**: `src/workers/continuum-core/src/airc/realtime_store.rs`
>
> **Canonical reference**: [COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md](COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md)

## Role

**Events** primitive substrate. Stores AIRC realtime envelopes with:
- bounded per-room replay queue (default 2,000 events / room)
- coalesced ephemeral presence (typing, thinking, listening — keyed; latest wins; auto-expires)
- coalesced peer manifests (capability index; latest per peer; auto-expires)
- subscription state (subscribe/unsubscribe/ack tracked per subscriber+topic)

This is the **moment-of-truth substrate** for headless-Rust. Multi-persona chat lands here via `airc/realtime-publish`; persona inboxes drain here via cursor polling on `airc/realtime-replay`. The store is what makes chat → persona round-trip work without Node in the loop.

The store is the **in-process** transport — when external airc attaches (daemon/file/queue), it routes around or in addition to this. For moment-of-truth, in-process is enough.

## Command surface

| Command | Handler in | Notes |
|---|---|---|
| `airc/realtime-publish` | `modules/airc.rs` | Validates envelope, calls `InMemoryAircRealtimeStore::publish` |
| `airc/realtime-replay` | `modules/airc.rs` | Cursor-paginated read of room events + active presence/subscriptions/peer manifests/capability index |

The store itself is a Rust trait (`AircRealtimeStore`) with one in-memory impl (`InMemoryAircRealtimeStore`). The trait shape:

```rust
pub trait AircRealtimeStore: Send + Sync {
    fn publish(&self, params: AircRealtimePublishParams) -> Result<AircRealtimePublishResult, String>;
    fn replay(&self, params: AircRealtimeReplayParams) -> Result<AircRealtimeReplayResult, String>;
}
```

Both methods are sync. They run inside the airc module's `async fn handle_command`, but the store itself doesn't `.await` anything internally — pure in-memory ops under one mutex.

## Cross-module dependencies

**None** for the store itself. Consumers (chat/send, persona inbox subscribers, widgets) reach the store through the airc module's command surface, not by importing it directly. Substrate principle: modules talk via commands.

## State model

ONE module-wide `parking_lot::Mutex<AircRealtimeState>` protects all state:

```rust
struct AircRealtimeState {
    rooms: HashMap<Uuid, VecDeque<StoredRealtimeEnvelope>>,   // per-room replay queue
    room_lamports: HashMap<Uuid, u64>,                         // per-room Lamport counter
    presence: HashMap<String, AircRealtimeEnvelope>,           // coalesced by presence key
    peer_manifests: HashMap<String, AircRealtimeEnvelope>,     // coalesced by peer key
    subscriptions: HashMap<String, AircSubscriptionEvent>,     // coalesced by subscriber/topic
}
```

### Why a module-wide mutex (not per-room sharding)

The store IS module-wide because per-room sharding adds complexity without changing the moment-of-truth correctness story. For 5–10 personas, mutex contention is sub-microsecond on uncontended in-memory ops — negligible. For 50+ personas it becomes a real bottleneck.

**Future refinement (flagged in PR #1492, NOT scheduled)**: shard state by room_id:

```rust
struct AircRealtimeState {
    rooms: DashMap<Uuid, Arc<parking_lot::Mutex<RoomState>>>,
}
```

This would unblock multi-room throughput while keeping the same correctness contract. Not needed for moment-of-truth; the module-wide lock is the simplest substrate that meets the requirements.

### Replay queue bound

`DEFAULT_EVENTS_PER_ROOM = 2_000`. When a room's queue reaches the bound, oldest events get popped from the front. **Known limitation** (out of scope here): a replayer with a stale cursor whose Lamport is older than the queue's oldest entry silently misses events 6..99 if the queue starts at 100. Future PR can add a "did_truncate" hint or a "your-cursor-is-stale-please-resync" signal.

### Coalesced presence + peer manifest pruning

`prune_expired_presence(now_ms)` runs on every publish AND on every replay that passes a `now_ms` parameter. Presence events with `expires_at_ms < now_ms` get removed; same for peer manifests. Pruning under the same module-wide mutex keeps consistency.

## Events emitted

The store IS the event log — consumers replay from it rather than subscribing to publish-time emissions. The flow:

1. Publisher calls `airc/realtime-publish` → store appends to room queue + updates Lamport
2. Subscriber calls `airc/realtime-replay` with `after_cursor` → store returns events strictly after the cursor + new cursor for the next round

This is the **cursor polling pattern** — the canonical way persona inboxes and widget subscribers drain the event stream.

## Concurrency contract

**Module-wide correctness** — all state mutations atomic under the parking_lot Mutex; per-room Lamport monotonicity holds; replay sees consistent snapshots; cursor polling never duplicates or loses events.

### Pinned invariants (multi-thread tests in `airc::realtime_store::tests`)

1. **`concurrent_publishes_to_same_room_lose_no_events_and_keep_lamports_contiguous`** — 64 concurrent publishers to GENERAL; final replay returns all 64; every Lamport in 1..=64 appears exactly once (no gaps, no duplicates from a race)
2. **`concurrent_publishes_to_different_rooms_keep_independent_lamport_sequences`** — 60 publishers across 3 rooms; each room's final Lamport == 20; cross-room interleaving doesn't break per-room contiguity
3. **`replay_during_concurrent_publish_observes_consistent_snapshot`** — 32 publishers + 8 replayers racing; each replayer's observed events are a consistent subset (no torn reads — no duplicates within one replay, no out-of-range timestamps); final replay returns all 32
4. **`cursor_polling_during_concurrent_publish_never_loses_or_duplicates_events`** — 40 staggered publishers + 1 cursor-polling consumer; no duplicate event_ids in the observed set; every published event eventually observed

All multi-thread with `worker_threads = 4`. PR #1492 codified these as moment-of-truth preconditions.

### Lamport monotonicity guarantee

Per-room Lamport is incremented under the module-wide mutex during each `push_replay`. Two concurrent publishes to the same room serialize through the mutex; one increments first, the other sees the next value. No race possible.

### Cursor protocol contract

The `AircReplayCursor` returned by `publish` (and at the tail of `replay`) is `{ room_id, lamport, event_id, observed_at_ms }`. A subsequent `replay` with `after_cursor = Some(c)` returns events where `c.strictly_before(event.cursor)` — strictly increasing Lamport order. No event served twice for the same cursor; no event skipped.

## Migration notes

**No TS predecessor.** Designed fresh in Rust as the in-process airc substrate. The wire shape (envelope / payload / delivery / replay cursor) is canonical from the start; the in-memory store implements the trait that future external transports also implement.

## Kinks found

**Concurrency invariants proven, throughput constraint flagged.**

1. **Module-wide mutex serializes multi-room throughput.** All 4 concurrency tests pass with the current design (correctness holds), but the design serializes cross-room work unnecessarily. Future per-room sharding (DashMap<Uuid, Mutex<RoomState>>) is the natural evolution when persona count grows past ~10. Flagged in PR #1492 commit message + this doc; NOT blocking for moment-of-truth.

2. **Stale cursor + replay queue bound** (known limitation, out of scope). A subscriber whose cursor lamport is older than the queue's oldest entry silently misses the pruned events. Future PR can add a `was_truncated: bool` hint to the replay result, or a sentinel error like "cursor stale, oldest available is N — resync from current snapshot." Not a concurrency bug; a substrate-contract gap.

3. **Other transports unproven.** PR #1492 pins ONLY the in-memory transport. Daemon-attached / file-store / queue-client transports get their own concurrency audit when they become hot paths.

### What this gives the moment-of-truth test

| Risk | Pinned by test |
|---|---|
| Multi-persona chat publishes lose events | ✅ `concurrent_publishes_to_same_room_lose_no_events_...` |
| Per-room Lamport breaks under cross-room interleaving | ✅ `..._different_rooms_keep_independent_lamport_sequences` |
| Replay during publish sees torn/partial state | ✅ `replay_during_concurrent_publish_observes_consistent_snapshot` |
| Cursor polling gives the same event twice or skips one | ✅ `cursor_polling_during_concurrent_publish_never_loses_or_duplicates_events` |

The four together guarantee: **chat → airc → persona inbox round-trip works correctly under multi-persona load.** That's the moment-of-truth precondition.

## References

- PR #1492 — Concurrency stress tests (4 tests pinning moment-of-truth invariants)
- `src/workers/continuum-core/src/airc/realtime.rs` — Envelope + cursor + presence + manifest type defs
- `src/workers/continuum-core/src/modules/airc.rs` — `airc/realtime-publish` + `airc/realtime-replay` command handlers
- [COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md §4](COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md) — concurrency doctrine
- Memory: `headless-rust-must-work-soon`, `three-primitives-commands-events-persona`
