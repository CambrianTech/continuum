# Continuous Learning Runtime Architecture

**Status**: Design Draft
**Builds On**: TRAINING-SYSTEM-ARCHITECTURE.md, LORA-TRAINING-STRATEGY.md
**Focus**: Runtime loop bridging Python training → Rust inference

---

## The Gap This Fills

Existing docs cover:
- ✅ Dataset generation and management
- ✅ Training orchestration (Python MLX/PEFT)
- ✅ PersonaGenome entity model

Missing:
- ❌ How trained adapters get into Rust/Candle inference
- ❌ Runtime continuous learning loop
- ❌ Hot-swap mechanism for live personas

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        CONTINUOUS LEARNING RUNTIME                          │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌──────────────┐     ┌──────────────┐     ┌──────────────────────────────┐│
│  │ Data Sources │     │   Training   │     │     Rust Inference Server    ││
│  │              │     │   Pipeline   │     │        (Candle gRPC)         ││
│  │ • Mistakes   │     │              │     │                              ││
│  │ • Corrections│────▶│ • MLX (Mac)  │────▶│ • LoadAdapter RPC            ││
│  │ • Feedback   │     │ • Unsloth    │     │ • Hot-swap adapters          ││
│  │ • Imports    │     │ • Cloud APIs │     │ • Multi-adapter composition  ││
│  └──────────────┘     └──────────────┘     └──────────────────────────────┘│
│         │                    │                         │                    │
│         │                    ▼                         │                    │
│         │           .safetensors adapter               │                    │
│         │                    │                         │                    │
│         │                    ▼                         ▼                    │
│         │  ┌─────────────────────────────────────────────────────────────┐ │
│         │  │                    Adapter Registry                         │ │
│         │  │  .continuum/genome/adapters/                                │ │
│         │  │  ├── helper-ai/                                             │ │
│         │  │  │   ├── typescript-v1.2.safetensors                        │ │
│         │  │  │   ├── chat-style-v3.1.safetensors                        │ │
│         │  │  │   └── manifest.json                                      │ │
│         │  │  └── shared/                                                │ │
│         │  │      └── coding-standards-v1.0.safetensors                  │ │
│         │  └─────────────────────────────────────────────────────────────┘ │
│         │                                                                   │
│         └──────────────────── Feedback Loop ────────────────────────────────┘
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Component Details

### 1. Data Accumulator

Collects training signal from live interactions:

```typescript
interface TrainingSignal {
  type: 'mistake' | 'correction' | 'positive' | 'import';
  personaId: string;
  domain: string;           // 'typescript', 'chat', 'wine', etc.
  input: string;            // What the AI saw
  wrongOutput?: string;     // What it said (if mistake)
  correctOutput: string;    // What it should have said
  context?: string;         // Surrounding conversation
  source: 'human' | 'ai' | 'automated';
  timestamp: number;
  weight: number;           // 1.0 = gold (human), 0.5 = silver (ai peer)
}
```

**Collection points:**
- Human corrections in chat ("Actually, you should...")
- Thumbs down reactions
- Failed tool calls
- Peer AI corrections
- Imported datasets

**Storage:**
```
.continuum/genome/training-buffer/
├── helper-ai/
│   ├── pending/
│   │   ├── 2025-01-06-001.jsonl  # Today's signals
│   │   └── 2025-01-06-002.jsonl
│   └── queued/
│       └── batch-2025-01-06.jsonl  # Ready for training
└── manifest.json
```

### 2. Training Trigger

Decides when to train:

```typescript
interface TrainingTrigger {
  type: 'threshold' | 'scheduled' | 'manual' | 'quality-drop';

  // Threshold: train after N examples
  thresholdConfig?: {
    minExamples: number;      // e.g., 500
    maxWaitHours: number;     // e.g., 24 (train anyway if waiting too long)
  };

  // Scheduled: train at specific times
  scheduleConfig?: {
    cron: string;             // e.g., "0 3 * * *" (3am daily)
    onlyIfData: boolean;      // Skip if no new data
  };

  // Quality drop: train when performance degrades
  qualityConfig?: {
    metricName: string;       // e.g., 'user_satisfaction'
    threshold: number;        // e.g., 0.8
    windowHours: number;      // e.g., 24
  };
}
```

**Default strategy:**
```typescript
const defaultTrigger: TrainingTrigger = {
  type: 'threshold',
  thresholdConfig: {
    minExamples: 500,        // Train after 500 corrections
    maxWaitHours: 168,       // Or weekly, whichever comes first
  }
};
```

### 3. Training Executor

Runs the actual training job:

```typescript
interface TrainingJob {
  jobId: string;
  personaId: string;
  domain: string;

  // Input
  datasetPath: string;       // .jsonl file
  baseAdapter?: string;      // Previous version to continue from

  // Config
  provider: 'mlx' | 'unsloth' | 'fireworks';
  hyperparameters: {
    rank: number;            // LoRA rank (8 for style, 32 for knowledge)
    alpha: number;           // Typically 2x rank
    epochs: number;          // 1-3 for incremental
    learningRate: number;    // 2e-4 typical
  };

  // Output
  outputPath: string;        // Where to save .safetensors

  // Status
  status: 'pending' | 'running' | 'completed' | 'failed';
  progress?: number;         // 0-100
  metrics?: TrainingMetrics;
}
```

**Execution flow:**
```bash
# 1. Prepare dataset
./jtag training/prepare \
  --input .continuum/genome/training-buffer/helper-ai/queued/*.jsonl \
  --output /tmp/training-batch.jsonl

# 2. Run training (MLX on Mac)
python scripts/train-lora-mlx.py \
  --base-model "mlx-community/Llama-3.2-3B-Instruct-4bit" \
  --dataset /tmp/training-batch.jsonl \
  --output .continuum/genome/adapters/helper-ai/typescript-v1.3.safetensors \
  --continue-from .continuum/genome/adapters/helper-ai/typescript-v1.2.safetensors \
  --rank 32 --alpha 64 --epochs 1

# 3. Validate (quick quality check)
./jtag training/validate \
  --adapter .continuum/genome/adapters/helper-ai/typescript-v1.3.safetensors \
  --test-set /datasets/typescript-test.jsonl \
  --min-quality 0.8

# 4. Deploy to inference server (HOT SWAP)
./jtag inference/load-adapter \
  --persona helper-ai \
  --adapter typescript-v1.3 \
  --replace typescript-v1.2
```

### 4. Rust Inference Integration (The Bridge)

**Current state:** gRPC server has `LoadAdapter` RPC but only tracks metadata.

**Required implementation:**

```rust
// workers/inference-grpc/src/lora.rs

use candle_core::{Tensor, DType, Device};
use candle_nn::VarBuilder;

/// Load LoRA adapter weights from safetensors file
pub fn load_lora_weights(
    adapter_path: &str,
    device: &Device,
    dtype: DType,
) -> Result<LoRAWeights, Box<dyn std::error::Error>> {
    let weights = candle_core::safetensors::load(adapter_path, device)?;

    // LoRA weights are typically named:
    // - lora_A.{layer}.weight
    // - lora_B.{layer}.weight

    let mut lora_weights = LoRAWeights::new();

    for (name, tensor) in weights {
        if name.contains("lora_A") {
            let layer = extract_layer_name(&name);
            lora_weights.add_a(layer, tensor.to_dtype(dtype)?);
        } else if name.contains("lora_B") {
            let layer = extract_layer_name(&name);
            lora_weights.add_b(layer, tensor.to_dtype(dtype)?);
        }
    }

    Ok(lora_weights)
}

/// Apply LoRA to base model weights
/// Formula: W' = W + scale * (B @ A)
pub fn apply_lora(
    base_weight: &Tensor,
    lora_a: &Tensor,      // [rank, in_features]
    lora_b: &Tensor,      // [out_features, rank]
    scale: f64,
) -> Result<Tensor, candle_core::Error> {
    let delta = lora_b.matmul(lora_a)?;
    let scaled = (delta * scale)?;
    base_weight.add(&scaled)
}
```

**Integration with existing Llama model:**

```rust
// Modified forward pass with LoRA
impl ModelState {
    fn forward_with_lora(
        &self,
        input: &Tensor,
        pos: usize,
        adapters: &[LoadedLoRA],
    ) -> Result<Tensor, String> {
        // For each layer that has LoRA adapters:
        // 1. Get base weight
        // 2. Apply each adapter: W' = W + sum(scale_i * B_i @ A_i)
        // 3. Use modified weight for forward pass

        // Note: Can pre-merge adapters at load time for efficiency
        // Only re-merge when adapter set changes
    }
}
```

### 5. Hot-Swap Protocol

**Goal:** Zero-downtime adapter updates while persona is running.

```
Timeline:
─────────────────────────────────────────────────────────────────────
t0: Persona running with typescript-v1.2
t1: Training completes → typescript-v1.3 ready
t2: LoadAdapter RPC called with typescript-v1.3
t3: Server loads new weights (1-2 seconds)
t4: Server atomically swaps active adapter
t5: Next inference uses v1.3
t6: Old v1.2 marked for unload (after grace period)
─────────────────────────────────────────────────────────────────────
```

**gRPC protocol:**

```protobuf
// Already defined in inference.proto
rpc LoadAdapter(LoadAdapterRequest) returns (LoadAdapterResponse);

message LoadAdapterRequest {
  string adapter_path = 1;   // Full path to .safetensors
  string adapter_id = 2;     // Unique ID (e.g., "typescript-v1.3")
  double scale = 3;          // LoRA scale factor
  string replace_id = 4;     // Optional: adapter to replace atomically
}
```

---

## Continuous Learning Daemon

**TrainingDaemon** orchestrates the full loop:

```typescript
// daemons/training-daemon/server/TrainingDaemonServer.ts

export class TrainingDaemonServer extends BaseDaemonServer {
  private accumulators: Map<string, DataAccumulator> = new Map();
  private triggers: Map<string, TrainingTrigger> = new Map();

  async onStart(): Promise<void> {
    // Subscribe to training signals
    Events.subscribe('persona:mistake', this.onMistake.bind(this));
    Events.subscribe('persona:correction', this.onCorrection.bind(this));
    Events.subscribe('persona:feedback', this.onFeedback.bind(this));

    // Start trigger check loop
    this.startTriggerLoop();
  }

  private async onCorrection(signal: TrainingSignal): Promise<void> {
    const accumulator = this.getAccumulator(signal.personaId, signal.domain);
    await accumulator.add(signal);

    // Check if threshold reached
    if (await this.shouldTrain(signal.personaId, signal.domain)) {
      await this.queueTrainingJob(signal.personaId, signal.domain);
    }
  }

  private async executeTrainingJob(job: TrainingJob): Promise<void> {
    // 1. Prepare dataset
    const datasetPath = await this.prepareDataset(job);

    // 2. Run training (spawn Python process)
    const result = await this.runTraining(job, datasetPath);

    // 3. Validate quality
    const quality = await this.validateAdapter(result.adapterPath);

    if (quality >= job.minQuality) {
      // 4. Deploy to inference server
      await this.deployAdapter(job.personaId, result.adapterPath);

      // 5. Archive old version
      await this.archiveOldAdapter(job.personaId, job.domain);
    } else {
      // Quality regression - don't deploy
      console.warn(`Training produced lower quality (${quality}), keeping old adapter`);
    }
  }
}
```

---

## File System Layout

```
.continuum/
├── genome/
│   ├── adapters/                    # Trained LoRA adapters
│   │   ├── helper-ai/
│   │   │   ├── typescript-v1.2.safetensors
│   │   │   ├── typescript-v1.3.safetensors  # Latest
│   │   │   ├── chat-style-v3.1.safetensors
│   │   │   └── manifest.json        # Active versions
│   │   └── shared/                  # Cross-persona adapters
│   │       └── coding-standards-v1.0.safetensors
│   │
│   ├── training-buffer/             # Pending training data
│   │   ├── helper-ai/
│   │   │   ├── pending/             # Accumulating
│   │   │   └── queued/              # Ready for training
│   │   └── manifest.json
│   │
│   ├── training-jobs/               # Job state
│   │   ├── active/
│   │   │   └── job-2025-01-06-001.json
│   │   └── completed/
│   │       └── job-2025-01-05-001.json
│   │
│   └── checkpoints/                 # Training checkpoints
│       └── helper-ai-typescript-v1.3/
│           ├── checkpoint-500.safetensors
│           └── checkpoint-1000.safetensors
│
└── personas/
    └── helper/
        └── genome.json              # Points to active adapters
```

---

## Implementation Phases

### Phase 1: LoRA Loading in Rust (Current Priority)
- [ ] Implement `load_lora_weights()` in Candle
- [ ] Add `apply_lora()` for weight merging
- [ ] Wire up `LoadAdapter` RPC to actually load weights
- [ ] Test with a pre-trained adapter

### Phase 2: Data Accumulation
- [ ] Create TrainingSignal entity
- [ ] Implement DataAccumulator
- [ ] Add correction detection in chat
- [ ] Store signals in training-buffer

### Phase 3: Training Automation
- [ ] Create TrainingDaemon
- [ ] Implement threshold-based triggers
- [ ] Wrap MLX training script
- [ ] Add quality validation

### Phase 4: Hot-Swap Deployment
- [ ] Implement atomic adapter swap
- [ ] Add version management
- [ ] Implement rollback mechanism
- [ ] Add monitoring/alerting

### Phase 5: Feedback Loop
- [ ] Track adapter performance over time
- [ ] Detect quality regressions
- [ ] Auto-rollback on regression
- [ ] Dashboard for training status

---

## Commands

```bash
# Data management
./jtag training/buffer/status           # Show pending data counts
./jtag training/buffer/flush --persona=helper-ai  # Force queue data

# Training
./jtag training/start --persona=helper-ai --domain=typescript
./jtag training/status                  # Show active jobs
./jtag training/stop --job=job-001

# Adapters
./jtag adapter/list --persona=helper-ai
./jtag adapter/deploy --path=./adapter.safetensors --persona=helper-ai
./jtag adapter/rollback --persona=helper-ai --domain=typescript

# Continuous learning
./jtag training/continuous/start --persona=helper-ai
./jtag training/continuous/stop --persona=helper-ai
./jtag training/continuous/status
```

---

## Success Metrics

1. **Training latency**: < 10 minutes for incremental update (500 examples)
2. **Hot-swap latency**: < 2 seconds for adapter swap
3. **Quality retention**: New adapter >= 95% of old adapter quality
4. **Data efficiency**: Measurable improvement from 500 corrections
5. **Uptime**: Zero inference downtime during adapter updates

---

## References

- [TRAINING-SYSTEM-ARCHITECTURE.md](TRAINING-SYSTEM-ARCHITECTURE.md) - Full training system design
- [LORA-TRAINING-STRATEGY.md](LORA-TRAINING-STRATEGY.md) - Training approaches and costs
- [COLLABORATIVE-LEARNING-VISION.md](COLLABORATIVE-LEARNING-VISION.md) - Multi-layer learning loop
- [docs/genome/DYNAMIC-GENOME-ARCHITECTURE.md](genome/DYNAMIC-GENOME-ARCHITECTURE.md) - PersonaGenome design
