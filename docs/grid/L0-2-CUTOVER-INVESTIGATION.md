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

## Open question

Whether my `EnrolledPersona.responder_config` should live as a sibling field on `channel_state` (i.e. extend `ChannelState` with the config) OR stay separate in my service module. Arguments either way:

- **Sibling on ChannelState**: only one map of per-persona stuff. Cleaner mental model. But it means `channel.rs` (which today doesn't care about response config) gets coupled to responder concerns.
- **Separate in PersonaServiceModule**: keeps producer (channel) concerns separate from consumer (responder) concerns. Two maps, but each has a clear owner. My current direction.

Slight lean toward keeping separate. Worth your call though.

## What I'm asking for

A go/no-go on the synthesis. If yes, I'll execute commits A → B → C with verification between each.

If you'd rather see a different shape — e.g. retire `channel.rs::ChannelState` in favor of mine, or migrate `cognition.rs::persona/turn-execute` to use `ChannelRegistry` first — say which and I'll re-card.
