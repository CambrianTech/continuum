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

## 4.11 The `Core`: one model, one context, one mind in it at a time

Status: design for review, 2026-11-21 (Fable; Codex reviews custody and the governor's side). No code
until it is agreed. It is the object section 3.8 ("The Core owns its budget") names, and the place
RAGE's slot hydration (chapter 0, "Slots, hydrated from descriptors") becomes a type.

**What it is.** A Core is one model loaded on one compute unit with one context, and the mind
swapped into it has the whole of that context. It is RAGE's active slot: a resident entering it is
hydrated from her descriptor (her pages, her genes, her sampler), acts, and steps out as cache. Only
one mind decodes on a Core at a time (section 3.8), so the Core's memory is never divided among
minds, at any grain, by anyone.

**Why it needs designing now (receipts, the M5).**
- The persona engine opens its context with a constant two sequences, "the active mind's and the
  next one's" (`persona_scheduler/driver.rs` `STREAMS`), without a unified KV buffer, so llama.cpp
  divides the context in half per sequence. Live at 7a5fd04ec8 (2026-11-21 15:44:35Z):
  `n_ctx 122368, n_ctx_seq 61184, n_seq_max 2`. The mind acting can never use more than half of a
  context only she is decoding in.
- At 24,320 per half (07:31-07:57Z), seven turns decoded into the half's end (`from=24320 count=1`)
  and died mid-answer while the other half held nobody decoding.
- The answer written that morning (c0f87b4503) refused turns at a door sized to the half
  (`StreamCapacity`, `Infeasible`, `TokenEvent::Exceeded`): a refusal over the wrong number, where
  the defect was the division. It is deleted by this design, not tuned.

### The object

```
Part
└─ Core<M: Model>                 a Loaded<M> and a Holder: the model's weights and the one context
   ├─ context: Context            the KV pool, unified: capacity and free cells as the engine measures them
   ├─ occupant: Occupant          the mind decoding now; her stream may span the whole context
   ├─ next: Staging               the next mind's restore or prefill, in cells the occupant is not using
   ├─ cache: Residents            pages of minds recently on the Core, warm while room remains
   ├─ pager: Pager                moves pages between tiers by handle (the persona engine's `Tier`)
   └─ next_mind: Policy<Ready, Next>   who goes next, and for how long (section 3.8's rule)
```

- **It descends from what exists.** `Loaded<M>` (4.1) gives it the model's lifecycle; `Holder` (4.3)
  gives it the footprint query and the reclaim command the governor drives. The Core adds only what
  is its own: the context, who is in it, and who is next.
- **Its parts already exist in pieces.** The persona engine's `PersonaEngine` holds `active`,
  `staged` and `retiring`, and its `Tier` moves blocks: that is occupant, next, cache and pager. The
  scheduler's frame pick is the next-mind policy. This design names them as one object and removes
  what divides it.
- **What a mind holds is a handle, never a share.** Her `Residency` (chapter 5) records where her
  pages are; only the Core moves them. She holds no window between turns.

### Its contract

**Commands in** (each answered by a `Completion`, chapter 3.1):
- `Turn { demand: Demand, input: TurnInput }`, where `TurnInput` is everything the turn is made of,
  as types: her prompt as token ids with her view's end, her sampler (built by the render that
  produced her prompt, so a call she starts has the shape her tools declare), her stops, her genes.
  The answer streams her tokens and ends with a typed `TurnEnd` (below).
- `Reclaim { bytes }` from the node's governor (it is a `Holder`): cache first, the occupant last.
- The lifecycle commands: `Suspend` preserves every resident; `Resume` restores them.

**Publications out** (one generation-tagged snapshot, chapter 3.8 "One publication"):
- `capacity`: the context's measured size in tokens and bytes, the one number every reader uses for
  "how big can her turn be";
- `occupant`, `next`, and each cached resident's residency (device, RAM, disk);
- per turn: her tokens, and her `TurnEnd`.

**Answers to a demand** are section 3.8's, against the whole context: `Granted` when it is her turn;
`Waiting` with the event that wakes her (the occupant's boundary, a page landing); `Infeasible`
before admission only when her essential need (what her newest turn cannot go below) exceeds the
Core's capacity, with both numbers, so placement can move her to a larger Core. Never a refusal
sized to a fraction of the context, and never a refusal once she is admitted.

**`TurnEnd`**, typed, so nothing downstream reads text to learn what happened: `Answered` (a stop,
end of generation), `Called` (her tool calls, decoded against their contracts), `Preempted` (a red
light: her progress and pages kept, she resumes), `CapacityReached` (the whole context is full at a
token boundary: her written tokens kept, her next turn's view collapses oldest first).

### Invariants (each a simulator assertion before it is code)

1. **One context, never divided.** The context opens with a unified KV buffer: every sequence
   addresses every cell. The number of sequences is how many minds can be bound at once (the
   occupant and the one being staged), never a division of the cells. No constant chooses it; the
   occupant's stream may reach the context's full capacity.
2. **Capacity is a measurement.** The context's size is the Core's share of the device (the node's
   governor, by demand) divided by the model's measured bytes per token, bounded by the model's
   trained context. It changes live with the share, never by relaunch.
3. **The occupant comes first.** Staging and cache use cells the occupant is not using. When she
   needs more, the coolest cached resident pages out, then the staged one; nothing takes her cells.
   A page leaves the device only when its save is confirmed (custody, section 3.8).
4. **Her output grows from the context.** `max_tokens` is her demand's expected reply, a value for
   the next-mind policy, not a reservation and not a ceiling below capacity. She writes while cells
   exist anywhere in the context.
5. **Every reader sizes from one publication.** Cognition fits her view to `capacity`; nothing reads
   a per-sequence size, a launch-time division or a cached plan. Her view collapses oldest first only
   when the whole context cannot hold her need (section 3.8, "Her context budget is her focus").
6. **Nothing in a turn aborts.** An admitted turn ends in one of its `TurnEnd`s. No timeout, no
   fallback, no refusal mid-generation; a condition the Core cannot serve now is a `Waiting` with its
   event.
7. **Types across every seam.** The render hands the Core typed input and receives typed output;
   the Core reports typed ends. No JSON crosses the in-process chat seam, no call's arguments travel
   as text to be parsed twice, and no reader sniffs raw output for what the parser already decided
   (`chat_turn.rs` `rsplit_once("<function=")`, cognition's `parse_tool_calls(reasoning)`).

### What it deletes

| Today | Why it goes |
|---|---|
| `STREAMS` and the non-unified context (`persona_scheduler/driver.rs`) | the division itself (invariant 1) |
| `n_ctx_seq` as a limit (`persona_scheduler/geometry.rs`; `in_process_engine.rs` reporting it as her window) | capacity is the whole context (invariants 1, 5) |
| `StreamCapacity`, `Infeasible`, `TokenEvent::Exceeded` (`persona_scheduler/admission.rs`, c0f87b4503) | a refusal sized to the half; the demand's `Infeasible` is against capacity (contract) |
| `InferenceError::Sampler` (7a5fd04ec8) | a mid-turn abort (invariant 6); a render that cannot build a sampler is a defect named where the render is made |
| `TokenEvent::Error(String)`, `Ending::reason() -> String`, `ChatRefusal { message: String }` on the turn path | typed `TurnEnd` (invariant 7) |
| the JSON round trip at the chat seam (`chat_boundary.cpp` `.dump()`, `chat_turn.rs` `serde_json::from_str`) and the executor's second parse of call arguments | typed input and output (invariant 7) |
| cognition's think-only, cut and thought-cut sentinels as pushed acts | her act's receipt is her perception (EVENT-MIND); a `TurnEnd` she can read replaces a teacher's message |
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
e01c25db85):

| Today | What it divides or rations | After |
|---|---|---|
| `persona_scheduler/driver.rs` `STREAMS`, context opened without `kv_unified` | the Core's context, in half | one unified context (invariant 1) |
| `in_process_engine.rs:212-220`: `served_context_window` and `max_tokens_per_sequence` report `n_ctx_seq` | publishes the half as every mind's window | publishes `capacity` |
| `capacity::mind_window` ("the one determiner of a mind's window") and `capacity::window_grant` (`ActHold`, `hold_for_act`), called on every act from cognition (`verdict/contribute.rs:50,81,471`) | a per-mind window decided and held per act | deliberation reads `capacity` and fits her view; nothing is held |
| `serving_daemon/grants.rs` (`requirements_for`, per-mind grants published per tick) and their owed state | per-mind shares of the pool | deleted |
| `serving_plan/plan_serving.rs`, `serving_plan/max_lanes.rs` (`MAX_LANES`), `serving_daemon/planning.rs` | lanes: slices of the pool, counted | one Core; nothing to plan per mind |
| `decode_knee.rs`, `prefill_knee.rs` (`MIN_KNEE_LANES`) as lane counts | how many minds decode at once | one decodes; the knee measurements remain as evidence that this is right, never as a count |
| `resource_admission/` (`lanes.rs`, `serving_lane_permit.rs`, `lane_handoff.rs`, `lane_tally.rs`, `gate.rs`) | permits in front of the engine, per lane | the Core's queue and next-mind policy |
| `served_window_store.rs`: the page store keyed `<model>--c<per-slot window>` | pages bound to a slice size | pages keyed by mind, model and KV format, never by a window |
| `gossip.rs:110-117`, `profile_builder.rs:75,150`, `spawner_module.rs:80` carrying `served_context_window` | a per-seat window as a mind's identity and a node's offer | a Core's `capacity` in the offer; no per-seat window |
| `citizen_health/resident_service.rs:65` (`ctx_per_lane`), `rest_seats.rs`, the health line's "lanes 5 @ 32k · lane-starved 5", `MINDS_PER_LANE_STARVED_ABOVE` | judges minds by lanes and seats | health reads the Core's snapshot: who waited how long for a swap |
| the spawner and roster's seat bound (seats = lanes x minds per lane) | how many residents may exist | unlimited residents; existence is never a node's decision (chapter 5.3) |
| `weight_residency.rs:124,163` `n_ctx_per_slot` | reads llama-server's slot geometry | reads the Core's context |

The page lifecycle, custody and the device governor stay: they share hardware between kinds of work and
move a mind's saved state, which is what a CPU's memory system does for its processes.

### How it is proven

- **Simulator first** (the dyno, CONTINUUM-ENGINE §7.0): one assertion per invariant, red against
  today's code. Invariant 1's: a lone acting mind on a context of N reaches position N - 1 with no
  other mind decoding.
- **Live, on the M5, 15 minutes at four or more residents**, before and after: the acting mind's
  largest position against capacity; decode and prefill tokens per second (a unified buffer must not
  cost either); turns ending `CapacityReached` while the context had room, 0; refusals, 0;
  `inference.turn.wrote_nothing` with a function marker, 0; act latency median and p90.
- **The persona engine gets smaller.** Each step lands with more lines removed than added.

### Order

1. Agree this section (Joel, Codex).
2. The simulator assertions, red.
3. Invariant 1 and 2: the llama binding exposes `kv_unified`; the Core opens one unified context;
   `STREAMS` and `n_ctx_seq` as a limit go; capacity is published. One change, one deploy, the live
   read above.
4. Invariant 5: cognition and `in_process_engine.rs` read `capacity`; the door and
   `TokenEvent::Exceeded` are deleted; `mind_window` and `window_grant` leave deliberation's path.
5. Governance: each row of "Governance has nothing to divide" is deleted in its own change, its callers
   moved to `capacity` or the Core's snapshot, Codex owning the serving, admission and health rows.
6. Invariants 3 and 4: occupant-first paging inside the one context; `max_tokens` becomes the demand's
   expectation.
7. Invariants 6 and 7: typed `TurnEnd`, the typed chat seam, the sentinels and text sniffers deleted.

Each step is one change, measured live before the next.
