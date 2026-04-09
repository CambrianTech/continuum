# Factory Pipeline UI — A High-Level Language for Model Design

The ForgeAlloy spec defines 10 stage types. The factory UI renders them as composable visual blocks. The alloy IS the high-level language — JSON is the bytecode, the pipeline composer is the IDE.

## Architecture

```
ForgeAlloy Spec (Rust)          Factory UI (TypeScript/Lit)
───────────────────────          ────────────────────────────
AlloyStage (enum)          →     StageElement (abstract base)
  ├── PruneStage           →       ├── PruneStageElement
  ├── TrainStage           →       ├── TrainStageElement
  ├── LoRAStage            →       ├── LoraStageElement
  ├── CompactStage         →       ├── CompactStageElement
  ├── QuantStage           →       ├── QuantStageElement
  ├── EvalStage            →       ├── EvalStageElement
  ├── PublishStage         →       ├── PublishStageElement
  ├── ExpertPruneStage     →       ├── ExpertPruneStageElement
  ├── ContextExtendStage   →       ├── ContextExtendStageElement
  └── ModalityStage        →       └── ModalityStageElement

ForgeAlloy.stages[]        →     PipelineComposer (visual editor)
```

The spec defines the interface. The UI implements it. Add a new stage type to the alloy → create a matching StageElement → register it → the pipeline composer discovers it automatically.

## Component Hierarchy

```
FactoryWidget (orchestrator — 333 lines)
├── ForgeControlsElement (source model, cycles, profiles)
│   └── PipelineComposer (visual stage editor)
│       ├── PruneStageElement
│       ├── TrainStageElement
│       ├── [+ Add Stage] → picks from STAGE_REGISTRY
│       └── ... any stage type
├── ActiveForgeElement (live metrics, sparkline)
└── PublishedModelsElement (leaderboard, expandable cards)

FactoryStatsWidget (right panel — independent)
├── Total downloads
├── Filters (type, sort)
├── Leaderboard bars
├── Device coverage
└── ForgeAlloy status
```

## StageElement Base Class

Every stage element extends `StageElement` which provides:

- **Shared styles** — field layouts, sliders, dropdowns, validation errors
- **Color coding** — each stage type has a unique color (prune=red, train=cyan, lora=purple, etc.)
- **Validation** — `validate()` returns error messages, constraints match the alloy JSON Schema
- **Config emission** — `stageConfig` returns the alloy-compatible JSON for this stage
- **Stage header** — type badge + order number, rendered consistently

```typescript
abstract class StageElement extends ReactiveWidget {
  abstract get stageType(): string;
  abstract get stageConfig(): Record<string, unknown>;
  validate(): string[] { return []; }
  protected emitChange(): void { /* dispatches stage-change event */ }
}
```

## PipelineComposer

The composer renders a vertical sequence of stage blocks with:

- **Add stage** — color-coded menu of available stage types
- **Remove stage** — hover action on each block
- **Reorder** — up/down arrows on hover
- **Connector lines** — visual flow between stages
- **Pipeline export** — `pipelineConfig` returns the alloy stages array

The composer doesn't know what stage types exist — it reads from `STAGE_REGISTRY`. This is the same dynamic discovery pattern as the command system.

## Commandable at Every Level

The alloy is the universal interface:

| Interface | How It Commands |
|-----------|----------------|
| **CLI** | `forge_model.py --alloy recipe.json` |
| **UI** | Drag stages in the pipeline composer |
| **API** | `Commands.execute('model/forge', alloyParams)` |
| **AI** | Persona builds alloy JSON programmatically |
| **Marketplace** | Smart contract specifies alloy, node executes it |
| **Grid** | Node receives alloy, executes, returns executed alloy |

Same alloy, every interface. The language is portable. The contract is universal.

## Analogies

- **Kerbal Space Program** — add stages, configure them, launch, watch it fly (or explode)
- **ComfyUI** — node-based visual workflow for AI, but typed and attestable
- **SCADA** — industrial control system monitoring distributed processes
- **Terraform** — declarative infrastructure as code, but for model architecture
- **Dockerfile** — reproducible build spec, but for neural networks

## The Backend: BigMama Assembly Line

The factory UI emits alloys; sentinel-ai's forge consumes them. Between
the two sits the **assembly line** — a disk-backed queue + worker that
turns the forge into a 24/7 production line. Toyota Production System
over alchemy: parts enter `intake/`, move down the line to `assembly/`,
get assayed by the eval-runner pack, and end up in `finished/` (or
`rework/` if QA flags them).

**Continuum is the shipping department.** It reads `finished/`, applies
release gates (alloy-declared minimum eval scores, security review,
branding, naming), and pushes to HuggingFace from its own auth scope.
Sentinel never pushes to HF — that's a deliberate architectural
boundary. The assembly line builds and assays; the shipping department
ships.

```
                                       ┌──────────────────────────┐
                                       │  .factory/line/          │
                  drop alloy here  →   │    intake/               │  ← from UI, CLI, generator, recipe
                                       │    assembly/  ← worker   │
                                       │    finished/  ← shipping │  ← continuum reads here
                                       │    rework/    ← QA flag  │
                                       └────────────┬─────────────┘
                                                    │
                                                    ▼
                                       FactoryWorker.process_one()
                                                    │
                                   ┌────────────────┴────────────────┐
                                   ▼                                 ▼
                          alloy_executor                       eval_runners
                          .execute_alloy()                   (registry dispatch)
                                   │                                 ▲
                                   │                                 │
                            family-adapter                  resolve_runner(name)
                            dispatch (16 adapters)                   │
                            → MoEUnfusedExpertsBase                  │
                            → MixtralAdapter                         │
                            → PhiMoEAdapter (inherits)               │
                            → DeepSeekV2Adapter                      │
                            → QwenVLAdapter                          │
                            → ... 11 more                            │
                                   │                                 │
                                   ▼                                 │
                            forged artifact  ──── assay (eval) ──→  9 real benchmark runners:
                                   │                                 HumanEval, HumanEval+,
                                   │                                 LCB v6, IFEval, BBH,
                                   │                                 MATH-Hard, GPQA,
                                   ▼                                 MMLU-Pro, MuSR
                            mark_finished()                          (Open LLM Leaderboard v2 pack)
                                   │
                                   ▼
                            .factory/line/finished/  ──→  CONTINUUM (shipping department)
                                                          • reads result manifest
                                                          • applies release gates
                                                          • pushes to HF
                                                          • posts model card
```

**Two-axis dispatch — both axes registry-driven, no shared code branches:**

- **Axis 1 — `source.architecture` → FamilyAdapter.** Each model family
  is one file in `sentinel-ai/scripts/adapters/` (16 adapters today).
  Adding a new family is one new file plus one import line. Old families
  stay frozen forever so older alloys reproduce bit-identically.
- **Axis 2 — benchmark name → BenchmarkRunner.** Each benchmark is one
  file in `sentinel-ai/scripts/eval_runners/` (9 real, 12 stubs). Adding
  a new benchmark is one new file. The §4.1.4.1 anchor-reproduction
  discipline gate routes through the same registry as production scoring.

**Sending BigMama a part to build:**

```bash
cp my-recipe.alloy.json /path/to/.factory/line/intake/
python -m factory_queue --root /path/to/.factory --max-iters 1
```

The worker picks the part off `intake/`, moves it onto `assembly/`,
runs `execute_alloy` (family-adapter dispatch), executes each stage
including `eval` (registry dispatch through the BenchmarkRunner pack),
and on success moves the alloy to `finished/` with a `.result.json`
sidecar pointing at the on-disk forged artifact and the eval results.
On any failure: `.error.json` with the full traceback in `rework/`. No
silent defaults, no retries on broken state, no f-word shortcuts.

**The filesystem IS the queue.** No DB, no service, no network
coordination. Multi-worker safety comes free if you ever need to scale
beyond a single GPU (atomic `intake → assembly` rename via `O_EXCL`).
Single-5090 case (today): one worker, one part at a time, complete
benchmark coverage per finished artifact, continuum decides when to
ship.

Code path: `sentinel-ai/scripts/factory_queue.py` (production CLI) +
`sentinel-ai/scripts/eval_runners/` (the 9 real benchmark runners) +
`sentinel-ai/scripts/adapters/` (the 16 family adapters).

## Future: Visual Pipeline Flow

The current composer is a vertical list. The eventual vision is a visual flow graph:

```
[Qwen3.5-27B] → [Prune 30%] → [Train Code 1000] → [Prune 10%] → [Train 500]
                                                                         ↓
                                              [Eval HumanEval] ← [Defrag] ← [Compact]
                                                    ↓
                                              [Quant GGUF Q4/Q8] → [Publish HF]
```

Stages as nodes. Connections as edges. Cycles as loops. The alloy JSON serializes the graph. The factory widget renders it. The forge executes it. The attestation proves it.
