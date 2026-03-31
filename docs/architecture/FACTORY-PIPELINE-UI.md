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
