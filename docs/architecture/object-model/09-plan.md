# 9. The plan

[The object model](README.md) · previous: [Coverage](08-coverage.md)

How the core gets from chapter 1 to chapters 3 through 7, and how we know it is finished.

## 9.1 Rules for every change

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

## 9.2 Phases

Each phase lists its changes in order. Rows in one phase with different owners run in parallel.

### Phase A: the roots

| Change | Deletes | Owner |
|---|---|---|
| `continuum-base::part`: `Part`, `Parts`, `parts!`, `Command`, `Completion`, `Outcome`, `Published`, `Task`, the lifecycle commands (replaces #6206's first cut) | persona engine's `Lifecycle`, `Suspension`, `Preservation`, `persona_parts!`, `PersonaUnit`'s delegation | Fable |
| `continuum-base::{policy, check, sink}` | nothing yet; families move in phase D | Fable |
| `continuum-flow::worker`: the stage with the control lane, topics, revisions and provenance | the kernel's `Stage`, `Inputs`, `DependentStage` (one production user moves) | Fable |
| `continuum-saved-state`: `Durable` with media, the ORM as one medium | nothing yet | Codex |
| Ratchets: one per old root (chapter 9.3), at today's counts, only falling | nothing | Fable |

### Phase B: the crate layers (pure moves)

Before the first move, capture `cargo metadata` with every optional and target-specific edge, and the
feature closures of the headless CPU, Metal and CUDA builds. Each move compares them before and after,
so that UI, remote and accelerator dependencies stay optional and nothing optional becomes default
(Codex's review).

One change per target crate (chapter 6.2), each moving whole modules with paths only, tests unchanged:

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
| `Holder` (pool, residency, cache) | `ResourcePool`, `ResourceConsumer`, `MemoryReporter`, `DeviceTenant`, `StagingResidency`, `VoiceReclaimLever`, `DiskReporter` as traits | Codex |
| `Monitor` | `Daemon`, `DaemonChannel`, `GpuMonitor` and `CapacitySource` as separate roots, the hand-written interval monitors | Codex |
| `Region`, `FusionRegion`, `Faculty` | `BrainRegion`, ready buffers, `RagSourceFaculty` | Fable |
| `Transport` | the client's and kernel's `Transport`, `GridTransport`, the airc and inference transports, gateway `Wire` as separate roots | Codex |
| `Room` and its views; `ActivityAdapter` | `AircCitizen` and its readers, the board readers, the three suite adapters | Fable |
| `Search` | `RecallLayer`, `CandidateSource`, `SearchAlgorithm` | Fable |
| `Renderer` | `RenderBackend`, `AvatarRenderer` | BigMama |
| `Policy`, `Check`, `Sink`, `Durable` leaves | the decision, judging, recording and persistence traits of chapter 8 | each family's owner |
| `Source` only | `RagSource`, `CriticalSource`, `RagRenderable` and its two scopes | Fable |

### Phase E: the persona and services

| Change | Deletes | Owner |
|---|---|---|
| `Persona` as one tree (chapter 5) | `PersonaCognition` and `PersonaAircRuntime` as separate roots; the node-wide hippocampus | Fable |
| `Service`, family by family across the crates; the last change deletes the trait | `ServiceModule`, string command names, glob subscriptions, the runtime's tick loop | Fable for the trait, each crate's owner for its modules |

## 9.3 Done

The plan is finished when the ratchets added in phase A read zero on canary:

- old roots: `ServiceModule`, `Daemon`, `BrainRegion`, both `Stage`s, `AIProviderAdapter`, `GridTransport`,
  `RagSource`, `ResourcePool`, `ResourceConsumer`, `MemoryReporter`, `EmbeddingProvider`, `RenderBackend`,
  `AvatarRenderer`, `AircCitizen` and its readers, and every trait chapter 8 marks for deletion;
- `fn shutdown` bodies outside the holdings' generic owners;
- hand-written loops over a part's parts;
- `static` model slots and other state in globals;
- crates outside chapter 6's layers, and dependencies that point up a layer.

Each ratchet only falls. Each phase's progress is its ratchets' numbers, reported on canary with the
SHA and date, and recorded in the plan table of CLAUDE.md.
