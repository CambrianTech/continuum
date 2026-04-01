# ForgeAlloy — Portable Pipeline Entity Specification

**Issue**: #659
**Status**: Design
**Packages**: `continuum-alloy` (crate, pip), `@continuum-ai/alloy` (npm)

---

## What Is An Alloy?

An alloy is a **complete, portable, typed definition** of how to transform a base model into a specialized one. It's a pipeline of stages, stored as JSON, executable by any factory node with the right hardware.

- **Not a flat config** — it's an ordered pipeline of typed stages
- **Not YAML** — JSON always, stored in our ORM as an entity
- **Portable** — publish alongside models on HuggingFace, anyone imports and runs
- **Composable** — mix and match stages for different forge profiles
- **Traceable** — lineage chain via `sourceAlloyId` enables re-forging

---

## Entity Definition

```typescript
/**
 * ForgeAlloy — The complete specification for producing a model.
 *
 * Stored in ORM. Serialized to JSON for portability.
 * The JSON IS the documentation — everything needed to reproduce.
 */
interface ForgeAlloy extends BaseEntity {
  // ── Identity ──────────────────────────────────
  name: string;                   // "qwen3.5-4b-code-aggressive"
  version: string;                // semver: "1.0.0"
  description: string;            // Human-readable summary
  author: string;                 // "continuum-ai" or username
  tags: string[];                 // ["code", "pruning", "4b", "fast"]
  license: string;                // "apache-2.0"

  // ── Source Model ──────────────────────────────
  source: AlloySource;

  // ── Pipeline ──────────────────────────────────
  stages: AlloyStage[];           // Ordered pipeline steps
  cycles: number;                 // Repeat prune→train N times (1 = single pass)

  // ── Hardware Requirements ─────────────────────
  hardware: AlloyHardware;

  // ── Outputs ───────────────────────────────────
  outputs: AlloyOutputs;

  // ── Lineage ───────────────────────────────────
  sourceAlloyId?: UUID;           // Parent alloy (for re-forge chains)
  forgedModelIds?: string[];      // HF model IDs produced by this alloy
}
```

---

## Source

```typescript
interface AlloySource {
  baseModel: string;              // "Qwen/Qwen3.5-4B"
  architecture: string;           // "qwen3_5", "llama", "mistral"
  revision?: string;              // HF revision/branch
  isMoE: boolean;                 // Mixture of Experts model
  totalExperts?: number;          // Total experts if MoE
}
```

---

## Stages

Each stage is a discriminated union — `type` determines which config applies.

```typescript
type AlloyStage =
  | PruneStage
  | TrainStage
  | LoRAStage
  | CompactStage
  | QuantStage
  | EvalStage
  | PublishStage
  | ExpertPruneStage
  | ContextExtendStage
  | ModalityStage;
```

### PruneStage — Head pruning

```typescript
interface PruneStage {
  type: 'prune';
  strategy: 'entropy' | 'magnitude' | 'gradient' | 'random';
  level: number;                  // 0.0-0.7, fraction of heads to prune
  minHeadsPerLayer: number;       // Safety floor (default: 4)
  minKvHeadsPerLayer: number;     // KV head safety floor (default: 2)
  analysisSteps?: number;         // Steps to collect entropy data
}
```

### TrainStage — Recovery/fine-tuning

```typescript
interface TrainStage {
  type: 'train';
  domain: string;                 // "code", "reasoning", "general", "math"
  dataset?: string;               // HF dataset ID or local path
  steps: number;                  // Training steps per cycle
  learningRate: string;           // "2e-4", "5e-5"
  batchSize: number;              // 1-64 (default: 4)
  gradientAccumulation: number;   // 1-16 (default: 1)
  scheduler: 'cosine' | 'linear' | 'constant' | 'constant_with_warmup';
  warmupRatio: number;            // 0.0-1.0 (default: 0.03)
  weightDecay: number;            // 0.0-1.0 (default: 0.01)
  maxGradientNorm: number;        // Gradient clipping (default: 1.0)
  precision: 'bf16' | 'fp16' | 'fp32';
  sequenceLength: number;         // Max tokens per sample (default: 2048)
  optimizations: Optimization[];  // flash_attention, gradient_checkpointing, etc.
}

type Optimization =
  | 'flash_attention'
  | 'gradient_checkpointing'
  | 'optimizer_8bit'
  | 'paged_optimizer'
  | 'fused_kernels'
  | 'sequence_packing'
  | 'dynamic_padding'
  | 'activation_offloading';
```

### LoRAStage — Adapter training

```typescript
interface LoRAStage {
  type: 'lora';
  rank: number;                   // 1-256 (default: 32)
  alpha: number;                  // Scaling, typically 2*rank
  dropout: number;                // 0.0-1.0 (default: 0.05)
  targetModules: string[];        // ["q_proj", "k_proj", "v_proj", "o_proj"]
  quantize: boolean;              // QLoRA (default: true)
  quantizeBits: 4 | 8;           // NF4 or INT8 (default: 4)
  dataset?: string;               // Override training data
  epochs: number;                 // 1-20 (default: 3)
  learningRate: string;           // Default: "1e-4"
  batchSize: number;              // Default: 4
  mergeAfter: boolean;            // Fold adapter into base weights (default: false)
}
```

### CompactStage — Plasticity-based compaction

```typescript
interface CompactStage {
  type: 'compact';
  deadThreshold: number;          // Below: physically removed (default: 0.1)
  dormantThreshold: number;       // Below: ternary 1.58-bit (default: 0.2)
  lowThreshold: number;           // Below: Q2 2-bit (default: 0.3)
  mediumThreshold: number;        // Below: Q4 4-bit (default: 0.5)
  highThreshold: number;          // Below: Q8 8-bit (default: 0.7)
  targetSizeGb?: number;          // Dynamic threshold adjustment
  enableQuantization: boolean;    // Actually apply mixed-precision (default: true)
}
```

### QuantStage — Output quantization

```typescript
interface QuantStage {
  type: 'quant';
  format: 'gguf' | 'mlx' | 'safetensors' | 'onnx';
  quantTypes: GgufQuantType[];    // ["Q4_K_M", "Q8_0"] — multiple outputs
  deviceTargets: string[];        // ["macbookair", "iphone", "5090"]
}

type GgufQuantType =
  | 'Q2_K' | 'Q3_K_S' | 'Q3_K_M' | 'Q3_K_L'
  | 'IQ4_XS' | 'Q4_K_S' | 'Q4_K_M'
  | 'Q5_K_S' | 'Q5_K_M'
  | 'Q6_K' | 'Q8_0'
  | 'F16' | 'F32';
```

### EvalStage — Benchmarking

```typescript
interface EvalStage {
  type: 'eval';
  benchmarks: Benchmark[];
  passingThreshold?: number;      // Minimum score to continue pipeline
  compareToBase: boolean;         // Show improvement vs base model (default: true)
}

interface Benchmark {
  name: string;                   // "humaneval", "mmlu", "gsm8k", "livecodebench"
  subset?: string;                // "humaneval-plus", "mmlu-pro"
  nShot?: number;                 // Few-shot count
  submitToLeaderboard: boolean;   // Auto-submit to HF leaderboard
}
```

### PublishStage — HuggingFace distribution

```typescript
interface PublishStage {
  type: 'publish';
  org: string;                    // "continuum-ai"
  repoNameTemplate: string;       // "{base}-{domain}-{variant}"
  includeAlloy: boolean;          // Include alloy.json in repo (default: true)
  cardFromBenchmarks: boolean;    // Generate card from EvalStage results
  tags: string[];                 // Additional HF tags
  private: boolean;               // Private repo (default: false)
}
```

### ExpertPruneStage — MoE expert pruning

```typescript
interface ExpertPruneStage {
  type: 'expert-prune';
  keepExperts: number;            // Number of experts to retain
  selectionStrategy: 'activation' | 'gradient' | 'random';
  profileDataset?: string;        // Dataset for activation profiling
  profileSteps: number;           // Steps to profile (default: 100)
}
```

### ContextExtendStage — RoPE rescaling (#648)

```typescript
interface ContextExtendStage {
  type: 'context-extend';
  targetLength: number;           // Target context window (e.g., 131072)
  method: 'yarn' | 'ntk' | 'linear' | 'dynamic-ntk';
  trainingDataset: string;        // Long-context training data
  trainingSteps: number;          // Steps for position interpolation
}
```

### ModalityStage — Add vision/audio (#649, #650)

```typescript
interface ModalityStage {
  type: 'modality';
  modality: 'vision' | 'audio' | 'multimodal';
  encoderModel: string;           // "siglip-400m", "whisper-small"
  projectionArch: 'mlp' | 'cross-attention' | 'linear';
  freezeBase: boolean;            // Freeze LLM weights (default: true)
  freezeEncoder: boolean;         // Freeze encoder weights (default: true)
  trainingDataset: string;        // Image-caption pairs, audio-text pairs
  trainingSteps: number;
  projectionDim?: number;         // Override projection dimension
}
```

---

## Hardware Requirements

```typescript
interface AlloyHardware {
  minVramGb: number;              // Minimum GPU VRAM to run this alloy
  recommendedVramGb: number;      // Recommended for reasonable speed
  estimatedDurationMinutes: number; // Rough estimate for recommended hardware
  supportsCPU: boolean;           // Can run on CPU-only (slow but works)
  testedOn?: string[];            // ["RTX 5090", "A100", "M3 Max"]
}
```

---

## Outputs

```typescript
interface AlloyOutputs {
  produces: OutputArtifact[];
}

interface OutputArtifact {
  type: 'safetensors' | 'gguf' | 'mlx' | 'lora-adapter' | 'model-card' | 'alloy';
  description: string;
}
```

---

## Example Alloy JSON

```json
{
  "name": "qwen3.5-4b-code-balanced",
  "version": "1.0.0",
  "description": "Balanced code forge for Qwen3.5-4B. 30% head pruning, 3 cycles, entropy strategy. Produces GGUF for MacBook Air and iPhone.",
  "author": "continuum-ai",
  "tags": ["code", "qwen3.5", "4b", "balanced", "macbook", "iphone"],
  "license": "apache-2.0",

  "source": {
    "baseModel": "Qwen/Qwen3.5-4B",
    "architecture": "qwen3_5",
    "isMoE": false
  },

  "stages": [
    {
      "type": "prune",
      "strategy": "entropy",
      "level": 0.3,
      "minHeadsPerLayer": 4,
      "minKvHeadsPerLayer": 2,
      "analysisSteps": 200
    },
    {
      "type": "train",
      "domain": "code",
      "dataset": "m-a-p/CodeFeedback-Filtered-Instruction",
      "steps": 1000,
      "learningRate": "2e-4",
      "batchSize": 4,
      "gradientAccumulation": 4,
      "scheduler": "cosine",
      "warmupRatio": 0.03,
      "weightDecay": 0.01,
      "maxGradientNorm": 1.0,
      "precision": "bf16",
      "sequenceLength": 2048,
      "optimizations": ["flash_attention", "gradient_checkpointing"]
    },
    {
      "type": "compact",
      "deadThreshold": 0.1,
      "dormantThreshold": 0.2,
      "lowThreshold": 0.3,
      "mediumThreshold": 0.5,
      "highThreshold": 0.7,
      "enableQuantization": true
    },
    {
      "type": "quant",
      "format": "gguf",
      "quantTypes": ["Q4_K_M", "Q8_0"],
      "deviceTargets": ["macbookair", "iphone"]
    },
    {
      "type": "eval",
      "benchmarks": [
        { "name": "humaneval", "submitToLeaderboard": true },
        { "name": "mmlu", "subset": "mmlu-pro", "nShot": 5, "submitToLeaderboard": false }
      ],
      "passingThreshold": 60,
      "compareToBase": true
    },
    {
      "type": "publish",
      "org": "continuum-ai",
      "repoNameTemplate": "{base}-{domain}-forged",
      "includeAlloy": true,
      "cardFromBenchmarks": true,
      "tags": ["continuum", "forged", "experiential-plasticity"],
      "private": false
    }
  ],

  "cycles": 3,

  "hardware": {
    "minVramGb": 8,
    "recommendedVramGb": 24,
    "estimatedDurationMinutes": 45,
    "supportsCPU": false,
    "testedOn": ["RTX 5090", "RTX 4090"]
  },

  "outputs": {
    "produces": [
      { "type": "safetensors", "description": "Forged base weights" },
      { "type": "gguf", "description": "Q4_K_M and Q8_0 quantized" },
      { "type": "model-card", "description": "Generated from benchmark results" },
      { "type": "alloy", "description": "This alloy definition (for reproducibility)" }
    ]
  }
}
```

---

## Packages

### Rust Crate (`continuum-alloy`)

Source of truth. All types defined here with `#[derive(TS, Serialize, Deserialize)]`.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ForgeAlloy {
    pub name: String,
    pub version: String,
    pub source: AlloySource,
    pub stages: Vec<AlloyStage>,
    pub cycles: u32,
    pub hardware: AlloyHardware,
    pub outputs: AlloyOutputs,
    // ...
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type")]
pub enum AlloyStage {
    #[serde(rename = "prune")]
    Prune(PruneStage),
    #[serde(rename = "train")]
    Train(TrainStage),
    // ...
}
```

### Python Package (`continuum-alloy`)

```python
from continuum_alloy import ForgeAlloy, PruneStage, TrainStage

# Load from JSON
alloy = ForgeAlloy.from_file("alloy.json")

# Access typed stages
for stage in alloy.stages:
    if isinstance(stage, PruneStage):
        print(f"Pruning {stage.level*100}% with {stage.strategy}")
    elif isinstance(stage, TrainStage):
        print(f"Training {stage.steps} steps at lr={stage.learning_rate}")

# Validate
alloy.validate()  # Raises if invalid

# Save
alloy.to_file("my-alloy.json")
```

### npm Package (`@continuum-ai/alloy`)

Generated via ts-rs from Rust. Zero hand-written types.

```typescript
import { ForgeAlloy, validateAlloy } from '@continuum-ai/alloy';

const alloy: ForgeAlloy = JSON.parse(fs.readFileSync('alloy.json', 'utf-8'));
validateAlloy(alloy);  // throws if invalid
```

---

## Commands

| Command | What |
|---------|------|
| `forge/alloy/create` | Create from params or JSON import |
| `forge/alloy/list` | List stored alloys |
| `forge/alloy/get` | Get alloy by ID or name |
| `forge/alloy/import` | Import from JSON file or HF model repo |
| `forge/alloy/export` | Export to JSON file |
| `forge/alloy/run` | Execute pipeline on a grid node |
| `forge/alloy/validate` | Validate without running |
| `forge/alloy/fork` | Clone and modify (sets sourceAlloyId) |

---

## Integration Points

- **Factory Widget**: alloy picker dropdown, stage editor, run button
- **Model Cards**: alloy.json embedded in every published model repo
- **Grid**: alloy sent to target node, executed stage by stage
- **Sentinel**: sentinel pipelines can reference alloys by ID
- **Academy**: training stages can reference academy curricula
- **Re-forge**: fork an alloy, adjust stages, run again (lineage tracked)

---

## Related Issues

- #655 — Master lifecycle pipeline
- #657 — Re-forge from known provenance
- #651 — Recipe composition (now: alloy stage composition)
- #646 — Python↔Rust bridge (protobuf for alloy transport)
- #648-650 — Context/vision/audio stages
- #658 — Sentinel forge recipe (uses alloys)
