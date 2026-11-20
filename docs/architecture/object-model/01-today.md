# 1. The core today

[The object model](README.md) · previous: [The engine](00-engine.md) · next: [Principles](02-principles.md)

Measured on canary c819052ef7, 2026-11-20, by reading every `Cargo.toml`, every `pub trait` and every
`impl ... for` in the workspace (tests excluded). Each section ends with what it means for the design.

## 1.1 Crates

The workspace has 47 crates under `core/`, plus four outside it (`apps/cli`, `client/continuum-client`,
`client/continuum-client-ffi`, `tools/manifest-gen`), about 515,000 lines of Rust.

- **Five crates hold 64% of it:** `continuum-engine` (131k lines), `continuum-core` (103k),
  `continuum-mind` (95k), `continuum-kernel` (89k), `continuum-cognition` (85k).
- **Thirteen dependency layers.** The floor is sound: 18 leaf crates depend on no other workspace
  crate (`continuum-base`, `continuum-host`, `continuum-flow`, the protocol crates, `llama`,
  `text-analysis`). Above the kernel, the stack is one chain: cognition (layer 7), academy (8), mind
  (9), dreaming and grid (10), serving (11), core (12). That is a monolith cut along file lines, not
  along the placement map of MODULE-ARCHITECTURE §9.2.
- **The kernel sits above things that should sit above it.** `continuum-kernel` depends on
  `continuum-cli-lifecycle` and on the client library (`core/continuum-kernel/Cargo.toml`), so the
  client is below the microkernel.
- **Crates wired into nothing:** `continuum-flow` (the stage the worker is built on),
  `continuum-persona-engine`, `continuum-persona-llama` and `continuum-proving-types`.
- **One purpose in several crates:**
  - inference and its scheduling: `continuum-engine` and the unwired persona engine pair;
  - serving: `continuum-serving` and `continuum-engine`'s `serving_pin_store` and `serving_tier_down`;
  - memory and recall: `continuum-memory`, `continuum-engram`, `continuum-store`'s vectors and
    `continuum-embedding`;
  - learning: `continuum-genome`, `continuum-dreaming` and `continuum-academy`;
  - processes: `continuum-process`, `continuum-cli-lifecycle::process` and `continuum-host`'s four
    process modules;
  - home directories: `continuum-base::home`, the kernel's `paths`, and `continuum-engram::home`.

What it means: crates are cut by ownership (chapter 8), and the merges and splits are part of the
plan (chapter 11).

## 1.2 Traits

There are 224 traits.

- **Inheritance is almost absent.** Nine traits have a workspace supertrait, and no chain is deeper
  than two levels. Every family is flat: each implementor implements its trait directly from scratch.
- **72 traits are pure** (no default bodies) with two or more implementors or a supertrait.
- **Generic owners exist but are rare:** about 25 structs implement a behavior once for many leaves
  (`PagedResourcePool`, `MonitoredGpu<P>`, `ShedLru`, `OrmStore<T>`, `View<T>`). Only three blanket
  impls exist, all in the kernel's command system.
- **Four names collide across crates:** `Stage` (flow and kernel), `Transport` (client and kernel),
  `CommandDispatch` (core and positron), `CommandHandler` (kernel and client FFI).

What it means: the design must supply the middle generations that are missing, so leaves stop
re-implementing them (chapters 2 and 4).

## 1.3 One idea, many traits

The clearest evidence. Each row is one concept the core declares several times.

| Concept | Declared as | Count |
|---|---|---|
| Record an event for capture or replay | `CaptureSink` (base), `TurnCredit`, `WorkspaceCaptureSink`, `LaneCaptureSink`, `PlacementCaptureSink`, `RagCaptureSink`, `PromptCaptureSink`, `GridCaptureSink`, `PagerCaptureSink` | 9 |
| Hold bytes and give them back | `ResourcePool`, `ResourceConsumer`, `MemoryReporter`, `DeviceTenant`, `StagingResidency`, `RenderSurface`, `VoiceReclaimLever`, `DiskReporter`, `CapacitySource` | 9 |
| Wake on a clock and publish | `ServiceModule::tick`, `Daemon`, `BrainRegion` | 3 |
| Do a turn of work on inputs | flow `Stage`, kernel `Stage`, `DependentStage` | 3 |
| Feed a mind's view | context `Source`, `RagSource`, `CriticalSource`, positron `StateSource`, `RagRenderable` | 5 |
| Read a room through airc | `AircCitizen` and its six parent readers, `WorkBoardReader`, `RoomBoardReader`, `CardBoards`, `BoardFetch` | 11 |
| Decide | `AllocationPolicy`, `WindowPolicy`, `GridPlacementPolicy`, `TierDownPolicy`, `NextPolicy`, `FocusPolicy`, `WorkingSetPolicy`, `BatchScheduler` | 8 |
| Judge something | `SourceRule`, `CrateRule`, `Shape`, `SyntaxValidator`, `PerceptionFact`, `RedactionDetector`, `SalienceDetector` | 7 |
| Carry messages to a peer | client `Transport`, kernel `Transport`, `GridTransport`, `AircEventTransport`, `AircRealtimeStore`, `AircInferenceTransport`, gateway `Wire` | 7 |
| Dispatch one command | core and positron `CommandDispatch`, kernel `CommandHandler`, `DynCommand`, `GridDispatch`, client `ServeHandler`, FFI `CommandHandler` | 7 |
| Load a model and run it | `SpeechToText`, `TextToSpeech`, `VoiceActivityDetection`, `EmbeddingProvider`, `AIProviderAdapter`, `RenderBackend` | 6 |
| Find candidates and rank them | `RecallLayer`, `CandidateSource`, `RecallRanker`, `SearchAlgorithm` | 4 |
| Persist state | `SavedState`, `OrmEntity`, `LedgerStore`, `RunLedger`, `ActivityStateStore`, `StepStateSink`, `RecallTraceStore`, `EpochWatermarkStore` | 8 |
| Adapt an outside task suite | `SuiteAdapter`, `ActivityAdapter`, `BenchmarkAdapter` | 3 |

What it means: each row becomes one root or one lineage, written once (chapters 3 and 4), and the
other declarations are deleted (chapter 11).

## 1.4 Long-lived components

| Family | Production implementors |
|---|---|
| `ServiceModule` | 69 in 15 crates |
| `Source` (context) and `RagSource` | 10 and 13 |
| `ResourcePool` | 13 |
| `AIProviderAdapter` | 5 adapters and 2 wrappers |
| `Faculty` | 5 |
| Speech backends | 13 across three traits |
| `Daemon` | 5 |
| `BrainRegion` | 4 |
| `GridTransport` | 2 |
| Persona engine `Lifecycle` | 1 composite |

Three ways to start and stop exist side by side: persona engine's `Lifecycle` (suspend only), each
family's own `initialize` and `shutdown` (34 `shutdown` bodies), and a sketch in MODULE-ARCHITECTURE
§9.4. No family is made of parts: every parent loops over its children by hand.
