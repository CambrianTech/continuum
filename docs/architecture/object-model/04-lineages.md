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

## 4.2 `Region`: parts that publish an estimate

A region keeps an estimate of something and publishes it, recomputing when an input it depends on
changes and invalidating what depends on it. Module: `continuum-kernel::region`.

```
Part
└─ Region                 publishes an estimate on change
   ├─ FusionRegion        refines it by confidence and age; rough before precise, never blank
   │  ├─ Amygdala         how much depth this moment needs
   │  ├─ Wernicke         word-level state of what she hears and is about to say
   │  └─ SceneFusion      a living estimate of one scene
   ├─ Faculty             contributes to her workspace's turn
   │  ├─ Recall, ActThenSpeak, Deferred
   │  └─ Deliberation     her turn on the Core
   ├─ Digest              a channel's consolidated activity (today's ChannelDigestRegion)
   └─ Monitor             measures on a cadence floor
      ├─ GpuMonitor       leaves: Metal, NVIDIA (today's GpuDeviceProbe implementors)
      ├─ MemoryPressure, DiskPressure, DiskUsage
      └─ CapacityMonitor  leaves: GPU, host RAM, unified pool (today's CapacitySource)
```

- **`Region`** replaces `BrainRegion` and its ready buffers: the estimate is a `Published` value.
- **`FusionRegion`** is a generic owner over the estimate type and its update rules
  ([BRAIN-REGIONS-SUBSTRATE.md](../BRAIN-REGIONS-SUBSTRATE.md), "Fusion"). Each leaf supplies its
  estimate type and how each source updates it.
- **`Faculty`** keeps its name and moves under `Region`: a faculty's contribution is its
  publication, which her workspace reads at the moment of a turn. `RagSourceFaculty` is deleted:
  sources feed her view through `Source` (3.6).
- **`Monitor`** replaces `Daemon`: a region with a cadence floor. `MonitoredGpu<P>` is the existing
  generic owner pattern, kept.
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
of `continuum-academy` (chapter 6). The Academy keeps its courses as activities (4.6).

## 4.9 `Service`: parts that answer the outside

A service answers action commands (3.2) from callers outside the tree: a CLI, a client, a peer. It is
a part like any other, with commands that happen to be addressable by name. Module:
`continuum-kernel::service`.

Replaces `ServiceModule` (69 implementors). Each module becomes one of:
- a `Service`, when it answers outside callers (data, chat, models, code, grid);
- a `Monitor`, when it only measures (GPU, system resources, health);
- a `Region`, when it only publishes an estimate (the hippocampus, the channel digest);
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
