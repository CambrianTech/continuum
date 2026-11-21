# 11. The plan

[The object model](README.md) · previous: [Coverage](10-coverage.md)

How the core gets from chapter 1 to chapters 3 through 9, and how we know it is finished.

## 11.0 Workstreams

Work runs in parallel streams, each owning a domain of the book end to end: its chapter, its
objects, its code, its validators and its live gate. The phases in 11.2 are shared foundations that
every stream builds on.

| Stream | Chapter | Owner | First deliverables |
|---|---|---|---|
| Minds and the genome | 5, 6 | Fable | #6203 lands; learning as a mode of her turn (`Step` labels, the duty, learning-context paging); the persona as one tree; the live learning gate |
| Live calls and embodiment | 7 | a second agent focused on it, with BigMama (live and media) | the proof plan's stages 1 to 3: one speaker to one listener, interruption, rendered bodies with native vision; then shared attention, scale, and a shared world |
| Resources, serving and the grid | 3.7, 3.8, 4.3, 4.4, 8 | Codex | `Demand` cleared by the governor; `Holder`; `Durable` media; the serving and grid crate cuts |
| The learning acceptance test | 6.7 | Cormac | a reviewed correction learned in place, retained after a restart, on unseen tasks, against the base without the gene |

Each stream reports its progress as its own validators and live receipts, plus the ratchets of 11.3
that its changes move.

## 11.1 Rules for every change

- **Whole families, nothing left behind.** A lineage change moves every implementor of the family it
  names and deletes the old trait, its registrations and its callers' old paths in the same change.
  Nothing is wrapped, adapted or kept for compatibility. There is no legacy to support.
- **No fallbacks.** A path that cannot move yet is a named, owned blocker on the change's card, never a
  shim in the code.
- **Moves and redesigns are separate changes** (MODULE-ARCHITECTURE §9.3). A crate cut is a pure move,
  proven by unchanged tests and a build. A lineage change transfers ownership, proven by its
  validators. They are never mixed in one change.
- **Each change deletes more than it adds,** and reports both.
- **Proven before it lands:** its validators, the hygiene ratchets, clippy with tests on the crates it
  touches, and for live behavior a matched read on the live node.

## 11.2 Phases

Each phase lists its changes in order. Rows in one phase with different owners run in parallel.

### Phase A: the roots

| Change | Deletes | Owner |
|---|---|---|
| `continuum-base::part`: `Part`, `Parts`, `parts!`, `Command`, `Completion`, `Outcome`, `Published`, `Task`, the lifecycle commands (replaces #6206's first cut) | persona engine's `Lifecycle`, `Suspension`, `Preservation`, `persona_parts!`, `PersonaUnit`'s delegation | Fable |
| `continuum-base::{policy, check, sink}` | nothing yet; families move in phase D | Fable |
| `continuum-flow::worker`: the stage with the control lane, topics, revisions and provenance | the kernel's `Stage`, `Inputs`, `DependentStage` (one production user moves) | Fable |
| `continuum-saved-state`: `Durable` with media, the ORM as one medium | nothing yet | Codex |
| `continuum-base::demand`: `Demand`, value curves, essentials, quality ladders, progress, switch cost; the governor clears demands into leases and typed outcomes ([GRID-MARKET-CLEARING.md](../GRID-MARKET-CLEARING.md) §1a, the `cambrian-flex` solver) | request types move onto it family by family in phase D (`Holder`) | Codex, with Fable for the Core's scheduler |
| Ratchets: one per old root (chapter 11.3), at today's counts, only falling | nothing | Fable |

### Phase B: the crate layers (pure moves)

Before the first move, capture `cargo metadata` with every optional and target-specific edge, and the
feature closures of the headless CPU, Metal and CUDA builds. Each move compares them before and after,
so that UI, remote and accelerator dependencies stay optional and nothing optional becomes default
(Codex's review).

One change per target crate (chapter 8.2), each moving whole modules with paths only, tests unchanged:

1. The floor: `continuum-protocol` (three protocol crates), `continuum-context` absorbs prompt and
   grounding, the kernel drops its dependencies on the client library and the CLI lifecycle.
2. The domains:
   - `continuum-serving` (engine, serving, persona engine, persona llama, models);
   - `continuum-media` and `continuum-render` (live, split);
   - `continuum-memory` (memory, engram, embedding, the store's vectors);
   - `continuum-hands` (hands, git, sentinel);
   - `continuum-activity` (experience, rooms);
   - `continuum-perception` (perception, engine's sensory).
3. Learning and minds:
   - `continuum-genome` absorbs dreaming and core's forge;
   - `continuum-mind` absorbs cognition, vitals, capture and tool-working-set;
   - `continuum-academy` gives its learning machinery to the genome.
4. The node: `continuum-core` keeps composition and binaries; `continuum-cli-lifecycle` moves to
   `apps/cli`, `jtag-mcp` to `apps/mcp`, `continuum-proving-types` into `serving-replay`.

Owners follow the domain: Codex for serving, grid and the kernel's resources; BigMama for media and
render; Cormac for the genome; Fable for the rest.

### Phase C: models and their holdings

| Change | Deletes | Owner |
|---|---|---|
| `Loaded`, `Session`, the slot in `continuum-base::part::holding` | `ReloadableModel` in live | Fable |
| The speech capabilities and their 13 leaves; one select policy per capability | the three traits' lifecycle methods; every backend's `static` model; `STTRegistry`, `TTSRegistry`; the three unload walks and the five-second constant | Fable, with BigMama reviewing |
| `Embedder` and its cache and counting owners | `EmbeddingProvider` | Fable |
| `InferenceAdapter` | `AIProviderAdapter`, `DispatchTermsPolicy` as a trait, the placement wrappers, `AdapterRegistry`, `LlamaServerControl` with the llama-server lane path | Codex with Fable |

### Phase D: the remaining lineages (parallel)

| Change | Deletes | Owner |
|---|---|---|
| The Core owns its budget (chapters 3.8 and 4.11; ordered in 11.4) | the serving daemon's per-mind grants (`serving_daemon/grants.rs`, `capacity::window_grant`) and their owed and `no_grant` states; the window allocator's sizing of minds; the lane planner (`serving_plan/max_lanes.rs` with `MAX_LANES = 8`); `MIN_KNEE_LANES = 2`; `MIN_SERVE_CTX = 2048`; health's `MINDS_PER_LANE_STARVED_ABOVE = 2` (it judges starvation by minds per lane); the hosting reconciler's seat bound (`persona.roster.bounded`: seats = lanes × minds per lane, which left two of four residents unseated on the M5, 2026-11-21) | Fable (Core), Codex (callers), the allocator card's resident (2eec3977) for its deletion |
| `Holder` (pool, residency, cache) | `ResourcePool`, `ResourceConsumer`, `MemoryReporter`, `DeviceTenant`, `StagingResidency`, `VoiceReclaimLever`, `DiskReporter` as traits | Codex |
| `Monitor` | `Daemon`, `DaemonChannel`, `GpuMonitor` and `CapacitySource` as separate roots, the hand-written interval monitors | Codex |
| `Estimator`, `Fusion`, `Region`, `FusionRegion`, `Faculty` | `BrainRegion`, ready buffers, `RagSourceFaculty` | Fable |
| `Transport` | the client's and kernel's `Transport`, `GridTransport`, the airc and inference transports, gateway `Wire` as separate roots | Codex |
| `Room` and its views; `ActivityAdapter` | `AircCitizen` and its readers, the board readers, the three suite adapters | Fable |
| `Search` | `RecallLayer`, `CandidateSource`, `SearchAlgorithm` | Fable |
| `Renderer` | `RenderBackend`, `AvatarRenderer` | BigMama |
| `Policy`, `Check`, `Sink`, `Durable` leaves | the decision, judging, recording and persistence traits of chapter 10 | each family's owner |
| `Source` only | `RagSource`, `CriticalSource`, `RagRenderable` and its two scopes | Fable |

### Phase E: the persona and services

| Change | Deletes | Owner |
|---|---|---|
| `Persona` as one tree (chapter 5) | `PersonaCognition` and `PersonaAircRuntime` as separate roots; the node-wide hippocampus | Fable |
| `Service`, family by family across the crates; the last change deletes the trait | `ServiceModule`, string command names, glob subscriptions, the runtime's tick loop | Fable for the trait, each crate's owner for its modules |

## 11.3 Done

The plan is finished when the ratchets added in phase A read zero on canary:

- old roots: `ServiceModule`, `Daemon`, `BrainRegion`, both `Stage`s, `AIProviderAdapter`, `GridTransport`,
  `RagSource`, `ResourcePool`, `ResourceConsumer`, `MemoryReporter`, `EmbeddingProvider`, `RenderBackend`,
  `AvatarRenderer`, `AircCitizen` and its readers, and every trait chapter 10 marks for deletion;
- `fn shutdown` bodies outside the holdings' generic owners;
- hand-written loops over a part's parts;
- `static` model slots and other state in globals;
- crates outside chapter 8's layers, and dependencies that point up a layer.

Each ratchet only falls. Each phase's progress is its ratchets' numbers, reported on canary with the
SHA and date, and recorded in the plan table of CLAUDE.md.

## 11.4 After the audit of 2026-11-21: what goes first

The audit (1.5) found the object model's own pieces right (one occupant per Core, hydration, custody,
pure salience) and two things in front of them wrong: the lane era still planning, sizing and granting
in front of the Core, and a clock running her mind. Those go first, ahead of the remaining lineage
phases, each as one change measured live before the next (11.1).

| # | Change | Deletes | Owner |
|---|---|---|---|
| 1 | Review of 1.5, 2.8, 2.9, 3.9, 4.11 and 5.7 | nothing; no code before it | Joel, Codex |
| 2 | Simulator assertions, red: 4.11's eight invariants and 5.7's gate on the dyno, including two awake models whose capacity generations coincide (a delayed save from one never frees the other's cells), and a shrink observed three ways: the target, the backend's bytes returned, and a second holder actually acquiring them (Codex, 73fe648b0c) | nothing | Fable (Core, mind), Codex (custody replays: stage, save and reclaim overlapping) |
| 3 | One unified context per awake model; capacity published (4.11 steps 3 and 4) | `STREAMS`, `n_ctx_seq` as a limit, the door, `mind_window` and `window_grant` on deliberation's path | Fable |
| 4 | Governance has nothing to divide (4.11's table) | the lane planner, grants, permits, seats, per-slot windows, lane-keyed health, the roster's seat bound | Codex (serving, admission, health, grid), Fable (engine, cognition) |
| 5 | `Measured<T>`, `Prior<T>` and the `ComputeUnit` descriptor in `continuum-base`; the unit's probes | the thirty numbers of 1.5, each moved to a measurement or a prior, row by row | Fable (types), each owner for its numbers |
| 6 | `Presence` (3.9): residents for minds, models and genes; the pager with Codex's custody | `parked`, `EngineState` as a lifecycle, the gene map, the KV slot pool's slot keying | Fable (Core), Codex (`Holder`) |
| 7 | The streamed, typed turn (4.11 invariants 6 and 7) | collect-whole, the JSON seam, the second argument parse, string failures, estimates of measured facts | Fable |
| 8 | Her own mind's events (5.7): senses as her part, receipts as percepts, surprise from receipts, attention the only waker, time as her scheduled events | the self-tick, the act question, the text wake rule, the sentinels and teacher texts, grading facts in the work path | Fable |
| 9 | The next-mind policy with value and switch cost; a second model may wake (4.11 invariant 8) | `frame::pick`'s threshold; the refusal of a second model and of unloading | Fable |

Rows 3, 4 and 5 may run in parallel by owner; row 8 depends on none of the Core rows and starts with
row 2.

New ratchets, added with row 2, only falling: numeric `const` items read by a `Policy` (row 5); timers,
ticks and intervals that start a mind's turn (row 8); words of the lane era in identifiers (`lane`,
`slot`, `seat`, `per_mind`, `n_ctx_seq`) outside tests (rows 3 and 4).
