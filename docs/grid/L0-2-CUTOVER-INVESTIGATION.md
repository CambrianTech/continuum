# L0-2-cutover — Investigation finding + proposed synthesis

**Status:** investigation, no code changes yet. Posted before L0-2-cutover implementation per Joel 2026-05-29: *"investigate first. might have better ideas. No harm. ... might learn from each other. ... find the best of both worlds. ... we probably know the airc grid better though."*

**Card:** 1089b1b9 (Blocked pending decision)
**Predecessors:** L0-2-respond-call (#1468) merged to canary with 24/24 unit tests; surfacing an architectural mismatch at the production integration layer.

## TL;DR

My L0-2-prep through L0-2-respond-call built a self-contained `PersonaServiceModule` with its own per-persona `EnrolledPersona` map (state, channels, cognition). I didn't realize there were already TWO existing Rust persona infrastructures, so my work created a third parallel one. The unit tests passed because I was staging items into my own state; in production, TS pushes items into the EXISTING state via `channel/enqueue` and my consumer never sees them.

The honest synthesis isn't "throw out existing" or "throw out mine" — both contribute. Mine has the modern doctrine (responder DI, separated inference/service CB thresholds, audited fallback discipline, airc-grid-aware design). Existing has the production-tested storage + producer-side tick + integration with the broader cognition module.

Best-of-both: keep the existing per-persona storage as canonical, refactor `EnrolledPersona` to REFERENCE it instead of duplicating it. Mine becomes the consumer-side tick + responder DI; existing stays the producer-side tick + storage.

## The three queue mechanisms (today)

After tracing the code:

| Mechanism | Location | Producer | Consumer | Status |
|---|---|---|---|---|
| **`PersonaCognition.inbox: PersonaInbox`** (flat) | inside `PersonaCognition` (stored in `channel_state.personas`) | unclear / legacy | `cognition.rs::persona/turn-execute` via `inbox.drain_frame` | **legacy** per persona/mod.rs comments |
| **`channel_state.registries[persona_id]: (ChannelRegistry, PersonaState)`** (modern multi-domain) | `channel.rs::ChannelState` (shared `DashMap`) | TS `RustCognitionBridge.channelEnqueue` → `channel/enqueue` | TS `PersonaAutonomousLoop.runServiceLoop` polls `channel/service-cycle-full` | **production path today** |
| **`EnrolledPersona.channels: ChannelRegistry`** (parallel to #2) | my `PersonaServiceModule.personas` (separate `HashMap`) | only tests | only `PersonaServiceModule.tick` | **duplicate I added** |

The two `ChannelRegistry` instances (#2 and #3) are structurally identical but live in different maps keyed by different mutexes/dashmaps. There's no synchronization between them.

## What `ChannelState`'s tick actually does (60s producer tick)

`channel.rs::ChannelModule.tick` (60-second interval, configurable via `channel/tick-config`):

1. Polls `tasks` collection for pending tasks per persona → enqueues task items
2. Runs `SelfTaskGenerator.tick` per persona → enqueues self-tasks
3. Runs training-data readiness checks
4. NO message dispatch — items just get pushed INTO the channels

So `channel_state` is the PRODUCER side. The CONSUMER side is whatever pops `service_cycle` and dispatches. Currently the consumer is TS `PersonaAutonomousLoop`. That's what I was supposed to replace.

## What `cognition.rs::persona/turn-execute` does

A separate Rust command. Looks up persona from `channel_state.personas` (the shared `DashMap<Uuid, PersonaCognition>`), drains a turn-frame from `PersonaCognition.inbox` (the flat legacy queue), builds an `InferenceRequest`, dispatches via the inference module.

This is the OLDER inference dispatch path. It uses the legacy flat inbox, not the modern `ChannelRegistry`. Effectively a sibling command that bypasses the modern channel system.

Implications:
- The flat `PersonaInbox` is still used by `persona/turn-execute` even though `ChannelRegistry` is the modern shape
- The two paths likely diverged at some point and never reconciled
- `persona/turn-execute` is its own deprecation/migration target separate from my work

## What my `PersonaServiceModule` brought that's new

Genuinely new contributions beyond what existed:

1. **`Responder` trait for dependency injection.** Production binds `DefaultResponder` (calls `persona::response::respond`); tests inject mocks. Lets the consumer be unit-tested without loading a model.
2. **Separated circuit-breaker thresholds**: 5 for service errors (deser, channel access) vs 15 for inference errors (transient hiccup ≠ broken persona). Existing code doesn't make this distinction.
3. **Lock-around-await discipline** for `respond()` (multi-second). The personas mutex is dropped before `.await`, reacquired after, so status/enroll/other personas don't block across inference.
4. **`ResponderConfig` validated at enrollment** — no empty-string defaults that the inference layer would have to fail-loud on. The URI doctrine peer mapped (5133d0a7) aligns — empty model fails at the boundary, not deeper.
5. **`ServicePopDecision` vs `ServiceOnceOutcome` split** — sync pop+evaluate inside the lock returns one shape, async respond() outside the lock returns another. Tight discipline about what runs where.

Existing code has none of these explicitly; instead the TS PersonaAutonomousLoop carries equivalent shape in its own loop body.

## Proposed synthesis: where each part lives

| Concern | Source of truth |
|---|---|
| Per-persona channel storage (modern multi-domain) | `channel.rs::ChannelState.registries` |
| Per-persona cognition state (engine, sleep, rate limit, message cache, etc.) | `channel.rs::ChannelState.personas` (shared `DashMap<Uuid, PersonaCognition>`) |
| Per-persona ResponderConfig (model, system_prompt, capabilities, specialty) | `PersonaServiceModule` — genuinely new, validates at enrollment |
| Per-persona circuit-breaker state (service + inference counters) | `PersonaServiceModule` — genuinely new |
| Producer tick (DB polls, self-task gen, training checks) | `channel.rs::ChannelModule` — production-tested, keep as-is |
| Consumer tick (pop + evaluate + respond) | `PersonaServiceModule` — replaces TS `PersonaAutonomousLoop` |
| Inference dispatch | `Responder` trait, default impl calls `persona::response::respond` |
| Legacy flat-inbox dispatch (`persona/turn-execute`) | Keep working until separately migrated to consume from `ChannelRegistry` |

### What `EnrolledPersona` looks like after refactor

```rust
pub struct EnrolledPersona {
    pub persona_id: Uuid,
    pub display_name: String,
    pub responder_config: ResponderConfig,
    pub circuit_open_until_ms: u64,
    pub consecutive_service_failures: u32,
    pub consecutive_inference_failures: u32,
    // NO cognition: PersonaCognition  — comes from channel_state.personas[persona_id]
    // NO channels: ChannelRegistry    — comes from channel_state.registries[persona_id].0
    // NO state: PersonaState          — comes from channel_state.registries[persona_id].1
}
```

### What `PersonaServiceModule` looks like after refactor

```rust
pub struct PersonaServiceModule {
    /// Per-persona enrollment metadata (config + circuit breaker).
    enrollments: Mutex<HashMap<Uuid, EnrolledPersona>>,
    /// Shared storage from channel.rs — Arc-shared so my module reads what
    /// channel/enqueue writes.
    channel_state: Arc<ChannelState>,
    /// Response dispatcher (production binds DefaultResponder).
    responder: Arc<dyn Responder>,
}
```

### `service_once_for` after refactor

Pops from `channel_state.registries[persona_id]` (existing) instead of `enrolled.channels` (removed). Uses cognition from `channel_state.personas[persona_id]` (existing) instead of `enrolled.cognition` (removed). Everything else (build_respond_input, full_evaluate, the four ServicePopDecision variants) stays the same.

### `drain_all_personas` after refactor

Lock discipline unchanged — collect ids from `enrollments` (brief lock), drop, per id: brief lock to pop+evaluate (touches `channel_state` AND `enrollments`), drop, await respond, brief lock to update circuit-breaker state.

The two locks (`enrollments` and the dashmap-internal `channel_state`) need careful ordering. Worth a comment.

## What L0-2-cutover actually involves under this synthesis

Three commits, in order, each green on its own:

### A) Refactor `PersonaServiceModule` to consume `channel_state` (no production wiring yet, no TS deletion)

- Change `PersonaServiceModule::new` / `with_responder` to take `Arc<ChannelState>` 
- `EnrolledPersona` slims down (drop cognition, channels, state fields)
- `service_once_for` reads from `channel_state.registries[persona_id]` + `channel_state.personas[persona_id]`
- Tests updated: instead of staging items into `EnrolledPersona.channels`, stage them into `channel_state.registries[persona_id]` using the same enqueue path TS uses (or by direct `ChannelRegistry::route`)
- 24/24 tests still pass; respond integration semantics unchanged

### B) Production wire — `PersonaUser.initialize` calls `persona/enroll`

- TS `PersonaUser.initialize` collects `ResponderConfig` from modelConfig + persona config + capabilities + specialty
- Dispatches `Commands.execute('persona/enroll', {persona_id, display_name, model, system_prompt, capabilities, specialty})`
- Production `PersonaServiceModule.tick` now actually runs for enrolled personas (it polls `channel_state.registries` which TS is already pushing to)
- TS `PersonaAutonomousLoop` is **still running** in this commit — both consumers run in parallel
- Verification: 15-persona scenario, look for messages being processed twice or going missing. If they go missing, fix the wiring. If they double, expected — gives us a window to verify the Rust path works end-to-end before deleting TS.

### C) Atomic TS deletion

- Delete `PersonaAutonomousLoop.ts`, all callsites, `PersonaUser.startAutonomousServicing`, `stopServicing`, integration tests that mock the TS loop
- Run the same 15-persona verification — should now go through Rust only
- Net massive TS deletion: 353 + N (callsites across PersonaUser.ts, PersonaTaskExecutor.ts, CognitionLogger.ts, autonomous-learning-e2e.test.ts)

## What I am NOT proposing

- Touching `cognition.rs::persona/turn-execute`. That's the legacy flat-inbox path; it's its own migration target. Leave it working; address separately.
- Touching the producer-side tick in `channel.rs`. It works; integration is already there.
- Deleting any of the four genuinely-new contributions my work added (Responder DI, separated CB thresholds, validated ResponderConfig, lock discipline). Those carry forward into the refactor.

## Followup finding: my `UnsupportedItem` outcome IS silent drop

Joel 2026-05-29 follow-up framing: *"yeah we want the flexibility to allow various recipes, channels, chains of thought, through channels. these personas are designing things, talking in other chats, collaborating, coding, sometimes just learning. They're supposed to be alive, not static, flexible for the future. ... inbox is all sorts of things in a brain. its channels. ... users multitask so do personas."*

That phrasing is the operative one. **Personas multitask** — exactly like a human user who's mid-conversation in chat A, has a code review pending in PR queue, is generating a study plan in academy, has a voice call waiting. Each one is a channel; each channel pops items the persona services; the persona's cognition decides priority + attention + dispatch.

The dispatch loop has to handle ALL the activity domains, not just chat. My `UnsupportedItem` outcome is treating non-chat domains as out-of-scope when they're actually first-class.

**And the channels cross-pollinate.** Joel 2026-05-29: *"these are contexts and they cross polinate."* The persona's chat conversation informs how it shows up in code review. The training corpus from completed academy sessions surfaces as engrams in subsequent recall. LoRA expertise distilled from coding work travels into how the persona talks about that code. Channels aren't isolated queues — they're contexts sharing the same per-persona cognition.

Architecturally that means: per-domain ACTIVITY HANDLERS dispatch the per-domain WORK, but they all read and write the SAME per-persona `PersonaCognition` (already shared via `channel_state.personas`). The handler isolation is for routing; the context unity is for memory + learning. The cross-pollination is implicit — `ChatHandler` admits an engram via `cognition.admission`; later `CodeHandler` recalls it via `cognition.admission.recall_recent` because they share the same `PersonaCognition` instance. Genome / LoRA expertise updates from any domain become available to any other domain through the same shared state.

So the synthesis doesn't need new cross-pollination machinery — it just needs to keep the per-persona cognition as the shared context spine that ALL handlers read/write. My initial design already does this (shared `Arc<PersonaCognition>` per persona, supplied to all dispatch paths). The thing I missed is the multi-handler routing on top.

**Hard problem flag (not solved in this slice):** Joel 2026-05-29: *"if i chatted with someone they know about it in a live chat or in a game ... or while coding ... this is sort of hard to manage in rag."* The cross-pollination is exactly what the user EXPECTS — Joel mentions Tron in chat-A, then opens a coding session about webgl, the persona surfaces the Tron context because it's relevant. That requires RAG retrieval policy that knows what's relevant *across* domains, not just within one.

The architecture this synthesis lands gives us the substrate (shared per-persona cognition, shared admission state, shared recall surface). The RAG retrieval policy that decides "this chat memory is relevant to this code session" is a separate concern — it's about what `cognition.admission.recall_*` returns when called from different contexts. Not solved here; flagging as known hard.

What this synthesis at least guarantees: the chat handler and the code handler share the same admission store + recall surface, so it's *possible* for the retrieval to surface cross-domain memories. Without that substrate, the cross-pollination wouldn't even be possible. With it, it becomes a retrieval-policy problem, not an architecture problem.

My L0-2-respond-call code:

```rust
if item_type != "chat" {
    return Ok(ServicePopDecision::UnsupportedItem { item_type });
}
```

`service_cycle` has already POPPED the item from the channel queue by the time the type check runs. Discarding it without a handler is silent drop dressed as observability. Under the "channels are the persona's brain" framing, dropping a voice frame / task / code-edit item is dropping a thought.

The fix isn't "don't pop yet" — `service_cycle` is the canonical pop. The fix is **dispatch handlers per activity domain**:

```rust
trait ActivityHandler: Send + Sync {
    fn activity_domain(&self) -> ActivityDomain;
    async fn handle(&self, persona_id: Uuid, item: ChannelItem) -> Result<HandlerOutcome, String>;
}
```

`PersonaServiceModule` holds a `HashMap<ActivityDomain, Arc<dyn ActivityHandler>>`. `service_once_for` routes the popped item by domain. The chat handler wraps `Responder::respond`. Task handler runs the task executor. Voice handler runs the voice loop. Code handler does code dispatch. Etc.

Recipes register new activity handlers at runtime (no recompile to add a new activity domain). Academy reads `HandlerOutcome::Completed` records into training corpus.

This expands L0-2-cutover scope but it's the right shape. The synthesis becomes:

| Concern | Source of truth |
|---|---|
| Per-persona channel storage (ALL domains) | `channel.rs::ChannelState.registries` |
| Activity dispatch registry | `PersonaServiceModule.handlers: HashMap<ActivityDomain, Arc<dyn ActivityHandler>>` |
| Chat → respond() | `ChatHandler` impl wrapping the existing `Responder` trait |
| Task → executor | `TaskHandler` impl (next slice; PersonaTaskExecutor.ts migration target) |
| Voice → voice loop | `VoiceHandler` impl (later slice) |
| Code, code-review, training, recipe-step, ... | each its own handler, registered by recipes / system at init |

### Revised L0-2-cutover commit plan

- **A — Refactor for ChannelState consumption + ActivityHandler trait.** `EnrolledPersona` slims (drops cognition/channels/state). `PersonaServiceModule.with_responder` extended to `with_handlers` (responder becomes the default chat-handler). `service_once_for` routes by domain. Unsupported items: if no handler is registered for the domain, surface as `Err` so the circuit breaker trips (not silently dropped — the persona's queue is leaking items).
- **B — Production wire (chat only).** Same as before. Chat handler ships; voice/task/etc handlers can be left to surface as `Err` if items arrive on those channels (or stubbed handlers that log + re-queue, defer-not-drop). TS PersonaAutonomousLoop still runs in parallel.
- **C — Atomic TS deletion.** Same as before. By this point, chat works end-to-end through Rust. Non-chat channels still have placeholder behavior; their handlers ship in subsequent slices that aren't part of L0-2-cutover.
- **D+ (later) — Per-domain handler slices.** Each new handler (task, voice, code, ...) is its own migration slice. TaskHandler maps to PersonaTaskExecutor.ts deletion. VoiceHandler to whatever the voice TS surface is. Etc.

This frames L0-2-cutover as "wire the dispatch shape AND ship chat end-to-end," not "delete the TS loop and pray every domain works." The infinite-recipe / academy-as-training-distiller pattern Joel describes is structurally supported.

## Open question

Whether my `EnrolledPersona.responder_config` should live as a sibling field on `channel_state` (i.e. extend `ChannelState` with the config) OR stay separate in my service module. Arguments either way:

- **Sibling on ChannelState**: only one map of per-persona stuff. Cleaner mental model. But it means `channel.rs` (which today doesn't care about response config) gets coupled to responder concerns.
- **Separate in PersonaServiceModule**: keeps producer (channel) concerns separate from consumer (responder) concerns. Two maps, but each has a clear owner. My current direction.

Slight lean toward keeping separate. Worth your call though.

## What I'm asking for

A go/no-go on the synthesis. If yes, I'll execute commits A → B → C with verification between each.

If you'd rather see a different shape — e.g. retire `channel.rs::ChannelState` in favor of mine, or migrate `cognition.rs::persona/turn-execute` to use `ChannelRegistry` first — say which and I'll re-card.
