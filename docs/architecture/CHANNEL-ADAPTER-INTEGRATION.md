# Channel Adapter Integration — finishing the TS-port + adding Rust-native lazy cells

> **Status**: design doc for task #244 (PR A). Branch `feat/channel-adapter-batch-cognition`.
> **Companion memories**: `[[learn-from-ts-apply-rtos-idealism]]`, `[[cognition-batches-per-channel-adapter]]`, `[[shared-decode-per-persona-perspective]]`, `[[pass-by-reference-lazy-metadata-with-data]]`.
> **Closes matrix row**: Demand-pull eliminates idle work (cognition side) — 🔴 → 🟡.

## What this doc is

This is the design + resumption-checkpoint for the first per-channel batch-adapter PR. The cognition-side of the demand-pull doctrine: a persona service cycle pulls a BATCH per channel and runs `analyze()` ONCE per channel-with-work, rather than per inbox entry. Combined with Arc-shared items carrying lazy-cached derived state (embedding, RAG chunks, future STT/video decode), the cost of N personas in a room scales as `M × decode_cost + N×M × cheap_interpret` instead of `N×M × decode_cost`.

The framing per `[[learn-from-ts-apply-rtos-idealism]]`: TS taught us the smart-item-dumb-channel pattern (BaseQueueItem). The Rust port already has that. What's NEW in Rust (and what TS couldn't do well) is the lazy-cache-on-the-item layer that makes the shared-decode cost split actually free.

## Discovery — what already exists in the Rust substrate

These are NOT to be re-invented:

| Existing | Where | Purpose |
|---|---|---|
| `ActivityDomain` enum | `persona/channel_types.rs` | Audio / Chat / Code / Background — channel discrimination |
| `QueueItemBehavior` trait | `persona/channel_types.rs` | Items own their urgency / consolidation / kick / RTOS aging. Comment says "Mirrors the TypeScript BaseQueueItem abstract class" |
| `ChannelQueue::consolidate(&mut self)` | `persona/channel_queue.rs:101` | Items merge per their `should_consolidate_with` decisions |
| `ChannelQueue::pop()` | `persona/channel_queue.rs:257` | Returns one `Box<dyn QueueItemBehavior>` at a time |
| `ChannelRegistry::service_cycle(state)` | `persona/channel_registry.rs:170` | Production cognition entry point — drives the autonomous loop |
| `ChannelRegistry::route(item)` | `persona/channel_registry.rs:70` | Routes incoming items to per-domain queues |
| Concrete items | `persona/channel_items.rs` | `VoiceQueueItem`, `ChatQueueItem`, `TaskQueueItem` |
| `MediaItemRequest` | `persona/channel_items.rs` | Blob-handle pattern (content-addressed, externalized base64 to disk per `MediaBlobService`) |

The infrastructure is half-built. What's missing is the **lazy-cell layer** + **drain-batch shape** + **analyze-once cognition wiring**.

## The minimum delta (what PR A actually adds)

### Delta 1 — `Box<dyn QueueItemBehavior>` → `Arc<dyn ChannelItem>`

Items must be Arc-shared so multiple personas in the same room hold references to the same item. Box prevents the sharing that makes lazy cells useful.

```rust
// Today:
fn pop(&mut self) -> Option<Box<dyn QueueItemBehavior>>

// PR A:
fn pop_arc(&mut self) -> Option<Arc<dyn ChannelItem>>
```

Migration: `ChannelItem: QueueItemBehavior + ...` (extends), so all existing `should_consolidate_with` / `is_urgent` / `kick_resistance` keep working. Existing call sites convert Box → Arc at the registration boundary.

**⚠️ Design wrinkle — consolidation semantics shift under Arc:**

The existing `ChannelQueue::consolidate_rebuild` (channel_queue.rs:106) operates on `Box<dyn>` items and the consolidation API allows item-mutating merges (TS pattern: "anchor absorbs others"). Under `Arc<dyn>`, items are SHARED — multiple personas may hold references — so they MUST be immutable after enqueue.

**Resolution (the Rust-native shape):** consolidation produces a new `ConsolidatedItem<T>` that wraps `Vec<Arc<T>>` of the originals. Originals stay immutable; the wrapper exposes the same trait surface to consumers. Lazy cells on the originals stay valid; the wrapper aggregates view (e.g., "summary across these 3 messages") via its OWN lazy cell that reads from the originals' cells.

```rust
pub struct ConsolidatedChatItem {
    items: Vec<Arc<ChatItem>>,
    aggregate_summary_cell: OnceLock<Arc<str>>,
}

impl ConsolidatedChatItem {
    pub fn aggregate_summary(&self) -> Arc<str> {
        self.aggregate_summary_cell
            .get_or_init(|| Arc::new(consolidate_summaries(
                self.items.iter().map(|i| i.summary())
            )))
            .clone()
    }
}
```

This is STRICTLY BETTER than the TS shape: per `[[learn-from-ts-apply-rtos-idealism]]`, immutable items + cache-only-grows aligns with both `[[no-fallbacks-ever]]` (no in-place mutation that could partially fail) and `[[strong-typing-across-boundaries]]` (consolidation is a TYPED transition, not a mutation that loses the original observations).

Implementation order: this means the `QueueItemBehavior::should_consolidate_with` predicate stays (items still self-determine grouping), but the consolidation EXECUTION shifts from "merge into anchor" to "build wrapper from group." The trait gets a new method: `fn into_consolidated(items: Vec<Arc<Self>>) -> Arc<dyn ChannelItem>` — or kept as free functions per-channel to avoid trait-object-self issues.

### Delta 2 — Lazy cells on item structs

Each concrete item carries `OnceLock<Arc<T>>` fields for expensive derived state. First consumer's call to `embedding()` triggers compute; everyone else gets the cached Arc clone. Per `[[pass-by-reference-lazy-metadata-with-data]]`.

```rust
pub struct ChatItem {
    id: Uuid,
    raw_text: Arc<str>,
    sender_id: Uuid,
    sender_name: Arc<str>,
    timestamp: u64,
    room_id: Uuid,
    // Lazy cells — pure decoders, cache on first demand:
    embedding_cell: OnceLock<Arc<Embedding>>,
    rag_chunks_cell: OnceLock<Arc<Vec<RagChunk>>>,
}

impl ChatItem {
    pub fn embedding(&self) -> Arc<Embedding> {
        self.embedding_cell
            .get_or_init(|| Arc::new(compute_embedding(&self.raw_text)))
            .clone()
    }

    pub fn rag_chunks(&self) -> Arc<Vec<RagChunk>> {
        self.rag_chunks_cell
            .get_or_init(|| Arc::new(compute_rag_chunks(&self.raw_text, &self.embedding())))
            .clone()
    }
}
```

`compute_embedding` and `compute_rag_chunks` stay PURE — no caching, no orchestration. The impurity (cache, sharing) lives in the cell on the item. Testable, reasonable about.

PR A ships the SEAM (trait methods + cell fields), even if the first decoders are trivial. PR D (audio) is where STT actually exercises the cost split.

### Delta 3 — `ChannelQueue::drain_batch(window_ms) -> Option<CoherentUnit>`

Replaces `pop()` for cognition's consumption path. Internally:
1. Run `consolidate()` (existing logic)
2. Pull all post-consolidation items within `window_ms` of the highest-priority anchor (mirroring `PersonaInbox::drain_frame` shape)
3. Group items by `ActivityDomain` (should be uniform — queue is per-domain — but defensive)
4. Hand the Arc<dyn ChannelItem> Vec to the domain's `BatchAdapter::collect(items)` → `CoherentUnit::<Domain>(typed_batch)`
5. Return Option (None when no work)

```rust
pub enum CoherentUnit {
    Chat(ChatBurst),
    Voice(VoiceClip),
    Task(TaskBatch),
    Background(BackgroundBatch),
}

pub struct ChatBurst {
    pub items: Vec<Arc<ChatItem>>, // still Arc-shared; lazy cells survive
    pub window_span_ms: u64,
    pub message_count: usize,
    // Cluster summary for cognition's prompt context:
    pub senders: SmallVec<[SenderSummary; 4]>,
}
```

Note: the burst carries Arcs to the original items, NOT copies of their text. That's the load-bearing property — cognition's `analyze` can call `item.embedding()` and hit cached cells if some other tick already triggered compute.

### Delta 4 — `PersonaChannelView::interpret(burst, identity) -> CoherentInput`

The per-persona perspective layer above the shared lazy items. Cheap: reads cached fields, ranks, filters by identity. Per `[[shared-decode-per-persona-perspective]]`.

```rust
pub trait PersonaChannelView: Send + Sync {
    fn interpret(
        &self,
        unit: &CoherentUnit,
        identity: &PersonaIdentity,
    ) -> CoherentInput;
}
```

`CoherentInput` is what cognition's `analyze` accepts. One per channel-with-work per service cycle. Cognition fires ONCE on the Vec of inputs, not N times per item.

### Delta 5 — `ChannelRegistry::service_cycle` rewrite

The existing shape pops items one-by-one. The new shape:

```rust
pub fn service_cycle(&mut self, state: &mut PersonaState) -> ServiceCycleResult {
    let mut inputs: Vec<CoherentInput> = Vec::new();

    for domain in DOMAIN_PRIORITY_ORDER.iter().copied() {
        let Some(queue) = self.get_mut(domain) else { continue };
        if !queue.has_work() { continue; }

        // ONE drain per domain per tick (replaces the per-item loop):
        let Some(unit) = queue.drain_batch(state.window_ms()) else { continue };

        // ONE interpret per domain per tick:
        let view = self.view_for(domain);
        let input = view.interpret(&unit, &state.identity);

        inputs.push(input);
    }

    // ONE analyze per service cycle, batching inputs across channels:
    cognition::analyze(&mut state.cognition, inputs)
}
```

This is the load-bearing change: `analyze()` fires ONCE per tick, with a `Vec<CoherentInput>` (one per channel with work), regardless of how many items each channel drained.

### Delta 6 — Architecture proof

New file `core/continuum-core/tests/architecture_demand_pull_cognition.rs`:

```rust
// proves: demand-pull cognition (service tick with N inbox entries on one
// channel = 1 analyze call, not N; cycle wall-clock bounded by
// inference_latency + ε regardless of arrival rate)
#[tokio::test]
async fn service_cycle_with_n_chat_messages_calls_analyze_once() {
    let persona = build_persona_with_recording_cognition();
    let chat_queue = persona.channels.get_mut(ActivityDomain::Chat).unwrap();

    // Enqueue N messages from same sender within window:
    for i in 0..50 {
        chat_queue.enqueue(Arc::new(ChatItem::new(...)));
    }

    // Tick service cycle:
    persona.service_cycle(&mut state);

    // Assert: cognition analyze was called ONCE, not 50 times:
    assert_eq!(persona.cognition_call_count.load(Ordering::Relaxed), 1);
}

// proves: demand-pull cognition (cycle wall-clock bounded; doesn't compound
// with arrival rate)
#[tokio::test]
async fn service_cycle_wallclock_independent_of_arrival_count() {
    let persona_with_5 = build_persona_with_n_messages(5);
    let persona_with_500 = build_persona_with_n_messages(500);

    let t5 = measure_cycle_wallclock(persona_with_5).await;
    let t500 = measure_cycle_wallclock(persona_with_500).await;

    // Bounded ratio: 500 entries vs 5 entries should be within 2× wall-clock,
    // not 100×. The consolidation + batch is cheap; the inference call is one
    // regardless.
    let ratio = t500.as_nanos() as f64 / t5.as_nanos() as f64;
    assert!(ratio < 2.0, "cycle scales O(N): t5={t5:?}, t500={t500:?}, ratio={ratio:.2}×");
}

// proves: shared-decode property (1 embedding compute per item, regardless
// of how many personas in the room consume it)
#[tokio::test]
async fn embedding_computed_once_across_multiple_personas() {
    let item = Arc::new(ChatItem::new(...));
    let compute_count = item.embedding_compute_count_for_testing();

    let mut handles = vec![];
    for _ in 0..16 {
        let item = Arc::clone(&item);
        handles.push(tokio::spawn(async move {
            item.embedding()
        }));
    }
    for h in handles { h.await.unwrap(); }

    assert_eq!(compute_count.load(Ordering::Relaxed), 1,
        "embedding compute fired N times across 16 personas — \
         lazy cell on item should make it fire once and cache");
}
```

These three tests pin the doctrine across the three memory dimensions: cognition-batches-per-channel (1 analyze), bounded-latency (wall-clock doesn't compound), shared-decode (1 compute per item across N consumers).

## Matrix row update

After PR A lands, `PROVING-THE-DOCTRINE.md` "Demand-pull eliminates idle work" row:

```
🔴 → 🟡 — cognition-side proof shipped (1 analyze per channel-tick;
wall-clock bounded; lazy cells make embedding compute share across
personas). Shape-3 substrate-side bench (vision encoder CPU=0 with 0
subscribers) still TODO.
```

## Resumption checkpoint

**Where the session stopped:** doc committed. Code not started. The Delta 1 design wrinkle (consolidation under Arc-sharing) just landed in the doc above. Branch `feat/channel-adapter-batch-cognition` exists but holds only the doc.

If this session ends before PR A is merged, the resumption checklist:

1. Branch `feat/channel-adapter-batch-cognition` is at canary HEAD + the doc itself.
2. Read this doc first (especially the Delta 1 wrinkle resolution — it's the load-bearing design decision). Then the four companion memories. Then `persona/channel_types.rs` and `persona/channel_registry.rs::service_cycle` to confirm the existing shape is still what's documented above.
3. Apply Deltas 1 → 6 in order. Each is testable independently:
   - **Delta 1** (Box → Arc) ships with the existing tests still passing (no behavior change).
   - **Delta 2** (lazy cells) ships with one unit test asserting "compute fires once."
   - **Delta 3** (drain_batch) ships with one unit test asserting "consolidation + window-clustering work end-to-end."
   - **Delta 4** (PersonaChannelView) ships with one unit test asserting "identity-aware interpret returns expected shape."
   - **Delta 5** (service_cycle rewrite) ships with `airc_chat_demo` still working end-to-end — this is the integration smoke; if the demo breaks, the rewrite is wrong.
   - **Delta 6** (architecture proof) ships last; it's the matrix witness.
4. Adversarial review per the session's grooved discipline: spawn reviewer, surface gaps, fold closures into the same PR, re-review, merge.
5. Matrix update + commit message must reference task #244 + the four doctrine memories.

## What this PR does NOT do (intentional follow-ups)

- **PR B**: `DecisionEvent` channel variant + `DecisionBatchAdapter` (votes consolidate into one tally per cycle; ranked-choice cooperation gets cheap)
- **PR C**: `airc_chat_demo` reshape to use the new contract end-to-end (kills the legacy per-message path; killer-loop integration test)
- **PR D**: Audio channel — first real exercise of `decode_once` (STT shared across listeners) + per-persona TTS encode (Maya's voice ≠ Helper's voice)
- **PR E**: Video frames — Bevy/WebRTC-style lazy GPU texture handles on `VideoItem`

Each subsequent PR slots in by ADDING channels and adapters, not retrofitting the trait. That's the test of whether PR A's seam was right.

## Doctrine alignment

- `[[learn-from-ts-apply-rtos-idealism]]` — TS taught smart-item-channel; Rust adds lazy cells; the synthesis is the project's value
- `[[cognition-batches-per-channel-adapter]]` — one thought per batch per channel
- `[[shared-decode-per-persona-perspective]]` — shared expensive decode, per-persona cheap perspective
- `[[pass-by-reference-lazy-metadata-with-data]]` — Arc-shared items + OnceLock cells; the data IS the cache
- `[[strong-typing-across-boundaries]]` — `CoherentUnit` enum, `CoherentInput` typed; no substring-matching on serialized item state
- `[[no-fallbacks-ever]]` — `drain_batch` returns `Option`, not a "best-effort" stub; empty queues return None, not an empty-burst sentinel

## What proof this would have been hard to ship without

The architecture-test matrix work earlier this session is what makes this PR safe to merge:

- **Federated alignment** (#1595): channel items routed in from airc carry verified peer_id; the gate refused hostile dispatches before they reach the inbox
- **Backpressure intrinsic** (#1594): the inbox can't OOM under flood; `LiveLag` surfaces typed overflow
- **Flow geometric** (#1596): K^0.599 — coordination cost stays sub-linear, so even after PR A the multi-persona room math holds
- **Singleton ban** (#1591): no `static OnceLock<Arc<T>>` lurking to compete with the per-item cells

The matrix work was the precondition. PR A is what the matrix work was for.
