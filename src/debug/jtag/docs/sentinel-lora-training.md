# Sentinel-Driven LoRA Training Pipelines

Orchestrates the full LoRA fine-tuning workflow through Sentinel pipelines: prepare data, train adapter, register, activate.

## Architecture

```
genome/training-pipeline (convenience entry point)
  -> builds Pipeline JSON via buildLoRATrainingPipeline()
  -> forwards to sentinel/run --type=pipeline
    -> Step 0: genome/dataset-prepare  (Command step)
    -> Step 1: condition on step 0 success
      -> Step 1.0: genome/train         (Command step - wraps PEFTLoRAAdapter)
      -> Step 1.1: genome/paging-adapter-register (existing Command step)
      -> Step 1.2: genome/paging-activate (existing Command step)
```

Step-to-step data flows via Rust interpolation: `{{steps.0.data.datasetPath}}` passes the JSONL path from dataset-prepare into train.

## Commands

### `genome/dataset-prepare`

Collects training data from a persona's chat conversations and exports as JSONL.

```bash
./jtag genome/dataset-prepare \
  --personaId="<uuid>" \
  --personaName="Helper AI" \
  --roomId="<room-uuid>" \
  --traitType="conversational" \
  --minMessages=10 \
  --maxMessages=500
```

**Returns**: `{ success, datasetPath, exampleCount, personaId, traitType }`

Dataset saved to `.continuum/genome/datasets/{name}-{trait}-{timestamp}.jsonl`

### `genome/train`

Executes LoRA fine-tuning on a JSONL dataset via PEFTLoRAAdapter.

```bash
./jtag genome/train \
  --personaId="<uuid>" \
  --personaName="Helper AI" \
  --traitType="conversational" \
  --datasetPath=".continuum/genome/datasets/helper-ai-conversational-1234.jsonl" \
  --baseModel="smollm2:135m" \
  --rank=32 \
  --epochs=3 \
  --learningRate=0.0001 \
  --batchSize=4
```

**Returns**: `{ success, adapterPath, metrics: { finalLoss, trainingTime, examplesProcessed, epochs } }`

Requires Python PEFT environment. Check with `PEFTLoRAAdapter.supportsFineTuning()`.

### `genome/training-pipeline`

One-command entry point. Builds and submits the full pipeline to Sentinel.

```bash
./jtag genome/training-pipeline \
  --personaId="<uuid>" \
  --personaName="Helper AI" \
  --roomId="<general-room-uuid>" \
  --baseModel="smollm2:135m"

# Track progress
./jtag sentinel/status --handle="<returned-handle>"
```

**Returns**: `{ success, handle, pipelineName }`

## Pipeline Template

`system/sentinel/pipelines/LoRATrainingPipeline.ts` exports `buildLoRATrainingPipeline(config)` which produces a `Pipeline` object matching the Rust sentinel schema.

```typescript
import { buildLoRATrainingPipeline } from '@system/sentinel/pipelines/LoRATrainingPipeline';

const pipeline = buildLoRATrainingPipeline({
  personaId: 'uuid',
  personaName: 'Helper AI',
  roomId: 'room-uuid',
  traitType: 'conversational',
  baseModel: 'smollm2:135m',
  rank: 32,
  epochs: 3,
});
```

## Shared Utilities

`TrainingDatasetBuilder.loadFromJSONL(filePath, metadata)` - Loads a JSONL training dataset back into a `TrainingDataset` object. Used by both `genome/train` and `genome/job-create`.

## Testing

```bash
# Unit tests (pipeline template validation)
npx vitest tests/unit/semantic-cognition.test.ts

# Integration tests (requires npm start)
npx vitest tests/integration/sentinel-lora-training.test.ts
```

## Academy Dojo — Dual-Sentinel Teacher/Student Architecture

The Academy extends the single-pipeline approach into a **self-sustaining learning system**. Two sentinels work together: a Teacher that synthesizes training data and examinations, and a Student that trains and proves mastery.

See [ACADEMY-DOJO-ARCHITECTURE.md](personas/ACADEMY-DOJO-ARCHITECTURE.md) for the full design document.

### Quick Start

```bash
./jtag genome/academy-session \
  --personaId="<uuid>" \
  --personaName="Helper AI" \
  --skill="typescript-generics" \
  --baseModel="smollm2:135m"

# Returns: { academySessionId, teacherHandle, studentHandle }

# Monitor progress
./jtag sentinel/status --handle="<teacherHandle>"
./jtag data/list --collection=academy_sessions
./jtag data/list --collection=academy_examinations
```

### Academy Commands

| Command | Purpose |
|---------|---------|
| `genome/dataset-synthesize` | LLM-generated training data for a topic |
| `genome/academy-session` | Spawn teacher + student sentinels for a skill |

### Academy Entities

| Entity | Collection | Purpose |
|--------|-----------|---------|
| `AcademySessionEntity` | `academy_sessions` | Session lifecycle tracking |
| `AcademyCurriculumEntity` | `academy_curricula` | Teacher-designed curriculum |
| `AcademyExaminationEntity` | `academy_examinations` | Exam questions + graded responses |

### Event Flow

All events scoped by session: `academy:{sessionId}:{action}`

```
Teacher                              Student
  │                                    │
  ├─ curriculum:ready ────────────────►│
  │                                    │
  ├─ dataset:ready ───────────────────►│
  │                                    ├─ training:started
  │◄──────────────── training:complete─┤
  │                                    │
  ├─ exam:ready ──────────────────────►│
  │◄──────────────── exam:responses ───┤
  │                                    │
  ├─ exam:graded ─────────────────────►│
  │   (topic:passed or topic:remediate)│
  │                                    │
  ├─ session:complete ────────────────►│
```

## Files

| File | Purpose |
|------|---------|
| `commands/genome/dataset-prepare/` | Collect chat data -> JSONL |
| `commands/genome/train/` | JSONL -> trained LoRA adapter |
| `commands/genome/training-pipeline/` | One-command full workflow |
| `commands/genome/dataset-synthesize/` | LLM-synthesized training data |
| `commands/genome/academy-session/` | Dual-sentinel session orchestration |
| `system/sentinel/pipelines/LoRATrainingPipeline.ts` | Single-pipeline template |
| `system/sentinel/pipelines/TeacherPipeline.ts` | Teacher sentinel pipeline template |
| `system/sentinel/pipelines/StudentPipeline.ts` | Student sentinel pipeline template |
| `system/genome/shared/AcademyTypes.ts` | Event taxonomy, config, shared types |
| `system/genome/entities/AcademySessionEntity.ts` | Session entity |
| `system/genome/entities/AcademyCurriculumEntity.ts` | Curriculum entity |
| `system/genome/entities/AcademyExaminationEntity.ts` | Examination entity |
| `system/genome/fine-tuning/server/TrainingDatasetBuilder.ts` | Dataset building + JSONL I/O |
| `tests/unit/semantic-cognition.test.ts` | Pipeline template + entity unit tests |
| `tests/integration/sentinel-lora-training.test.ts` | Command integration tests |
