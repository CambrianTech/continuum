# 8. Crates

[The object model](README.md) · previous: [Live calls and embodiment](07-live.md) · next: [Modules](09-modules.md)

Crates are layered from the general at the bottom to the specific at the top. A crate depends only on
crates in layers below it (the `layer_discipline` ratchet enforces this). Each domain crate owns one
lineage from chapter 4 and can run on its own node, as the placement map requires
(MODULE-ARCHITECTURE §9.2); domains talk to each other through commands and publications, never by
reaching into each other's state.

## 8.1 The layers

```
6  node            continuum-core (the composition and its binaries)
5  interfaces      continuum-positron, continuum-gateway
4  minds           continuum-mind, continuum-academy
3  learning        continuum-genome
2  domains         continuum-serving, continuum-media, continuum-render, continuum-perception,
                   continuum-memory, continuum-store, continuum-hands, continuum-grid, continuum-activity
1  kernel          continuum-kernel, continuum-process
0  floor           continuum-base, continuum-host, continuum-flow, continuum-protocol,
                   continuum-context, continuum-saved-state, llama, and the pure leaves
```

## 8.2 Each crate

### Floor (layer 0): knows nothing of minds

| Crate | Owns | Hosts (chapter 3, 4) | Absorbs from today |
|---|---|---|---|
| `continuum-base` | the roots and leaf utilities | `Part`, `Parts`, `Command`, `Completion`, `Outcome`, `Published`, `Task`, the lifecycle commands, `Loaded`, `Session`, `Policy`, `Check`, `Sink` | its own utilities; `ReloadableModel` from live |
| `continuum-host` | every OS-divergent capability | none | `continuum-cli-lifecycle::process` and `continuum-process`'s OS calls |
| `continuum-flow` | the worker, QoS classes, dispatchers | the worker | the kernel's `Stage` and `DependentStage` |
| `continuum-protocol` | wire types for every seam | none | `continuum-airc-protocol`, `continuum-tool-protocol`, `continuum-bridge-protocol` (directory `livekit-protocol`), as modules |
| `continuum-context` | a mind's view: the one codec | `Source` | `continuum-prompt`, `continuum-grounding` |
| `continuum-saved-state` | state that outlives a process | `Durable` | none |
| `llama`, `continuum-text-analysis`, `expert-pager-policy`, `continuum-orm-derive` | pure leaves | `expert-pager-policy` is a `Policy` leaf | none |

### Kernel (layer 1): hosts parts

| Crate | Owns | Hosts | Absorbs from today |
|---|---|---|---|
| `continuum-kernel` | the runtime that hosts parts: commands and their registry, topics, the governor, the ORM medium, paging, identity, logging and probes | `ActionCommand`, `Service`, `Region`, `Monitor`, `Holder` and its owners | itself, minus its dependencies on the client library and the CLI lifecycle (both move above it) |
| `continuum-process` | owned child processes and their termination | none | itself |

### Domains (layer 2): one lineage each, each runnable on its own node

| Crate | Owns | Hosts | Absorbs from today |
|---|---|---|---|
| `continuum-serving` | the Core, residency, inference adapters, capacity and planning, the model registry | `InferenceAdapter`; the persona engine's tier | `continuum-engine` (inference, capacity, model registry, serving modules), `continuum-serving`, `continuum-persona-engine`, `continuum-persona-llama`, `continuum-models` |
| `continuum-media` | calls, mixing, speech | `SpeechToText`, `TextToSpeech`, `VoiceActivity` | `continuum-live`'s audio and call modules; `livekit-bridge` stays its own binary crate beside it, the only process that links webrtc |
| `continuum-render` | bodies and scenes on a deadline | `Renderer`, `AvatarRenderer`, frame sinks | `continuum-live`'s avatar and video modules |
| `continuum-perception` | shared streams, detectors, fusion | `Fusion` leaves for scenes; stream parts | `continuum-perception`, `continuum-engine::sensory`, vision description |
| `continuum-memory` | engrams, recall, embeddings | `Search`, `Recall`, `Embedder` | `continuum-memory`, `continuum-engram`, `continuum-embedding`, `continuum-store`'s vector commands |
| `continuum-store` | the data layer and its commands | `Service` for `data/*`; ORM entities | itself, minus vectors |
| `continuum-hands` | a mind's acting tools: code, git, commands, pipelines | its tool parts | `continuum-hands`, `continuum-git`, `continuum-sentinel` |
| `continuum-grid` | discovery, trust, routing, placement across nodes | `Transport` and its kinds | `continuum-grid`, the kernel's routing transports |
| `continuum-activity` | rooms, activities, recipes, cards | `Room`, `Activity` | `continuum-experience`, `continuum-rooms` |

### Learning (layer 3)

| Crate | Owns | Hosts | Absorbs from today |
|---|---|---|---|
| `continuum-genome` | the genome object: genes, shadow, lessons, training, the foundry | `Genome` and its branches | `continuum-genome`, `continuum-dreaming`, `continuum-core::forge` |

### Minds (layer 4)

| Crate | Owns | Hosts | Absorbs from today |
|---|---|---|---|
| `continuum-mind` | the persona and her mind | `Persona`, `Mind`, `Workspace`, `Faculty`, `Amygdala`, `Wernicke`, her body branches | `continuum-mind`, `continuum-cognition`, `continuum-vitals`, `continuum-capture`, `tool-working-set` |
| `continuum-academy` | courses: lessons, exams, graders | `Course` | `continuum-academy`'s curriculum and benchmark adapters (its learning machinery moves to the genome) |

### Interfaces (layer 5) and the node (layer 6)

| Crate | Owns | Absorbs from today |
|---|---|---|
| `continuum-positron` | views for every client | itself |
| `continuum-gateway` | the door for outside agents | itself |
| `continuum-core` | composition only: the boot plan that builds the node's tree, and the binaries | everything else it holds today moves to the crate that owns it; `continuum-cli-lifecycle` moves to `apps/cli` |

### Outside the layers

- **Clients:** `client/continuum-client`, `client/continuum-client-ffi`, `apps/cli`.
- **Tools:** `source-hygiene`, `serving-replay`, `tools/manifest-gen`, `airc-test-fixtures`.
  `continuum-proving-types` folds into `serving-replay`. `jtag-mcp` becomes `apps/mcp`, a client.

## 8.3 What the layering removes

- The upper chain (cognition, academy, mind, dreaming, serving, core) becomes three layers in the
  right order: the genome below minds, minds below the Academy that teaches them.
- The kernel no longer sits above the client library.
- Each purpose in 1.1 has one crate: inference and serving in `continuum-serving`, memory and recall
  in `continuum-memory`, learning in `continuum-genome`, processes in `continuum-host` and
  `continuum-process`, home directories in `continuum-base`.
- Crate count under `core/` goes from 47 to 31: 27 in the layers, plus `livekit-bridge`,
  `source-hygiene`, `serving-replay` and `airc-test-fixtures`. The node binary holds composition only.
