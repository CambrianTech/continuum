# Training System Quick Reference

**Quick guide for working with the LoRA fine-tuning training system**

---

## Commands Overview

### Dataset Preparation

```bash
# Generate JSONL from git history
./jtag ai/dataset/create --project=continuum-git
# → /datasets/parsed/continuum-git-2025-11-08.jsonl

# Import JSONL to SQLite (queryable)
./jtag training/prepare --datasetPath=/datasets/parsed/continuum-git-*.jsonl
# → { dbHandle: 'training-db-001', examplesImported: 1420 }

# Query prepared dataset
./jtag data/list --dbHandle=training-db-001 --collection=training_examples \
  --filter='{"quality": {"$gte": 0.8}, "used": false}' --limit=10
```

### Training Session

```bash
# Start training (returns immediately with sessionId)
./jtag training/start \
  --personaId=<uuid> \
  --provider=peft \
  --baseModel=TinyLlama/TinyLlama-1.1B-Chat-v1.0 \
  --datasetDbHandle=training-db-001 \
  --datasetFilter='{"quality": {"$gte": 0.8}, "used": false}' \
  --maxExamples=500 \
  --hyperparameters='{"rank": 32, "epochs": 3, "learningRate": 0.0001}'
# → { sessionId: '550e8400-...', status: 'queued', estimatedDuration: 1200000 }

# Check status
./jtag training/status --sessionId=550e8400-...
# → { status: 'training', progress: 0.35, currentEpoch: 2 }

# View metrics
./jtag training/metrics --sessionId=550e8400-...
# → { metrics: [...], summary: { minLoss: 0.82, finalLoss: 0.85 } }

# Stop training
./jtag training/stop --sessionId=550e8400-... --saveCheckpoint=true

# List all sessions
./jtag training/list --personaId=<uuid> --status=completed
```

### Adapter Deployment

```bash
# Test adapter quality
./jtag adapter/test --sessionId=550e8400-... --sampleCount=20
# → { metrics: {...}, recommendation: 'deploy' }

# Deploy to genome
./jtag adapter/deploy \
  --sessionId=550e8400-... \
  --adapterName=helper-ai-typescript-expertise \
  --deployToGenome=true \
  --layerWeight=1.0
# → { genomeLayerId: '660e8400-...', adapterSize: 52428800 }

# Check genome status
./jtag ai/genome/stats --personaId=<uuid>
```

---

## Directory Structure

```
/datasets/
├── raw/                    # Source data (git repos, etc.)
├── parsed/                 # JSONL files from ai/dataset/create
│   └── continuum-git-2025-11-08.jsonl
└── prepared/               # SQLite databases for training
    └── continuum-git.sqlite

.continuum/genome/
├── adapters/               # Deployed adapters (production)
│   └── <layer-id>/
│       ├── adapter_model.safetensors
│       └── adapter_config.json
└── training/               # Training outputs (temporary)
    └── <session-id>/
        ├── dataset.jsonl
        ├── adapter/
        ├── checkpoints/
        └── logs/
```

---

## Workflow Diagram

```
┌──────────────────────────────────────────────────────────────┐
│ 1. Dataset Preparation                                       │
└──────────────────────────────────────────────────────────────┘
   ai/dataset/create → JSONL file
   training/prepare → SQLite database
   data/list → Query examples

┌──────────────────────────────────────────────────────────────┐
│ 2. Training (Async)                                          │
└──────────────────────────────────────────────────────────────┘
   training/start → Returns sessionId immediately
   training/status → Poll for progress
   training/metrics → View loss curves
   Training completes → Adapter saved

┌──────────────────────────────────────────────────────────────┐
│ 3. Deployment                                                │
└──────────────────────────────────────────────────────────────┘
   adapter/test → Validate quality
   adapter/deploy → Create GenomeLayerEntity
   ai/genome/stats → Verify deployment
```

---

## Entity Reference

### TrainingSessionEntity

**Collection**: `training_sessions`

**Key Fields**:
- `personaId`: UUID - PersonaUser being trained
- `provider`: 'peft' | 'mlx' | 'openai' | 'anthropic' | 'deepseek'
- `baseModel`: string - Base model name
- `status`: TrainingStatus - Current state
- `progress`: number - 0.0-1.0
- `datasetDbHandle`: string - Database handle for dataset
- `hyperparameters`: object - LoRA rank, epochs, learning rate, etc.
- `adapterPath`: string - Path to trained adapter (local)
- `genomeLayerId`: UUID - Created layer (after deployment)
- `finalLoss`: number - Training loss
- `trainingDuration`: number - Milliseconds

### TrainingMetricsEntity

**Collection**: `training_metrics`

**Key Fields**:
- `sessionId`: UUID - Parent training session
- `timestamp`: number
- `epoch`: number
- `step`: number
- `trainLoss`: number
- `validationLoss`: number
- `learningRate`: number

### GenomeLayerEntity

**Collection**: `genome_layers` (existing)

**Key Fields**:
- `name`: string - Human-readable name
- `traitType`: TraitType - 'conversational' | 'expertise' | 'personality'
- `adapterPath`: string - Path to safetensors file
- `baseModel`: string
- `rank`: number - LoRA rank
- `alpha`: number - LoRA alpha
- `embedding`: number[] - 768-dim vector for similarity search

---

## Provider Comparison

| Provider | Training Location | Cost | Speed | Adapter Download |
|----------|-------------------|------|-------|------------------|
| **PEFT** | Local (GPU/CPU) | Free | Medium | ✅ Yes (safetensors) |
| **MLX** | Local (Apple Silicon) | Free | Fast | ✅ Yes (safetensors) |
| **OpenAI** | Cloud API | $3-8/M tokens | Slow | ❌ No (API-hosted) |
| **DeepSeek** | Cloud API | ~$1/M tokens | Medium | ❌ No (API-hosted) |
| **Anthropic** | Cloud API (Bedrock) | TBD | Medium | ❌ No (API-hosted) |

**Recommendation**: Start with **PEFT** for local development, use **OpenAI** for production quality.

---

## Hyperparameters Guide

### LoRA Rank (8-256)
- **Low (8-16)**: Fast, small adapters (~10MB), less flexible
- **Medium (32-64)**: Good balance, recommended default
- **High (128-256)**: Slow, large adapters (~100MB), very flexible

### Epochs (1-100)
- **1-2**: Quick iteration, may underfit
- **3-5**: Good default, most use cases
- **10+**: Risk of overfitting, use with large datasets

### Learning Rate (0.00001-0.001)
- **Low (0.00001)**: Stable, slow convergence
- **Medium (0.0001)**: Good default
- **High (0.001)**: Fast, risk of instability

### Batch Size (1-32)
- **Small (1-2)**: Low memory, slower training
- **Medium (4-8)**: Good balance
- **Large (16-32)**: High memory, faster training

**Recommended Defaults**:
```json
{
  "rank": 32,
  "alpha": 32,
  "epochs": 3,
  "learningRate": 0.0001,
  "batchSize": 4
}
```

---

## Storage Architecture

### Option A: JSONL-Only (Simple)
```
JSONL → Trainer → Adapter
```
- No conversion step
- No querying capability
- Cannot mark examples as "used"

### Option B: JSONL + SQLite (Recommended)
```
JSONL → training/prepare → SQLite → Trainer → Adapter
```
- Queryable with filters
- Can mark examples as "used"
- Supports continuous learning
- Multi-database handle integration

**Why SQLite?**
1. Continuous learning requires tracking "used" examples
2. Quality filtering essential for good training
3. Multi-database handle system already exists
4. One-time conversion cost, ongoing querying benefit

---

## Continuous Learning Flow

```
PersonaUser interactions (100+)
         ↓
PersonaUser.generateSelfTasks()
         ↓
Creates training task
         ↓
Processes task → training/start
         ↓
Training runs async
         ↓
PersonaUser.onTrainingCompleted()
         ↓
adapter/test → adapter/deploy
         ↓
New layer active in genome
```

**Triggers**:
- **Threshold**: 100+ unused interactions
- **Time**: Every 30 days
- **Quality**: Drop in performance metrics

---

## Testing Strategy

### Unit Tests
```bash
npx vitest tests/unit/training-session-entity.test.ts
npx vitest tests/unit/training-orchestrator.test.ts
```

### Integration Tests
```bash
# Small dataset (100 examples, TinyLlama, 1 epoch = ~5 minutes)
npx vitest tests/integration/training-end-to-end.test.ts

# Full workflow
npx vitest tests/integration/adapter-deployment.test.ts
```

### Manual Testing
```bash
# Quick test with TinyLlama (5-10 minutes)
./jtag training/start \
  --provider=peft \
  --baseModel=TinyLlama/TinyLlama-1.1B-Chat-v1.0 \
  --datasetDbHandle=training-db-001 \
  --maxExamples=100 \
  --hyperparameters='{"epochs": 1}'
```

---

## Common Issues

### Training Fails with "Python environment not bootstrapped"

**Solution**:
```bash
cd .continuum/genome/python
bash bootstrap.sh
```

### Training OOM (Out of Memory)

**Solutions**:
1. Reduce batch size: `--hyperparameters='{"batchSize": 1}'`
2. Reduce rank: `--hyperparameters='{"rank": 8}'`
3. Use smaller model: `--baseModel=TinyLlama/TinyLlama-1.1B-Chat-v1.0`
4. Use QLoRA (4-bit quantization) - coming in Phase 7.2+

### Training Too Slow

**Solutions**:
1. Increase batch size: `--hyperparameters='{"batchSize": 16}'`
2. Use GPU (CUDA or Apple Silicon)
3. Reduce examples: `--maxExamples=500`
4. Use MLX provider (Apple Silicon only)

### Adapter Quality Poor

**Solutions**:
1. Increase epochs: `--hyperparameters='{"epochs": 5}'`
2. Increase rank: `--hyperparameters='{"rank": 64}'`
3. Filter for high-quality examples: `--datasetFilter='{"quality": {"$gte": 0.9}}'`
4. Use larger base model
5. Increase dataset size

---

## API Reference

### Commands.execute Types

```typescript
// Start training
const result = await Commands.execute<TrainingStartResult>('training/start', {
  personaId: UUID,
  provider: 'peft' | 'mlx' | 'openai',
  datasetDbHandle: string,
  datasetFilter?: Record<string, unknown>,
  hyperparameters?: Partial<TrainingHyperparameters>
});

// Check status
const status = await Commands.execute<TrainingStatusResult>('training/status', {
  sessionId: UUID,
  includeMetrics?: boolean
});

// Deploy adapter
const deployment = await Commands.execute<AdapterDeployResult>('adapter/deploy', {
  sessionId: UUID,
  adapterName?: string,
  deployToGenome?: boolean,
  layerWeight?: number
});
```

### DataDaemon with Handles

```typescript
// Open prepared database
const dbResult = await Commands.execute('data/open', {
  adapter: 'sqlite',
  config: {
    path: '/datasets/prepared/continuum-git.sqlite',
    mode: 'readonly'
  }
});

// Query examples
const examples = await Commands.execute('data/list', {
  dbHandle: dbResult.dbHandle,
  collection: 'training_examples',
  filter: { quality: { $gte: 0.8 }, used: false },
  limit: 100
});
```

---

## Performance Benchmarks

**Hardware**: M1 MacBook Air (8GB RAM)

| Model | Examples | Epochs | Rank | Time | Adapter Size |
|-------|----------|--------|------|------|--------------|
| TinyLlama-1.1B | 100 | 1 | 8 | 5 min | 8 MB |
| TinyLlama-1.1B | 100 | 3 | 32 | 15 min | 35 MB |
| TinyLlama-1.1B | 500 | 3 | 32 | 60 min | 35 MB |
| Phi-2 (2.7B) | 100 | 3 | 32 | 30 min | 50 MB |

**Scaling Estimates**:
- Time scales linearly with examples and epochs
- Adapter size depends on rank (not epochs/examples)
- Larger models scale linearly with parameter count

---

## Phase Implementation Status

- ✅ **Phase 1**: Core Training Infrastructure (Week 1)
- ✅ **Phase 2**: Status Monitoring & Metrics (Week 2)
- ✅ **Phase 3**: Adapter Deployment (Week 3)
- ✅ **Phase 4**: Continuous Learning (Week 4)
- 🔨 **Phase 5**: Multi-Provider Support (Week 5) - IN PROGRESS
- 📋 **Phase 6**: Production Hardening (Week 6+) - PLANNED

---

## Related Documentation

- [TRAINING-SYSTEM-ARCHITECTURE.md](./TRAINING-SYSTEM-ARCHITECTURE.md) - Complete architecture
- [UNIVERSAL-LORA-FINE-TUNING.md](../system/user/server/modules/UNIVERSAL-LORA-FINE-TUNING.md) - Provider research
- [FINE-TUNING-PROVIDER-RESEARCH.md](../system/user/server/modules/FINE-TUNING-PROVIDER-RESEARCH.md) - API comparison
- [MULTI-DATABASE-HANDLES.md](./MULTI-DATABASE-HANDLES.md) - Multi-database system
- [PERSONA-CONVERGENCE-ROADMAP.md](../system/user/server/modules/PERSONA-CONVERGENCE-ROADMAP.md) - Continuous learning

---

**Last Updated**: November 7, 2025
**Version**: 1.0
