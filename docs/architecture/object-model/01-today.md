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

## 1.5 The runtime as built (audit, 2026-11-21, 7a5fd04ec8)

Sections 1.1 to 1.4 count declarations. This section reads what runs. Four slices were read end to
end at 7a5fd04ec8 (the M5's build that afternoon) against chapters 0 to 5; every line cited was read.
Where a finding is an inference, it says so.

### The engine runs one mind at a time, and everything in front of it still plans lanes

The in-process persona engine is the only production engine (`continuum-core`
`ipc/start_server/serving_modules.rs:37`). In front of it, the llama-server lane stack still plans,
sizes, permits and grants:
- **The Core's size comes from the lane planner.** `plan_serving`'s lane fit and the knee clamp
  (`serving_daemon/demand.rs`) choose a per-mind window; `launch.rs` `engine_pool` passes it on;
  `llama_cpp_backend.rs:454-470` multiplies it by `STREAMS = 2` (`unified_tier.rs:19`), and the
  context opens with `n_seq_max: 2` and no unified KV (`driver.rs:410-418`; the binding does not expose
  `kv_unified`, `backend_init.rs:490-519`). llama.cpp then gives each sequence half
  (`llama-context.cpp:293-297`). Live: `n_ctx 122368, n_ctx_seq 61184`.
- **The half is everyone's window.** `in_process_engine.rs:212-220` publishes `n_ctx_seq` as the
  served window and the sequence cap; the engine's door refuses at it (`admission.rs:30`,
  `driver.rs:560`).
- **The second half does nothing concurrent.** A swap stages, settles, hands off and retires
  synchronously (`driver.rs:852-876`; overlap is "Not yet", `persona_scheduler.rs:17`).
- **Every act passes a lane permit and a per-mind grant.** A lane permit (`resource_admission/lanes.rs`,
  idle turns held to lanes minus one); then `mind_window::decide_and_hold` and `window_grant`
  (`contribute.rs:50-79, 471`), grants recomputed on a 5 s tick (`serving_daemon/tick.rs:88`) by
  dividing the pool among minds (`grants.rs:100-147`); then a reservation held until her terminal
  receipt (`lease.rs:369`).
- **A new shape needs a restart.** A changed allocation is refused: "release the existing backend
  before resizing" (`llama_cpp_backend.rs:476-481`).

### Minds, models and genes have three unrelated lifecycles

- Minds: `Roles` (active, next, retiring) in `continuum-persona-engine`, plus a bare `parked` map in the
  driver that never pages to RAM or disk except at a stop (`driver.rs:132, 483`).
- Models: `EngineState` and `Option<Loaded>` behind a `static NODE_ENGINE`; a second model is refused
  (`HoldsAnotherBase`) and unloading is refused while residents exist (`in_process_engine.rs:184-209`).
  A load's duration is a log line, not a probe: the last one took about 16.5 s for a 20.4 GB GGUF,
  page cache likely warm (15:44:19 to 15:44:35Z).
- Genes: a `HashMap` of adapters that never evicts and never reports its bytes
  (`llama_cpp_backend.rs:77, 440`).
- Unused: the engine's `Lifecycle` trait has no caller through it; `schedule::NextPolicy` has no
  implementor; the real next-mind choice is `frame::pick` (a 500 ms wait threshold, no value curve, no
  switch cost).

### The turn crosses text where it should cross types, and loses what the engine measured

- The chat seam is a JSON round trip in process (request as `Value` to string to C++ parse; `describe`
  and the parsed message `.dump()`ed and re-parsed, `chat_turn.rs:60,189`), and a call's arguments
  cross as a string parsed twice.
- The engine counts her prompt exactly and reports `input_tokens: 0` and `timing: None`
  (`provider.rs:613, 626`), so cognition estimates what was measured and sees no reuse.
- Why a turn ended is guessed from counts and strings (`chat_turn.rs:229-235`; stops matched in Emit and
  again in `collect`). Failures are `String`s (`TokenEvent::Error`, `ChatRefusal`, `From<String> for
  InferenceError` collapsing to `Transient`).
- The Core streams tokens, and the adapter seam collects them whole and delivers one chunk after decode
  (`TurnAnswer::collect`, `deliver_whole`, provider.rs:628): a live voice cannot start early (inference).
- A malformed persona id silently becomes an anonymous one-off whose memory is dropped after the turn
  (`provider.rs:412-415`, `driver.rs:554, 883`).

### She is run by a clock and pushed by the system

- A self-tick of 15 s backing off to 240 s runs a full self-cycle (`beat_outcome.rs:13-46`); on it,
  `ask_the_act_question` drives a work turn with fixed text ("This turn is for your claimed work",
  `work_burst.rs:22-48`), with a "[progress checkpoint]" after 6 acts without a write.
- Room messages start turns by a text rule (`triggers_a_turn`, `wake_backlog.rs:312`, a glyph prefix
  for receipts), bypassing salience and her dial. `set_dial` has no production caller, so her dial is
  always the default.
- Six fabricated sentinel acts and their teacher texts hand her another generation
  (`contribute.rs:768-980`, `command_executor.rs:237-370`); `[act-budget]`, `[unfulfilled]` and
  `[unverified]` facts are written into her working memory by text heuristics.
- Her own act receipts never reach perception (the feed drops them as "her own echo"); surprise fires
  only on a card verdict. Grading facts sit in the general work path for benchmark-titled cards
  (`act_question.rs:273-295`).
- Of the regions chapter 5 names: senses are a struct called synchronously from the inbound path;
  attention has two owners; the hippocampus is node-wide and polled every second
  (`substrate_governor.rs:125`); the amygdala, Wernicke and drives do not exist.

### Numbers written for one machine

Thirty were listed with their values; the most consequential: `STREAMS = 2`; `MAX_LANES = 8`;
`MINDS_PER_LANE_STARVED_ABOVE = 2` (`continuum-vitals` `health_ledger.rs:598`, which also bounds the
roster, grid seats and the host cache); `MIN_KNEE_LANES = 2`; `DECODE_FLOOR_TPS = 10`;
`MIN_SERVE_CTX = 2048`; `BOOTSTRAP_WORKING_SET = 16384`; the power-mode serving fractions
0.55/0.80/0.92/0.96; `UBATCH_TOKENS = 2048` (measured on the M5, applied everywhere);
`PREFILL_COMPUTE_KV_DIVISOR = 4` (calibrated on one 24B model); `CACHE_RAM_MIB = 4096`;
`ACT_LATENCY_BUDGET_SECS = 150`; `TICK_DEADLINE = 25 min`, which abandons an in-process act still
decoding because its extension reads only llama-server slots (`tick_deadline.rs:118-123`, inference);
the 15 s to 240 s self-tick; `select_local_model` returning a fixed model id (`persona/allocator.rs:168-174`).

### What is right and stays

- One mind decodes at a time, structurally (`Roles` cannot hold two), and the driver blocks on events,
  never polls. Emit and Record are flow stages off the engine thread; tokenizing and the sampler are
  built on her own thread.
- A mind is hydrated from a descriptor: `PersonaUnit` carries her blocks, history and checkpoints, with
  typed `MindId { persona, epoch }` the fork enforces; geometry is read from the created context with
  an incarnation; checkpoint reuse beyond append, every miss classified.
- Custody: every pipeline refusal returns what it was given; preserve writes one complete generation
  and publishes its manifest last; restore verifies; `end()` is the one terminal path of a turn.
- The pressure owners (`MemoryPressureMonitor`, `live_pool`'s give-back order, the `ResourceGovernor`'s
  two-phase reclaim), the measured knees as evidence, the typed `Waiting` reasons with numbers.
- The mind's pure pieces: salience as a typed pure function with surprise carrying expectation and
  observation; her continuation written only by her act; `DeferredFaculty`'s CBAR shape; mid-turn
  perception between acts; the executor checking arguments against each command's own parameters.

What it means: the pieces that are right are the object model's own (a Core of one occupant,
hydration, custody, pure salience); what is wrong is the lane era still in front of them and a clock
driving her. Chapters 2, 3, 4.11, 5.7 and 11.4 are revised from this section.
