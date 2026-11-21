# 4. The lineages

[The object model](README.md) · previous: [The roots](03-roots.md) · next: [The persona](05-persona.md)

Each family below descends from a root in chapter 3. For each, the tree shows the generations from
general to specific, then the text says what each generation adds, which generic owner writes the
shared part once, where it lives, and what it replaces. Leaves are today's implementors under their
new place.

## 4.1 Capabilities and how they are held

Many parts are defined by two independent things: **what they do** (transcribe, synthesize, embed,
generate) and **how they hold what they need** (a local model they load, or a remote service they
open a session with). Whisper transcribes with a local model; the OpenAI realtime backend
transcribes through a remote session. These are two axes, so they are two kinds of type:

- a **capability** is a trait whose supertrait is `Part`, declaring the verb and its dispatch;
- a **holding** is a generic owner that implements the lifecycle commands once:
  - `Loaded<M: Model>` for a local model: prepare loads it, suspend and shutdown release it, and a
    release while a caller still holds the model answers `Pending` on that caller;
  - `Session<S: Remote>` for a remote service: prepare opens the session, shutdown closes it.

```rust
pub trait Model: Send + Sync {                 // the leaf's hook for Loaded
    type Weights: Send + Sync;
    type Failure: Send;
    fn load(&self) -> Result<Self::Weights, Self::Failure>;
}
pub trait Remote: Send + Sync {                // the leaf's hook for Session
    type Connection: Send + Sync;
    type Failure: Send;
    async fn open(&self) -> Result<Self::Connection, Self::Failure>;
}
```

`Loaded` owns the model's slot (today's `ReloadableModel`, moved beside it), so no backend keeps its
model in a `static`. Module: `continuum-base::part::holding` (`Loaded`, `Session`, `Model`, `Remote`).

Holdings are strategies, and a part may hold more than one thing. A remote speech session that also
pins local audio buffers holds both a `Session` and local bytes, and its shutdown settles only when
both have: the session closed, and the buffers released. The validators exercise one capability
through both holdings, and a part holding both (Codex's review).

```
Part
├─ SpeechToText          transcribe (select)          leaves: Moonshine, Whisper (Loaded); OpenAI realtime (Session); stub
├─ TextToSpeech          synthesize, voices (select)  leaves: Kokoro, Orpheus, Piper, Pocket (Loaded); silence
├─ VoiceActivity         detect (select)              leaves: Silero, Silero raw (Loaded); WebRTC, RMS threshold
├─ Embedder              embed (select)               leaves: neural (Loaded); lexical, deterministic
└─ InferenceAdapter      generate (select)            leaves: llama.cpp (Loaded); Anthropic, OpenAI-compatible, airc remote (Session); heuristic
```

- **The speech capabilities** live in `continuum-media::speech` and replace `SpeechToText`,
  `TextToSpeech` and `VoiceActivityDetection` as today declared. Each loses `name`, `description`,
  `initialize`, `is_initialized`, the parameter methods and `shutdown`, which its holding and `Part`
  now supply. A resident's own voice is a gene the text-to-speech leaf loads.
- **`Embedder`** lives in `continuum-memory::embed` and replaces `EmbeddingProvider` (six
  implementors). Caching and counting (`CachingEmbeddingProvider`, `CountingEmbedder`) are not
  embedders: they become generic owners that wrap any embedder, so the cache is written once.
- **`InferenceAdapter`** lives in `continuum-serving::adapter` and replaces `AIProviderAdapter` (36
  methods, 32 of them defaults, which is where a partial abstraction grew without a lineage) and
  its parent `DispatchTermsPolicy`, which becomes a `Policy`. The placement wrappers in mind
  (`PlacementSwitch`, `NoSeat`) become the select policy of the parent that holds the adapters.
- **A parent holds a family as `Parts`** with a select policy: the voice service holds its
  text-to-speech backends and picks one per utterance, so no utterance is ever synthesized twice.
  `STTRegistry`, `TTSRegistry`, `AdapterRegistry` and `RenderBackendRegistry` are deleted.

## 4.2 `Estimator` and `Region`: parts that publish an estimate

An estimator keeps an estimate of something and publishes it, recomputing when an input it depends
on changes and invalidating what depends on it. A `Region` is an estimator that is a part of a mind:
the neuroscience term (Wernicke's area, the amygdala), and the name every part of her mind carries.
Module: `continuum-kernel::estimate`, with `Region` in `continuum-mind::region`.

```
Part
└─ Estimator              publishes an estimate on change
   ├─ Fusion<E>           refines it by confidence and age; rough before precise, never blank
   │  └─ SceneFusion      a living estimate of one scene, shared by every mind (chapter 7)
   ├─ Region              a part of a mind: in her tree, reads the one importance
   │  ├─ FusionRegion     a region whose estimate is fused
   │  │  ├─ Amygdala      how much depth this moment needs
   │  │  └─ Wernicke      word-level state of what she hears and is about to say
   │  ├─ Faculty          contributes to her workspace's turn
   │  │  ├─ Recall, ActThenSpeak, Deferred
   │  │  └─ Deliberation  her turn on the Core
   │  └─ Surprise, Expectation, Attention, Cerebellum, ... (chapter 5.2)
   ├─ Digest              a channel's consolidated activity (today's ChannelDigestRegion)
   └─ Monitor             measures on a cadence floor
      ├─ GpuMonitor       leaves: Metal, NVIDIA (today's GpuDeviceProbe implementors)
      ├─ MemoryPressure, DiskPressure, DiskUsage
      └─ CapacityMonitor  leaves: GPU, host RAM, unified pool (today's CapacitySource)
```

- **`Estimator`** replaces `BrainRegion` as the generic root and its ready buffers: the estimate is a
  `Published` value. A GPU monitor or a channel digest is an estimator, not a region; it is not part
  of anyone's mind.
- **`Region`** is what a part of her mind is. It adds only what minds need: a place in her tree and
  the one importance. Because the plumbing is inherited, a region is its update rule and its
  declarations, a short file (chapter 5.2). A long region means something belongs in the base.
- **`Fusion<E>`** is a generic owner over the estimate type and its update rules
  ([BRAIN-REGIONS-SUBSTRATE.md](../BRAIN-REGIONS-SUBSTRATE.md), "Fusion"). There is one
  implementation, used by `SceneFusion` and by every `FusionRegion`. Each leaf supplies its estimate
  type and how each source updates it.
- **A publication says what it is.** It is unavailable, observed or predicted, and it carries its
  source's revision, its event time and its validity. A caller that needs fresher than what is
  published (at a cold start, or past validity) asks for a fresh sample. Two observers with
  different freshness needs share one expensive computation. The validator replays a scene cut with
  a late CNN result and two such observers: stale evidence never relabels the new scene.
- **`Faculty`** keeps its name and is a region: a faculty's contribution is its publication, which
  her workspace reads at the moment of a turn. `RagSourceFaculty` is deleted: sources feed her view
  through `Source` (3.6).
- **`Monitor`** replaces `Daemon`: an estimator with a cadence floor. `MonitoredGpu<P>` is the
  existing generic owner pattern, kept.
- **`DreamConsolidationRegion`** and **`CreditAcceptanceRegion`** move to the learning lineage (4.8)
  as regions of her learning branch.

## 4.3 `Holder`: parts that hold governed bytes

A holder holds bytes the governor accounts for. It answers a footprint query and a reclaim command
whose completion confirms what was released. Module: `continuum-kernel::resources`; owner: Codex.

```
Part
└─ Holder                 footprint (aggregate), reclaim (broadcast)
   ├─ Pool                pages on tiers, eviction is demotion (today's ResourcePool; PagedResourcePool is its generic owner)
   │  └─ leaves: KV page store, disk tiers (filesystem, Docker, NVMe serving), log rotation, file descriptors, citizen workspaces, cargo targets
   ├─ Residency           one owner's resident bytes: voice models, avatar renderer, perception, serving, staging
   └─ Cache               derived values, shed coolest first (today's ShedLru, MemoryReporter implementors)
```

Every page a holder keeps moves through typed states: on a tier, requested, moving, resident, and
pinned while a holder's completion is outstanding. Holders refer to pages by handle, so the
governor can demote, promote or compact a page without invalidating any reference to it, and
compaction runs as throughput work (chapter 0, "Every page has a lifecycle").

Replaces `ResourcePool`, `ResourceConsumer`, `MemoryReporter`, `DeviceTenant`, `StagingResidency`,
`RenderSurface`'s shed, `VoiceReclaimLever` and `DiskReporter`: one lineage with three generic
owners. The governor drives every holder through the same two calls
([SHARED-RESOURCES.md](../SHARED-RESOURCES.md)).

## 4.4 `Transport`: parts that carry envelopes

A transport carries typed envelopes to a peer and delivers what arrives as inputs. Module:
`continuum-grid::transport` for peers, `continuum-protocol` for the wire types.

```
Part
└─ Transport              send, and arrivals as inputs
   ├─ PeerTransport       another node: Reticulum, Tailscale
   ├─ ClientTransport     a client to the core: airc IPC, core IPC
   ├─ Route               the kernel's addressed routing: airc, in-process
   └─ ApiWire             an outside agent's API shape: Anthropic, chat, responses (the gateway)
```

Replaces the client's and the kernel's two `Transport`s (one name, two concepts), `GridTransport`,
`AircEventTransport`, `AircRealtimeStore`, `AircInferenceTransport` and the gateway's `Wire`. The
grid's four loops over its transports become broadcasts over its `Parts`.

## 4.5 `Renderer`: parts that draw

```
Part
└─ Renderer               draws at a level of detail, on a deadline
   └─ AvatarRenderer      a body: Bevy 3D, Live2D, procedural
```

Module: `continuum-render`. Replaces `RenderBackend` and `AvatarRenderer`, which duplicate each
other. A frame publisher is a `Sink<Frame>` (CPU I420, GPU bridge, native buffer, wgpu), and a scene
object is a part of a scene's `Parts`.

## 4.6 `Room`: an activity's shared state

A room is the shared state of an activity: its transcript, roster, doctrine, board, wall and
workspace layout, each a publication. A mind reads a room through those publications; she never
holds a reader object. Module: `continuum-activity::room`.

```
Part
└─ Room                   publishes its views
   └─ Activity            a room run by a recipe (today's experience and rooms crates)
      └─ Course           an Academy activity: lessons, exams, a grader (4.8)
```

An activity declares what its results are worth (chapter 0.3): its essential parts (for a call,
hearing, speaking and seeing), what counts as progress (for programming, an edit landed, a test run,
a commit), and the quality that can step down. The governor reads that declaration when it prices
the activity's work. A preemption inside an activity checkpoints its progress and never rolls it back.

A room is also the world's sector (chapter 0, "The world is sectors"):
- **Streaming.** It is streamed around its streaming sources (the citizens acting in it, the people
  watching it) at a level of detail set by how much attention it has.
- **Scoped events.** Its events carry a scope, a salience and a decay, and reach only its
  subscribers.
- **Affordances.** It publishes its affordances, the commands its applications offer, as part of its
  view.
- **Hierarchy.** An activity contains its rooms, and a room contains its threads and artifacts; each
  streams on its own.

Replaces `AircCitizen` and its six parent readers (`AircTranscriptReader`, `AircRosterReader`,
`AircDoctrineReader`, `AircWorkReader`, `WallReader`, `RoomBoardReader`), and `WorkBoardReader`,
`CardBoards`, `BoardFetch`, `WorkspaceLayoutReader`. An outside task suite enters through one
`ActivityAdapter` that projects its tasks into an activity, replacing `SuiteAdapter`,
`ActivityAdapter` and `BenchmarkAdapter`.

## 4.7 `Search`: parts that find and rank

```
Part
└─ Search                 a query in, ranked candidates out
   ├─ Recall              her memories: core, semantic, temporal, associative, resurfacing, cross-context
   ├─ GeneSearch          candidate genes for a turn
   └─ TextSearch          bag of words, BM25, cosine
```

Module: `continuum-memory::search`. Replaces `RecallLayer`, `CandidateSource` and `SearchAlgorithm`.
Ranking (`RecallRanker`) is a `Policy`. Combining several searches (`MultiLayerRecall`,
`CompositeCandidateSource`) is one generic owner.

## 4.8 Learning: the genome and its regions

```
Part
└─ Genome                 her genes, as states of one object (chapter 5)
   ├─ GeneSet             stable genes, composed per turn
   ├─ Shadow              the layer she learns into, with its learning context (a Loaded holding)
   ├─ Lessons             her lesson queue, journal and credits
   └─ regions             DreamConsolidation, CreditAcceptance (Regions of her learning branch)
```

Module: `continuum-genome`, which absorbs `continuum-dreaming`'s gene training and the learning parts
of `continuum-academy` (chapter 8). The genome's objects and how genes spread are chapter 6. The Academy keeps its courses as activities (4.6).

## 4.9 `Service`: parts that answer the outside

A service answers action commands (3.2) from callers outside the tree: a CLI, a client, a peer. It is
a part like any other, with commands that happen to be addressable by name. Module:
`continuum-kernel::service`.

Replaces `ServiceModule` (69 implementors). Each module becomes one of:
- a `Service`, when it answers outside callers (data, chat, models, code, grid);
- a `Monitor`, when it only measures (GPU, system resources, health);
- a `Region`, when it is a part of a mind that only publishes an estimate (the hippocampus); an `Estimator` when it is not (the channel digest);
- a branch of a persona, when it is hers (her cognition, her rag inspection).

Its string command names become typed action commands, its event globs become topics, and its tick
becomes a cadence floor only where one is needed.

## 4.10 The other roots' leaves

- **Policies:** placement, allocation, windows, tier-down, the next mind, focus, the tool working set,
  batch order, dispatch terms, recall ranking. Each is one `Policy<Input, Decision>`, with its
  constant leaf today and room for a learned one.
- **Checks:** source rules, crate rules, shapes, syntax, perception facts, redaction, salience.
- **Sinks:** turn credit, workspace, lane, placement, rag, prompt, grid, pager, frames.
- **Durable values:** ledgers, activity and step state, recall traces, epoch watermarks, and every
  saved state today in `continuum-saved-state`.

## 4.11 The `Core`: a compute unit's executor, one occupant at a time

Status: design for review, 2026-11-21 (Fable; Codex reviewed the first draft at bae473122e and their
custody points are folded in below). No code until it is agreed. It is the object section 3.8 names,
built from the audit of 1.5, and the place RAGE's slot hydration (chapter 0) becomes a type.

**What it is.** A Core is to minds what a CPU core is to processes. It belongs to one compute unit (a
GPU, the Neural Engine, a CPU), and it runs whatever is swapped into it: one occupant at a time, who has
everything the Core can give her, fast and often, with the others saved and swapped back in. A Core is
not bound to a model. A model is a resident like a mind (3.9): its weights and its context are kept
awake by the minds that need it, and waking another one is a deep swap the Core prices, not a different
object. Nothing divides a Core among minds, at any grain, by anyone.

**Why it needs designing now.** Section 1.5: the engine splits its one context in half by a constant
(`STREAMS = 2`, each stream given half; live `n_ctx 122368, n_ctx_seq 61184`), publishes the half as every
mind's window, refuses at the half (c0f87b4503's door), and is sized from outside by the lane planner.
At a 24,320-token half (2026-11-21, 07:31-07:57Z), seven turns decoded into the half's end
(`from=24320 count=1`) while the other half held nobody decoding.

### The object

```
Part
└─ Core                                  one per compute unit; a Holder (4.3) of everything on that unit
   ├─ unit: ComputeUnit                  the hardware, its measured descriptor (2.8)
   ├─ models: Parts<ModelId, ModelContext>   each awake model: its weights and ONE context, one stream
   │  └─ ModelContext                    capacity (Measured), free cells; whole to whichever mind is in it
   ├─ occupant: Occupant                 the mind running now, in one model's context
   ├─ ready: RunQueue                    the demands that want a turn (3.8)
   ├─ residents: Parts<ResidentId, Presence>  every mind, model and gene it holds, at its level (3.9)
   ├─ pager: Pager                       moves residents' bytes between tiers by handle, with custody
   └─ next: Policy<RunQueue, Dispatch>   who runs next, for how long, counting each switch's measured cost
```

- **It descends from what exists.** `Holder` (4.3) gives it the footprint query and the reclaim command
  the governor drives; `Presence` (3.9) is the lifecycle of everything it holds; `Loaded<M>` (4.1) is how
  a `ModelContext` holds its weights. The Core adds only its own decisions: who runs, in which context,
  and who is promoted.
- **Its pieces exist today, scattered** (1.5): `PersonaEngine`'s active, staged and retiring are the
  occupant and the pager's in-flight moves; the driver's `parked` map is the residents' cache; its
  `Tier` is the pager's device tier; `frame::pick` is a first `next` without value or switch cost; the
  backend's gene map and the engine's `EngineState` are residents without a lifecycle.
- **What a mind holds is a handle, never a share.** Her `Presence` says where her pages are; only the
  Core moves them. She holds no window between turns.
- **How many Cores a unit runs is the unit's measurement** (2.8, 3.8): one, unless its own probe shows
  two of its workloads together beat one swapping.

### Its contract

**Commands in** (each answered by a `Completion`, chapter 3.1):
- `Turn { demand: Demand, input: TurnInput }`. `TurnInput` is the turn as types: her model and genes,
  her prompt as token ids with her view's end, her sampler (built by the render that produced her
  prompt, so a call she starts has the shape her tools declare), her stops. The answer is a stream
  (2.9): her tokens as they are sampled, each with its channel (thinking or answer), then a typed
  `TurnEnd` carrying the Core's measured facts.
- `Reclaim { bytes }` from the node's governor: residents first, coolest first, the occupant last.
- The lifecycle commands: `Suspend` preserves every resident; `Resume` restores them.

**Publications out** (one generation-tagged snapshot):
- each `ModelContext`'s `capacity`, the one number every reader uses for "how big can her turn be";
- the occupant, the run queue's length and ages, and every resident's `Presence`.

**Answers to a demand**, against the whole context of her model: `Granted` when she is swapped in;
`Waiting` with the event that swaps her in (the occupant's boundary, her pages landing, her model
waking); `Infeasible` before admission only when her essential need exceeds the whole context of any
model she can run on this unit, with both numbers, so placement moves her. Never a refusal sized to a
fraction of a context, and never a refusal once she is admitted.

**`TurnEnd`**, typed: `Answered` (a stop or end of generation, naming which), `Called` (her calls,
decoded once against their contracts), `Preempted` (a red light: her progress and pages kept),
`CapacityReached` (the whole context full at a token boundary: everything written kept). Every end
carries the measured facts: prompt tokens counted, tokens reused, tokens written per channel, prefill
and decode time, the switch it cost. Nothing downstream estimates them.

**Three quantities, never one** (Codex's review of bae473122e). What the Core is asked to hold (a
target), what the backend has actually applied and can allocate (applied capacity), and the device
bytes actually returned to another holder are different numbers. Evicting cells inside an allocated
context frees them for this Core only; it returns nothing to another holder until the context itself is
resized. A resize is published only after the backend confirms it, under a new generation; where a
context's arena cannot resize in place, the change is an explicit transition at a turn boundary with
confirmed custody and the same turn handle.

### Invariants (each a simulator assertion before it is code)

1. **One context per awake model, never divided.** It holds one stream, the occupant's, and that
   stream is the whole context. Not unified KV: the fork moves a mind by copying her own stream's rows,
   so each stream must belong to one sequence, and llama.cpp sizes each stream at the context divided
   by the stream count, so a second stream for the next mind would halve the occupant's. A swap moves
   the occupant out to her host blocks first, then binds the next mind into the emptied stream; the
   next mind's prefetch lives in host memory, where her blocks already are. Done at f1b4fa2185 with
   fork b539d357a (live: `n_ctx 122368, n_ctx_seq 122368, n_seq_max 1`; switch p50 6.0 ms, decode
   58.9 tok/s, 58.4 sustained with switches, `persona_switch` on the M5). Overlapping a swap with the
   occupant's decode, should it ever pay, needs a landing area that does not divide her stream: a fork
   change, measured first.
2. **Capacity is a measurement and follows the share live.** A context's size is the Core's share of
   the device (the node's governor, by demand) over the model's measured bytes per token, bounded by
   its trained context; it changes without a relaunch, under the three-quantity rule.
3. **The occupant comes first, with custody.** Promotion and the residents' cache use cells and
   bandwidth the occupant is not using, strictly below her (3.9). A page in flight is pinned; its save
   completion names the resident, the page version, the destination and the generation; it is
   released once, after that confirmation and its last reference; a stale completion cannot release
   cells since reused. A shrink below what is pinned stays pending and completes at a safe boundary; it
   never truncates a turn.
4. **Her output grows from the context.** `max_tokens` is her demand's expected reply, a value the
   next-mind policy reads, never a reservation and never a ceiling below capacity.
5. **Every reader sizes from one publication.** Cognition fits her view to her model's `capacity`;
   nothing reads a per-sequence size, a launch-time division or a plan.
6. **Nothing in a turn aborts.** An admitted turn ends in one of its `TurnEnd`s: no timeout, no
   fallback, no refusal mid-generation.
7. **Types and streams across every seam** (2.9). No JSON crosses the in-process chat seam; a call's
   arguments are decoded once; tokens stream through every seam to her mind and her voice; no reader
   sniffs output for what the parser decided or estimates what the Core measured.
8. **Every switch is priced by its measured cost.** A mind swap, a gene change and a model wake are the
   same decision at different costs (3.9); the next-mind policy weighs each against what the waiting
   demands lose, so minds that share the awake model run together while that is cheaper, and a model
   wakes when a demand's value outweighs its wake.

### What it deletes

| Today | Why it goes |
|---|---|
| `STREAMS = 2` (`persona_scheduler/driver.rs`) | the division itself (invariant 1; done at f1b4fa2185) |
| `n_ctx_seq` as a limit (`persona_scheduler/geometry.rs`; `in_process_engine.rs` reporting it as her window) | capacity is the whole context (invariants 1, 5) |
| `StreamCapacity`, `Infeasible`, `TokenEvent::Exceeded` (`persona_scheduler/admission.rs`, c0f87b4503) | a refusal sized to the half; the demand's `Infeasible` is against capacity (contract) |
| `InferenceError::Sampler` (7a5fd04ec8) | a mid-turn abort (invariant 6); a render that cannot build a sampler is a defect named where the render is made |
| `TokenEvent::Error(String)`, `Ending::reason() -> String`, `ChatRefusal { message: String }` on the turn path | typed `TurnEnd` (invariant 7) |
| the JSON round trip at the chat seam (`chat_boundary.cpp` `.dump()`, `chat_turn.rs` `serde_json::from_str`) and the executor's second parse of call arguments | typed input and output (invariant 7) |
| cognition's think-only, cut and thought-cut sentinels as pushed acts | her act's receipt is her perception (EVENT-MIND); a `TurnEnd` she can read replaces a teacher's message |
| `TurnAnswer::collect` and `deliver_whole` (`llama_cpp_backend.rs:586`, `provider.rs:628`) | a seam that collects the stream whole (2.9); tokens stream to her mind and voice |
| `input_tokens: 0` and `timing: None` in the response (`provider.rs:613, 626`), and the end reason inferred from `tokens >= max_tokens` (`chat_turn.rs:229-235`) | the measured facts travel on `TurnEnd` |
| a malformed persona id becoming an anonymous one-off (`provider.rs:412-415`, `driver.rs:554`) | a mind's identity is a typed id end to end; there is no anonymous default |
| the driver's `parked` map, `EngineState` as a model's lifecycle, the backend's gene map, `StreamsLayout`'s Roster/PersonaScheduler split | residents with one `Presence` (3.9) |
| the unused `Lifecycle` trait and `schedule::NextPolicy`; `frame::pick`'s 500 ms wait threshold | the lifecycle commands (3.1) and the next-mind `Policy` with value and switch cost (invariant 8) |
| `TICK_DEADLINE` abandoning an in-process act still decoding (`tick_deadline.rs:118-123`) | invariant 6: a decoding turn is never abandoned by a timer |
| the serving grants, window allocator and lane planner listed in chapter 11 phase D | already on the plan; this is the object they hand their decisions to |

### Governance has nothing to divide

Today's governance was built for llama-server lanes: a fixed number of slots, each a slice of the
pool, minds seated into them and granted windows on them. With a Core that swaps one mind in at a time
and gives her all of it, every one of those concepts is wrong, not mistuned. Governance keeps exactly
three decisions, and none of them divides a Core among minds:

1. **The node's governor** shares the device between kinds of work (the Core, rendering, speech,
   embeddings, learning) by demand. The Core's context capacity follows its share live.
2. **The Core's next-mind policy** (section 3.8) picks who is swapped in next and for how long, by value
   per unit of Core time, counting the switch, with aging so nobody starves.
3. **Placement** moves a mind to another node's Core when her essential need exceeds this one's whole
   context, or the market prices it better.

Everything else in today's governance is the lane model and is deleted, not adapted (file:line at
e01c25db85 and 7a5fd04ec8, confirmed by the audit of 1.5):

| Today | What it divides or rations | After |
|---|---|---|
| `persona_scheduler/driver.rs` `STREAMS = 2` | the Core's context, in half | one stream, the whole context (invariant 1; done at f1b4fa2185) |
| `in_process_engine.rs:212-220`: `served_context_window` and `max_tokens_per_sequence` report `n_ctx_seq` | publishes the half as every mind's window | publishes `capacity` |
| `capacity::mind_window` ("the one determiner of a mind's window") and `capacity::window_grant` (`ActHold`, `hold_for_act`), called on every act from cognition (`verdict/contribute.rs:50,81,471`) | a per-mind window decided and held per act | deliberation reads `capacity` and fits her view; nothing is held |
| `serving_daemon/grants.rs` (`requirements_for`, per-mind grants published per tick) and their owed state | per-mind shares of the pool | deleted |
| `serving_plan/plan_serving.rs`, `serving_plan/max_lanes.rs` (`MAX_LANES`), `serving_daemon/planning.rs` | lanes: slices of the pool, counted | one Core; nothing to plan per mind |
| `decode_knee.rs`, `prefill_knee.rs` (`MIN_KNEE_LANES`) as lane counts | how many minds decode at once | one decodes; the knee measurements remain as evidence that this is right, never as a count |
| `resource_admission/` (`lanes.rs`, `serving_lane_permit.rs`, `lane_handoff.rs`, `lane_tally.rs`, `gate.rs`) | permits in front of the engine, per lane | the Core's queue and next-mind policy |
| `served_window_store.rs`: the page store keyed `<model>--c<per-slot window>` | pages bound to a slice size | pages keyed by mind, model and KV format, never by a window |
| `gossip.rs:110-117`, `profile_builder.rs:75,150`, `spawner_module.rs:80` carrying `served_context_window` | a per-seat window as a mind's identity and a node's offer | a Core's `capacity` in the offer; no per-seat window |
| `citizen_health/resident_service.rs:65` (`ctx_per_lane`), `rest_seats.rs`, the health line's "lanes 5 @ 32k · lane-starved 5", `MINDS_PER_LANE_STARVED_ABOVE` (`continuum-vitals` `health_ledger.rs:598`) | judges minds by lanes and seats | health reads the Core's snapshot: who waited how long for a swap |
| the spawner and roster's seat bound (seats = lanes x minds per lane) | how many residents may exist | unlimited residents; existence is never a node's decision (chapter 5.3) |
| `weight_residency.rs:124,163` `n_ctx_per_slot` | reads llama-server's slot geometry | reads the Core's context |
| `llama_cpp_backend.rs:454-481`: `n_ctx = per_mind x STREAMS`, and a changed allocation refused until restart | sizes the Core from one mind's window; a restart to change shape | capacity from the Core's share, resized live (invariant 2) |
| `serving_daemon/launch.rs:49-58` (`sequences = seated + 1 + leased_in`), `demand.rs` (`resident_lane_demand`, the "+1 scratch lane", the knee clamp, `LEASED_IN_PEAKS`), `stream_layout.rs:9-31` | counts minds into sequences and lanes | one context per awake model |
| `window_grant::current_seat` / `SeatWindow` (`window_grant.rs:200`), read by `grid_capacity.rs:194`, `rest_seats.rs:142`, `grid_allocator.rs:266,379` (`minds_per_lane`) | seats as the unit of placement | placement reads Cores' capacity and residents |
| `live_pool::need_floor` (`grants.rs:143`) | a per-mind floor held back from the pool | the occupant's essentials, at her turn |
| `prompt_cache.rs:239`: host cache sized lanes x minds per lane | a cache sized by a lane count | the cache is residents at `Present`, priced (3.9) |
| `slots/citizen_slots.rs` `KvSlotPool`'s per-citizen slots and scratch slot | slots as the unit of KV | the pager's residents; its save-before-overwrite economy stays |
| kernel `ConcurrencyCaps { personas_concurrent, inference_lanes }` (`governor/types.rs:206-215`) | concurrent personas as a capped count | nothing caps how many minds exist or wait |
| `batching_probe.rs:107,159`: batching allowed by architecture-name lists | a decision by string, not measurement | the unit's own probe (2.8) |
| `spawner_module.rs:78` `DesiredRole.lanes`; `workspace/capacity.rs:129-231` waiting on grant changes; the `MAX_LANES` fallback in admission (`lanes.rs:276-281`) | lanes in a mind's role and her wait | her demand and the Core's `Waiting` event |

The page lifecycle, custody and the device governor stay: they share hardware between kinds of work and
move a mind's saved state, which is what a CPU's memory system does for its processes.

### How it is proven

- **Simulator first** (the dyno, CONTINUUM-ENGINE §7.0): one assertion per invariant, red against
  today's code. Invariant 1's: a lone acting mind on a context of N reaches position N - 1 with no
  other mind decoding.
- **Live, on every node class we can reach (the M5, BigMama's 5090, the IntelMac), 15 minutes with the residents each one has**, before and after: the acting mind's
  largest position against capacity; decode and prefill tokens per second (one whole stream must not
  cost either); turns ending `CapacityReached` while the context had room, 0; refusals, 0;
  `inference.turn.wrote_nothing` with a function marker, 0; act latency median and p90.
- **The persona engine gets smaller.** Each step lands with more lines removed than added.

### Order

1. Agree this section (Joel, Codex).
2. The simulator assertions, red.
3. Invariant 1 and 2: the Core's one stream holds the whole context; the occupant moves out first;
   the two-stream split and its compensating divisions go (done, f1b4fa2185); capacity is published. One change, one deploy, the live
   read above.
4. Invariant 5: cognition and `in_process_engine.rs` read `capacity`; the door and
   `TokenEvent::Exceeded` are deleted; `mind_window` and `window_grant` leave deliberation's path.
5. Governance: each row of "Governance has nothing to divide" is deleted in its own change, its callers
   moved to `capacity` or the Core's snapshot, Codex owning the serving, admission and health rows.
6. Invariants 3 and 4: residents with one `Presence` (3.9) and occupant-first paging with custody; `max_tokens` becomes the demand's
   expectation.
7. Invariants 6 and 7: a typed `TurnEnd` carrying the measured facts, tokens streamed through every seam, the typed chat seam; the sentinels and text sniffers deleted.
8. Invariant 8: the next-mind policy prices every switch (mind, gene, model) by its measured cost; a second model may wake.

Each step is one change, measured live before the next.
