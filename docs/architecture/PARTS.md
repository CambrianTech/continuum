# Parts: the object model

Status: design, revised 2026-11-20 with Codex's review of the names, the dispatch rules and the
command binding. Owners: Fable (the part hierarchy); Codex (resources, completion ownership,
review). The code mirrors this chapter: each named interface below is one type in one module, at
the place this chapter gives it.

[Runtime architecture](../README.md#runtime-architecture) · [How a mind runs](HOW-A-MIND-RUNS.md) ·
[Module architecture](MODULE-ARCHITECTURE.md) · [Shared resources](SHARED-RESOURCES.md)

**Contents**

1. [Summary](#1-summary)
2. [The problem](#2-the-problem)
3. [Goals and non-goals](#3-goals-and-non-goals)
4. [The general: what every part is](#4-the-general-what-every-part-is)
5. [The specific: the lineages](#5-the-specific-the-lineages)
6. [Names](#6-names)
7. [Worked example: a call ends, then memory is needed](#7-worked-example-a-call-ends-then-memory-is-needed)
8. [Alternatives considered](#8-alternatives-considered)
9. [Coverage: every component in the core, placed](#9-coverage-every-component-in-the-core-placed)
10. [The plan to finish](#10-the-plan-to-finish)
11. [How we will know it works](#11-how-we-will-know-it-works)
12. [Open questions](#12-open-questions)

## 1. Summary

Everything that runs in the core is a **part**: a speech model, an inference adapter, a grid
transport, a service, a brain region, a persona and each of her branches. The design is a hierarchy
of named interfaces, from the general to the specific.

- **The general** (section 4) is what every part is. It has an identity and a generation, it is
  made of parts, it lives on a worker, it answers commands with completions and queries with
  published snapshots, and it hands long work off as tasks. All of this is written once.
- **The specific** (section 5) is the lineages. A model part adds a model it loads and releases; a
  speech-to-text model adds transcription; a region adds a published estimate; a fusion region adds
  refinement by confidence and age. Each generation holds only what it adds. A leaf such as Whisper
  writes how to load its model and how to transcribe, and inherits the rest.

The code is the same tree. Each interface is a trait whose supertrait is its parent; each shared
subsystem is a generic owner written once; each lives in the module this chapter names.

Section 9 places every component family in the core on this tree, with its implementors counted, so
nothing is left out. Section 10 is the plan to finish: eleven changes, each moving one whole family
and deleting its old trait in the same change, with no fallbacks and no compatibility layer, and
ratchets that say when it is done.

## 2. The problem

Today the core answers "what is a component" many separate ways.

- **Five ways to live:**
  - `ServiceModule`: commands by string name, events by glob, a tick; 102 implementors.
  - `Daemon`: a tick publishing a watch snapshot.
  - `BrainRegion`: a tick publishing into a ready buffer.
  - The kernel's `Stage`: typed topics, revisions and provenance.
  - `continuum-flow`'s `Stage`: CBAR's `QueueThread`, which no other crate uses.
- **Three ways to start and stop:** persona engine's `Lifecycle` (suspend only), each root's own
  `initialize` and `shutdown` (34 `shutdown` bodies), and a sketch in MODULE-ARCHITECTURE §9.4.
- **No way to be made of components:** every parent writes its own loop over its children.
- **The leaves repeat their parents.** Thirteen audio backends repeat one surface across three
  traits; each keeps its model in a `static`; three owners unload them, each walking the registries
  in its own serial loop, and none reports a model still held by a transcription in flight.

Each copy is a place where a fix lands once and is missed everywhere else.

## 3. Goals and non-goals

Goals:

- One hierarchy of named interfaces, general to specific. Every component descends from it, and
  each generation holds only what it adds.
- Shared subsystems written once: being made of parts, living on a worker, commands and their
  completions, queries and their snapshots, tasks.
- Results that are typed, arrive as they happen, and are never lost.
- State owned by parts, never by globals.

Non-goals:

- Deciding when to release memory or which work runs first. The governor
  ([SHARED-RESOURCES.md](SHARED-RESOURCES.md)) and the flow scheduler (`continuum-flow`) decide; this
  design gives them one surface and exact answers.
- Moving every family at once. Each moves in its own change (section 10).

## 4. The general: what every part is

The general interfaces live in `continuum-base::part` (the floor crate), except the worker, which
lives in `continuum-flow` because it needs an async runtime. Each subsection is one module.

### 4.1 `Part`: identity and generation (`part/part.rs`)

```rust
pub trait Part: Send + Sync {
    fn identity(&self) -> Identity;     // its kind: a static name and a sentence
    fn generation(&self) -> Generation; // which incarnation this is; a replacement gets a new one
}
```

An instance's own id (a persona's id, a configured provider's key) is not its identity; its owner
holds it, or it is the part's key among its parent's parts.

### 4.2 `Parts`: being made of parts (`part/parts.rs`)

A part made of parts holds them in one of two forms with one traversal: a fixed set of named fields
(a persona's branches), or `Parts<K, P>`, a keyed, ordered set with one part per key (a registry of
backends). Replacing a key keeps its place in the order and gives the new part a new generation.

Every method that crosses into a part's parts declares its **dispatch** where the method is
declared, never at the call site:

| Dispatch | Meaning | Example |
|---|---|---|
| Aggregate | Ask every part and combine the answers | footprint, health |
| Broadcast | Send to every part, keep each answer | shutdown, suspend |
| Select | Send to one part, chosen by the parent's policy | transcribe, synthesize: one backend speaks, never all of them |

The traversal for each dispatch is written once. Direction follows dependencies: commands that
start (`prepare`, `resume`) reach the parent first and then its parts; commands that stop
(`suspend`, `shutdown`) reach the parts first and then the parent, which first closes itself to new
work.

### 4.3 `Worker`: how a part lives (`continuum-flow`)

A worker is a part's logical, serialized owner: the one place its state changes, one input at a
time. It is `continuum-flow`'s stage (CBAR's `QueueThread`), joined with the stage contract's
typed topics, revisions and provenance ([STAGE-CONTRACT.md](STAGE-CONTRACT.md)).

- **Inputs** wake it: events (every item matters, read by cursor), snapshots (only the newest
  matters, read by revision), commands, and task completions.
- **Two lanes.** Data inputs share one bounded lane; a reliable producer waits for room. Commands and
  completions have a reserved control lane, so a part whose data lane is full can still be told to
  stop and can still receive the task result it needs to drain.
- **Short turns.** A turn changes the part's state and publishes. It holds no lock across I/O and
  never waits on another part.
- **Executors are shared.** A worker is a logical owner, not an OS thread: many independent parts'
  workers run on a shared executor, and the flow scheduler's QoS classes decide what waits behind
  what.

### 4.4 `Command` and `Completion`: changing a part (`part/command.rs`)

A command changes a part's state. Submitting one never waits: it returns the command's completion at
once, and the caller awaits it or subscribes to it.

```rust
let completion = registry.submit(Shutdown);   // returns immediately
let report = completion.settled().await;      // or completion.subscribe() for each answer as it arrives
```

- **Binding.** A command is bound at ingress, atomically, to an operation id, the target's
  generation, and (for a broadcast) the set of parts it admitted. A part added afterwards is not in
  this command's set; a part replaced afterwards answers under its old generation.
- **Answers arrive as they happen.** Each part's answer is recorded in the completion when it arrives.
  A slow part holds back no other; an observer that detaches and returns sees every answer that
  arrived meanwhile; nothing is reissued.
- **Retries rejoin.** Submitting the same command again while it runs returns the same completion.
- **Fencing.** A `shutdown` admitted during a `prepare` fences the prepare's generation, so the
  prepare's late completion cannot mark the part ready.
- **The parent finishes on its parts' events.** A parent's own step in a stopping command runs when
  its parts' answers arrive, never by blocking on them.

### 4.5 `Outcome`: what an answer says (`part/outcome.rs`)

```rust
pub enum Outcome<P, E> {
    Done,
    Pending(P), // accepted, not finished; P is the completion that will finish it
    Failed(E),
}
```

A part answers with an `Outcome`; a broadcast's completion holds each part's. `Pending` keeps
custody: a model unloaded while a transcription still holds it answers `Pending` on that
transcription, and its bytes stay counted until then. No command has a default answer: a default
`Done` would let a part that holds memory claim to have released it.

### 4.6 `Published`: reading a part (`part/published.rs`)

A query reads what a part has published; it never waits on the worker or takes its lock.

```rust
pub struct Published<S> {
    pub generation: Generation, // whose snapshot
    pub revision: Revision,     // which version of it
    pub state: S,
}
```

An aggregate query over parts combines snapshots of different ages, so it is telemetry: a footprint
read this way says roughly how much is held. Only a confirmed release (a completion's `Done` for a
shutdown) frees bytes the governor may grant to someone else.

### 4.7 `Task`: long work (`part/task.rs`)

Work longer than a turn is dispatched as a task. Its completion is one more input that wakes the
part, on the control lane. Nothing spawned is unowned: every task belongs to the part whose lifetime
it shares, and that part's shutdown stops or joins it.

### 4.8 The lifecycle commands (`part/lifecycle.rs`)

`Prepare`, `Suspend`, `Resume` and `Shutdown` are four commands every part answers, broadcast
through its parts. They are not a mechanism of their own: they are commands, with completions and
outcomes, like any other.

## 5. The specific: the lineages

Each lineage descends from section 4. Each generation names what it adds; everything else it
inherits.

### 5.1 The tree

```
Part                                   identity, generation; parts, worker, commands, queries, tasks
├─ Model                               owns a model it loads and releases       continuum-base::part::model
│  ├─ SpeechToText                     transcribe (select)                      continuum-live audio
│  ├─ TextToSpeech                     synthesize, voices (select)              continuum-live audio
│  ├─ VoiceActivity                    detect (select)                          continuum-live audio
│  └─ InferenceAdapter                 generate (select)                        continuum-engine (step 4)
├─ Region                              publishes an estimate of something       continuum-kernel runtime
│  ├─ FusionRegion                     refines its estimate by confidence and age
│  │  ├─ Amygdala                      the estimate is how much depth a moment needs
│  │  ├─ Wernicke                      the estimate is word-level state, heard and planned
│  │  └─ VisualFusion                  the estimate is a scene
│  └─ Monitor                          the estimate is a measurement (memory pressure, disk); today's Daemon
├─ Holder                              holds governed bytes: footprint, reclaim continuum-kernel resources (Codex)
├─ Holder                              holds governed bytes: footprint, reclaim continuum-kernel resources (Codex)
├─ Service                             answers typed commands from callers      continuum-kernel runtime; today's ServiceModule
├─ Transport                           carries envelopes to peers               continuum-grid
└─ Persona                             one resident, made of her branches       continuum-persona-engine
```

### 5.2 `Model`: a part that holds a model

What it adds: a model it owns and the hook to load it.

```rust
pub trait Model: Part {
    type Loaded: Send + Sync;
    type Failure: Send;
    fn slot(&self) -> &ReloadableModel<Self::Loaded>;          // the field it owns; no statics
    fn load(&self) -> Result<Self::Loaded, Self::Failure>;      // the leaf's only lifecycle code
}
```

What it inherits: the lifecycle commands, written once for every model by a generic owner. Prepare
and resume load through the slot, suspend and shutdown unload it, and an unload while a caller still
holds the model answers `Pending` on that caller. Whether it is loaded is a query on its slot.

A part with no local model (a cloud speech session, a test stub) is not a `Model`. It descends from
`Part` and answers the lifecycle commands by opening and closing its session.

### 5.3 The speech models

What each adds: its own verb, declared with **select** dispatch, so its registry chooses one
backend by policy and no utterance is ever synthesized twice.

```rust
pub trait SpeechToText: Model { async fn transcribe(&self, audio: Audio, language: Option<Language>) -> Result<Transcript, SpeechFailure>; }
pub trait TextToSpeech: Model { async fn synthesize(&self, text: &str, voice: Voice) -> Result<Speech, SpeechFailure>; fn voices(&self) -> Voices; }
pub trait VoiceActivity: Model { fn detect(&self, frame: &Pcm) -> Activity; }
```

A leaf (Whisper, Kokoro, Silero) implements its kind's verb and `Model::load`. Its registry is a
`Parts<Name, dyn TextToSpeech>` with one active member, owned by the voice service.

### 5.4 `Region`, `FusionRegion` and their leaves

What `Region` adds: a published estimate, recomputed when an input it depends on changes, and
invalidating what depends on it. `BrainRegion` and the ready buffers become this.

What `FusionRegion` adds: the estimate is refined, never replaced wholesale. Each source updates it
where it applies, weighted by confidence and age; a rough estimate stands until a better one arrives
([BRAIN-REGIONS-SUBSTRATE.md](BRAIN-REGIONS-SUBSTRATE.md), "Fusion"). Its leaves supply their
estimate type and update rules: the amygdala's depth judgement, Wernicke's word-level state, a
scene.

What `Monitor` adds: a measurement on a cadence floor, for the parts that must report even when
nothing changes. `Daemon` becomes this.

### 5.5 `Holder`, `Service`, `Transport` and `Persona`

- **`Holder`** adds governed bytes: a footprint query (telemetry, by generation and revision) and a
  reclaim command whose completion confirms what was released. `ResourcePool`, `ResourceConsumer`
  and `MemoryReporter` become this one lineage, driven by the governor
  ([SHARED-RESOURCES.md](SHARED-RESOURCES.md); Codex).
- **`Service`** adds typed commands from callers outside the tree (a CLI, a client, a peer). Today's
  `ServiceModule` becomes this; its string command names become typed commands, its event globs
  become topics, and its tick becomes a cadence floor where one is needed.
- **`Transport`** adds carrying envelopes to peers. The grid's four loops over its transports become
  broadcasts over its `Parts`.
- **`Persona`** adds nothing a persona does not need: she is a part made of a fixed set of branches
  (identity, mind, genome, body, residency, learning), each a part
  ([HOW-A-MIND-RUNS.md](HOW-A-MIND-RUNS.md), section 3). Suspending her is one broadcast.

## 6. Names

One name per concept. Each row's old names become the new one; nothing keeps two.

| Name | What it is | Replaces |
|---|---|---|
| `Part` | Anything that runs: identity, generation | the separate roots of every family |
| `Parts` | What a part is made of, with aggregate, broadcast and select | `Children`, `persona_parts!`, hand-written loops |
| worker | A part's logical serialized owner | `Daemon`'s and `BrainRegion`'s tick tasks, `ServiceModule`'s tick loop |
| command, `Completion` | A state change, and its answers as they arrive | `initialize`, `shutdown -> Result`, `Lifecycle::suspend` |
| query, `Published` | A read of published state, with generation and revision | getters that lock or recompute, daemon snapshots, ready buffers |
| `Outcome` | Done, pending on a completion, or failed | `Preservation`, `Result<(), String>` |
| task | Long work whose completion is an input | `Dispatched` runs, unowned spawns |
| `Model` | A part that owns a model it loads | per-backend statics and lifecycle methods |
| `Region`, `FusionRegion`, `Monitor` | A part that publishes an estimate; one that refines it; one that measures | `BrainRegion`, `Daemon` |
| `Holder` | A part holding governed bytes: footprint query, reclaim command | `ResourcePool`, `ResourceConsumer`, `MemoryReporter` |
| `Service` | A part answering typed commands from outside | `ServiceModule` |
| context `Source` | What feeds her view, through the one codec | `RagSource` |

Kept from Codex's review: part, worker, command, completion, task.

## 7. Worked example: a call ends, then memory is needed

1. Two residents finish a call. The voice service is a `Service` whose parts are its speech-to-text
   and text-to-speech registries, each `Parts` of `Model`s. Whisper and Kokoro are loaded; Piper is
   not.
2. Learning needs device memory. The governor submits `Shutdown` to the voice service's model
   registries and receives the completion at once.
3. The command is bound to its operation id and to the three models present at ingress. Each model's
   worker answers on its own:
   - Kokoro unloads and answers `Done`.
   - Piper was never loaded and answers `Done`.
   - Whisper unloads, but a transcription that began a moment earlier still holds the model, so it
     answers `Pending` on that transcription.
4. The completion records each answer as it arrives. The governor funds a grant with Kokoro's bytes
   at once and with Whisper's when the transcription completes; it never funds one from the
   footprint query's telemetry.
5. A new call starts and the voice service submits `Prepare`. The old shutdown's generation is
   fenced, so its late completion cannot unload the model the new call is using.

Today step 3 is a serial loop that logs failures, step 4 does not exist, and step 5 is guarded by a
five-second timer.

## 8. Alternatives considered

- **A lifecycle per family** (an audio `Backend` trait). Tried in #6205 and withdrawn: one more
  copy.
- **Lifecycle as its own mechanism** (#6206's first cut: a verb enum and a walk that only knows
  those verbs). Withdrawn: footprint, health and every other method would need the same machinery
  again. Traversal belongs to `Parts`; the lifecycle is four commands.
- **Calling a part's method and awaiting it.** The caller blocks on the slowest part, a dropped
  caller cancels the work, and results appear only when all are done. Commands with completions
  avoid all three.
- **`&mut self` methods.** Simple with one owner, but parts are shared by handle; exclusive access
  belongs inside the worker's turn.
- **Keeping five worker shapes and adapting between them.** Each adapter is one more copy. They
  differ only in what wakes them and what they publish: inputs and publications of one worker.
- **Blanket implementations** (`impl<T: Model> Lifecycle for T`, and one for composites). Two
  blankets over one trait overlap and Rust rejects them; generic owners give the same sharing.

## 9. Coverage: every component in the core, placed

Every long-lived component family in the core, counted on canary c819052ef7 (production
implementors only, tests excluded), and where it lands. A family is covered when every implementor
is on its lineage and the old trait is deleted.

| Family today | Implementors | Lands as | Owner |
|---|---|---|---|
| `ServiceModule` | 77, across 16 crates (continuum-core 38, mind 9, dreaming 5, engine 3, kernel 3, grid 2, and one each in embedding, serving, sentinel, cognition, live, store, models, gateway, hands) | `Service`; a module that only measures becomes a `Monitor`; one that only publishes becomes a `Region` | Fable (the trait), each crate's owner (its modules) |
| `Daemon` | 5 (`MonitoredGpu`, `ResourceDaemon`, `DiskPressureMonitor`, `DiskUsageScanner`, `MemoryPressureMonitor`) | `Monitor` | Codex |
| `BrainRegion` | 4 (`HippocampusModule`, `CreditAcceptanceRegion`, `ChannelDigestRegion`, `DreamConsolidationRegion`) | `Region`; the hippocampus becomes a branch of each persona's mind | Fable |
| `Faculty` | 4 (`DeferredFaculty`, `LlmDeliberationFaculty`, `RagSourceFaculty`, `RecallFaculty`) | `Region`s her workspace reads; the deliberation faculty is her turn on the Core | Fable |
| kernel `Stage` and flow `Stage` | 1 production user (`FacultyStage`) and 0 | one worker (`continuum-flow`); the kernel trait is deleted | Fable |
| `AIProviderAdapter` | 5 adapters (`LlamaCppAdapter`, `AnthropicAdapter`, `OpenAICompatibleAdapter`, `AircRemoteInferenceAdapter`, `HeuristicInferenceAdapter`) and 2 placement wrappers in mind | `InferenceAdapter: Model` for a local model, `InferenceAdapter: Part` for a remote one; the wrappers become the parent's select policy | Codex (serving) with Fable |
| `SpeechToText`, `TextToSpeech`, `VoiceActivityDetection` | 4, 5 and 4 backends | `SpeechToText`, `TextToSpeech`, `VoiceActivity` on `Model`; the stub, the silence backend and the cloud session on `Part` | Fable (BigMama reviews: live) |
| `GridTransport` | 2 (`ReticulumTransport`, `TailscaleTransport`) | `Transport`; the grid's four loops become broadcasts | Codex (grid) |
| `ResourcePool`, `ResourceConsumer`, `MemoryReporter` | 13, 6 and 5 | one `Holder: Part` lineage: a part that holds governed bytes, answers footprint as a query and reclaim as a command | Codex |
| `RagSource` and context `Source` | 13 and the codec's own | context `Source` only (the one codec, CLAUDE.md); every `RagSource` moves onto it and the trait is deleted | Fable |
| Persona engine `Lifecycle`, `PersonaParts` | 1 composite | `Persona`, a fixed `Parts` of her branches | Fable |
| `PersonaCognition`, `PersonaAircRuntime` | 2 roots | branches of `Persona` (her mind, her presence in rooms) | Fable |

What is not a part: plain values and wire types (a turn's tokens, a gene's bytes, a ts-rs type), pure
functions, and errors. They have no lifetime of their own, so they are owned by the part that holds
them and carry no identity.

## 10. The plan to finish

The rules for every change:

- **One family per change, whole.** A change moves every implementor of one family onto its lineage
  and deletes the old trait, its registrations and its callers' old paths in the same change. Nothing
  is adapted or wrapped, and no old trait outlives its family's change.
- **No fallbacks, no compatibility.** There is no legacy to support. A path that cannot move yet is a
  named, owned blocker on the card, not a shim in the code.
- **Each change deletes more than it adds**, and reports both.
- **Proven before it lands:** its validators from section 11, the hygiene ratchets, and for a change
  to live behavior, a matched read on the live node.

The order, with what each change deletes. Changes on separate rows run in parallel when their owners
differ.

| # | Change | Deletes | Owner | Depends on |
|---|---|---|---|---|
| 1 | The general in `continuum-base::part`: `Part`, `Parts`, `Command`, `Completion`, `Outcome`, `Published`, `Task`, the lifecycle commands (reworks #6206) | persona engine's `Lifecycle`, `Preservation`, `persona_parts!`, `PersonaUnit`'s delegation | Fable | #6204 |
| 2 | The worker: `continuum-flow`'s stage with the control lane, topics, revisions and provenance | the kernel `Stage` trait; `FacultyStage` moves | Fable | 1 |
| 3 | `Model` and the audio lineage | the three speech traits' lifecycle methods, every backend's `static` model, `STTRegistry`, `TTSRegistry`, the three unload walks, the five-second constant | Fable | 1, 2 |
| 4 | `Holder` (the governed-bytes lineage) | `ResourcePool`, `ResourceConsumer`, `MemoryReporter` as separate traits; the duplicate pressure readings | Codex | 1 |
| 5 | `Monitor` | `Daemon`, `DaemonChannel`, the hand-written interval monitors | Codex | 2 |
| 6 | `Region` and `FusionRegion` | `BrainRegion`, the ready buffers, `Faculty` | Fable | 2 |
| 7 | `InferenceAdapter` | `AIProviderAdapter` and its placement wrappers | Codex with Fable | 3 |
| 8 | `Transport` | `GridTransport`; the grid's four transport loops | Codex | 1 |
| 9 | Context `Source` only | `RagSource` and its 13 implementations' old shape | Fable | none |
| 10 | `Persona` as one tree | `PersonaCognition` and `PersonaAircRuntime` as separate roots | Fable | 1, 6 |
| 11 | `Service` | `ServiceModule`: its string command names, glob subscriptions and tick loop, family by family across the 16 crates, the last one deleting the trait | Fable (trait), each crate's owner | 2, 5, 6 |

How we will know it is finished, measured on canary by the hygiene ratchets:
- the old roots (`ServiceModule`, `Daemon`, `BrainRegion`, the kernel `Stage`, `Faculty`,
  `AIProviderAdapter`, `GridTransport`, `RagSource`, `ResourcePool`, `ResourceConsumer`,
  `MemoryReporter`, the three speech traits' old shape) are at zero;
- `fn shutdown` bodies outside the lineages' generic owners are at zero;
- hand-written loops over a part's children are at zero;
- `static` model slots are at zero.

A ratchet for each lands with change 1 and only falls.

## 11. How we will know it works

Validators that run in seconds, with fixture parts and no model or device:
- a fixed set and a keyed set of parts driven by one traversal, for an aggregate query and a
  broadcast command;
- one slow part and one failing part in a broadcast: the others finish, the failure is named, and an
  observer sees the finished answers before the slow one arrives;
- an observer that detaches mid-command and reattaches, seeing every answer that arrived meanwhile,
  with nothing reissued;
- a part's data lane saturated, then `Shutdown`: it arrives on the control lane and completes;
- a part replaced mid-shutdown with no other handle to it: it finishes under its old generation, and
  its stale publication is not read as its replacement's;
- concurrent suspend, resume and shutdown on one part, and a shutdown during a prepare;
- two text-to-speech backends installed: one utterance is synthesized exactly once;
- an unload while a caller holds the model: `Pending`, released when the caller finishes.

On the live node, after step 3:
- the bytes the governor grants from match what the device reports released;
- releasing the audio models under pressure no longer waits on the slowest one;
- the five-second idle constant is gone.

## 12. Open questions

- Whether idle release should exist at all, or release is only the governor's decision under
  pressure. The policy belongs in [SHARED-RESOURCES.md](SHARED-RESOURCES.md).
- Where `Model` lives once `InferenceAdapter` adopts it: `continuum-base` for now, because both
  `continuum-live` and `continuum-engine` need it.

