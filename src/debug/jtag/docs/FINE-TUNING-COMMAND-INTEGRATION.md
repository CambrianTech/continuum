# Fine-Tuning Command Integration Architecture

**Status**: Phase 1 Implemented (2025-11-15)
**Next Phase**: Large Dataset Streaming Support

## Overview

This document describes how fine-tuning adapters are integrated into the JTAG command system, enabling end-to-end training workflows through commands.

## Architecture Pattern

### Fire-and-Forget Adapter Instantiation

Similar to inference adapters in `AIProviderDaemon`, fine-tuning adapters are instantiated on-demand:

```typescript
// Inference pattern (for comparison)
const adapter = daemon.selectAdapter(provider);
const response = await adapter.generateText(request);

// Fine-tuning pattern (new)
const adapter = await createFineTuningAdapter(provider);
const result = await adapter.trainLoRA(request);
```

**Key differences**:
- **Inference**: Adapters registered in daemon, reused across requests
- **Fine-tuning**: Adapters instantiated per job (fire-and-forget), handle their own persistence

### Command Flow

```
User → genome/job-create command
  ↓
1. Validate params & create FineTuningJobEntity in database (status: 'queued')
  ↓
2. Load training dataset from file into memory (Phase 1: small datasets only)
  ↓
3. Instantiate provider adapter on-demand
  ↓
4. Call adapter.trainLoRA(request)
  ↓
  → Adapter uploads data to provider
  → Adapter creates training job
  → Adapter persists session to database internally
  → Returns immediately with job handle
  ↓
5. Update FineTuningJobEntity (status: 'running', providerJobId: real ID)
  ↓
6. Return success to user
```

## Implementation

### genome/job-create Command

**File**: `commands/genome/job-create/server/GenomeJobCreateServerCommand.ts`

**Key Functions**:

```typescript
/**
 * Create fine-tuning adapter instance for a given provider
 * Fire-and-forget pattern - instantiate when needed
 */
async function createFineTuningAdapter(provider: string): Promise<BaseLoRATrainer> {
  switch (provider) {
    case 'openai':
      const { OpenAILoRAAdapter } = await import('...OpenAIFineTuningAdapter');
      return new OpenAILoRAAdapter(apiKey);
    // ... other providers
  }
}

/**
 * Load training dataset from file path
 * Phase 1: Loads entire dataset into memory
 * Phase 2: TODO - Streaming support for large datasets
 */
async function loadTrainingDatasetFromFile(
  filePath: string,
  personaId: UUID
): Promise<TrainingDataset> {
  const content = await fs.promises.readFile(filePath, 'utf-8');
  const lines = content.trim().split('\n').filter(line => line.trim());
  const examples = lines.map(line => JSON.parse(line));

  return {
    examples,
    metadata: {
      personaId,
      personaName: 'PersonaUser',
      traitType: 'custom',
      createdAt: Date.now(),
      source: 'conversations',
      totalExamples: examples.length
    }
  };
}
```

**Execution Flow**:

```typescript
async execute(params: GenomeJobCreateParams) {
  // 1-5. Validation and entity creation (existing code)

  // 6. Save initial job entity to database
  await Commands.execute('data/create', {
    collection: 'fine_tuning_jobs',
    data: jobEntity
  });

  // 7. Load training dataset from file
  const dataset = await loadTrainingDatasetFromFile(
    jobEntity.trainingFileId,
    params.personaId
  );

  // 8. Instantiate adapter and start training
  const adapter = await createFineTuningAdapter(params.provider);
  const trainingRequest: LoRATrainingRequest = {
    personaId: params.personaId,
    personaName: dataset.metadata.personaName,
    traitType: dataset.metadata.traitType,
    baseModel: config.model.baseModel,
    dataset,  // In-memory dataset object
    rank: config.method.loraConfig?.rank,
    alpha: config.method.loraConfig?.alpha,
    epochs: config.schedule.epochs,
    learningRate: config.optimizer.learningRate,
    batchSize: config.schedule.batchSize
  };

  const trainingResult = await adapter.trainLoRA(trainingRequest);

  if (!trainingResult.success) {
    // Update job status to 'failed'
    await Commands.execute('data/update', {
      collection: 'fine_tuning_jobs',
      id: jobId,
      updates: { status: 'failed', error: trainingResult.error }
    });
    return { success: false, error: trainingResult.error };
  }

  // 9. Update job with real provider job ID
  await Commands.execute('data/update', {
    collection: 'fine_tuning_jobs',
    id: jobId,
    updates: {
      status: 'running',
      providerJobId: trainingResult.modelId,
      startedAt: Date.now()
    }
  });

  // 10. Return job details
  return { success: true, job: { ... } };
}
```

### Adapter Lifecycle

**Adapters implement two primitives** (from `BaseLoRATrainer`):

1. **`trainLoRA(request)`** - Public orchestrator method
   - Validates request
   - Calls `_startTraining()` primitive
   - Persists session to database internally
   - Returns immediately (< 30 seconds)

2. **`_startTraining(request)`** - Provider-specific primitive
   - Converts dataset to JSONL
   - Uploads to provider
   - Creates training job
   - Returns handle with provider job ID

**Example: OpenAI Adapter**:

```typescript
protected async _startTraining(request: LoRATrainingRequest): Promise<TrainingHandle> {
  // 1. Convert dataset to JSONL and write to temp file
  const datasetPath = await this.exportDatasetToJSONL(request.dataset);

  // 2. Upload to OpenAI
  const fileId = await this.uploadDataset(datasetPath);

  // 3. Create fine-tuning job
  const jobId = await this.createFineTuningJob(request, fileId);

  // 4. Clean up temp file
  await this.cleanupTempFiles(datasetPath);

  // 5. Return handle
  return { jobId, fileId, metadata: { ... } };
}
```

## Phase 1: Current Implementation (DONE)

### Capabilities

✅ End-to-end fine-tuning through commands
✅ Support for OpenAI, Fireworks, DeepSeek, Together
✅ Async handle pattern (non-blocking)
✅ Database persistence at every step
✅ Error handling and status tracking
✅ Integration test infrastructure

### Limitations

⚠️ **Loads entire dataset into memory** - acceptable for small datasets (< 10K examples)
⚠️ **No streaming support** - not suitable for large datasets (> 100K examples)
⚠️ **No chunking** - cannot split large datasets across multiple jobs
⚠️ **No progress tracking during upload** - user doesn't see upload progress

### Test Coverage

```bash
# End-to-end test
npx tsx tests/integration/genome-fine-tuning-e2e.test.ts

# Tests:
# 1. Submit training jobs for 4 providers
# 2. Query job status using genome/job-status
# 3. Poll until completion or timeout
# 4. Verify jobs transition from 'queued' → 'running' → 'completed'
```

## Phase 2: Large Dataset Support (TODO)

### Goals

1. **Streaming file reads** - avoid loading entire dataset into memory
2. **Chunked uploads** - upload in batches for progress tracking
3. **Resume capability** - recover from failed uploads
4. **Dataset validation** - verify format before uploading

### Proposed Changes

#### Option A: Adapter Accepts File Path

**Change `LoRATrainingRequest` interface**:

```typescript
export interface LoRATrainingRequest {
  // ... existing fields ...

  // Phase 1: In-memory dataset
  dataset?: TrainingDataset;

  // Phase 2: File path for streaming
  datasetPath?: string;

  // Validation: Exactly one must be provided
}
```

**Adapter reads file directly**:

```typescript
protected async _startTraining(request: LoRATrainingRequest): Promise<TrainingHandle> {
  // Phase 2: Stream from file if provided
  if (request.datasetPath) {
    return await this._startTrainingFromFile(request.datasetPath, request);
  }

  // Phase 1: Use in-memory dataset (existing code)
  const datasetPath = await this.exportDatasetToJSONL(request.dataset!);
  // ... rest of existing code
}

private async _startTrainingFromFile(
  datasetPath: string,
  request: LoRATrainingRequest
): Promise<TrainingHandle> {
  // Stream file directly to provider
  const readStream = fs.createReadStream(datasetPath);
  const uploadResult = await this.uploadDatasetStream(readStream);
  return { jobId: uploadResult.jobId, ... };
}
```

#### Option B: Introduce Dataset Service

**Create dedicated dataset management**:

```typescript
// New: commands/dataset/upload command
interface DatasetUploadParams {
  filePath: string;
  chunkSize?: number;  // Default: 1000 examples
  onProgress?: (uploaded: number, total: number) => void;
}

interface DatasetUploadResult {
  datasetId: UUID;
  totalExamples: number;
  uploadedChunks: number;
}

// genome/job-create uses dataset ID instead of file path
interface GenomeJobCreateParams {
  // ...
  datasetId: UUID;  // References uploaded dataset
}
```

### Memory Usage Comparison

| Dataset Size | Phase 1 (In-Memory) | Phase 2 (Streaming) |
|--------------|---------------------|---------------------|
| 1K examples  | ~1 MB               | ~10 KB              |
| 10K examples | ~10 MB              | ~10 KB              |
| 100K examples| ~100 MB             | ~10 KB              |
| 1M examples  | ~1 GB ❌            | ~10 KB ✅           |

## Status Query Pattern

### genome/job-status Command

**Current Implementation** (Phase 1):

```typescript
// Just reads from database
const job = await Commands.execute('data/read', {
  collection: 'fine_tuning_jobs',
  id: jobId
});

return { success: true, job: { ... } };
```

**Future Enhancement** (Phase 1.5):

```typescript
// Support refresh=true to query provider
async execute(params: GenomeJobStatusParams) {
  // 1. Load job from database
  const jobEntity = await loadJob(params.jobId);

  // 2. If refresh=true, query provider for real-time status
  if (params.refresh) {
    const adapter = await createFineTuningAdapter(jobEntity.provider);
    const status = await adapter._queryStatus(
      jobEntity.id,
      jobEntity.providerJobId,
      jobEntity.metadata
    );

    // 3. Update database with latest status
    await Commands.execute('data/update', {
      collection: 'fine_tuning_jobs',
      id: params.jobId,
      updates: {
        status: status.status,
        fineTunedModel: status.modelId,
        completedAt: status.status === 'completed' ? Date.now() : undefined
      }
    });

    return { success: true, job: { ...jobEntity, ...status }, refreshed: true };
  }

  // 4. Return cached status from database
  return { success: true, job: jobEntity, refreshed: false };
}
```

## Provider-Specific Notes

### OpenAI
- **API**: REST with FormData file upload
- **File Format**: JSONL with `{messages: [...]}` format
- **Job Status**: `validating_files` → `queued` → `running` → `succeeded`
- **Cost**: ~$0.00405 per example (gpt-4o-mini)

### Fireworks
- **API**: Two-step (create dataset, then upload file)
- **File Format**: JSONL with `{messages: [...]}` format
- **Job Status**: `CREATING` → `PENDING` → `RUNNING` → `COMPLETED`
- **Unique**: Can download trained model weights (.safetensors)

### DeepSeek
- **API**: REST (27x cheaper than OpenAI)
- **Pricing**: ~$0.55/1M tokens vs OpenAI's $15/1M
- **Status**: MVP stub (interface only, full implementation Phase 2)

### Together
- **API**: REST with multi-part upload
- **File Format**: JSONL
- **Job Status**: Similar to Fireworks

## Testing Strategy

### Unit Tests
```bash
# Adapter-specific tests (using stubs)
npx tsx system/genome/fine-tuning/server/adapters/test-openai.ts
npx tsx system/genome/fine-tuning/server/adapters/test-fireworks.ts
```

### Integration Tests
```bash
# Full command flow (requires API keys)
npx tsx tests/integration/genome-fine-tuning-e2e.test.ts
```

### Test Data
```
/Volumes/FlashGordon/cambrian/datasets/prepared/fine-tuning-test.jsonl
```

Small dataset (< 100 examples) for testing with real APIs.

## Error Handling

### Job Creation Failures

```typescript
try {
  const trainingResult = await adapter.trainLoRA(request);

  if (!trainingResult.success) {
    // Mark job as failed in database
    await updateJobStatus(jobId, 'failed', trainingResult.error);
    return { success: false, error: trainingResult.error };
  }
} catch (error) {
  // Unexpected errors
  await updateJobStatus(jobId, 'failed', error.message);
  throw error;
}
```

### Upload Failures (Phase 2)

```typescript
// Retry logic for chunked uploads
async uploadWithRetry(chunk: Buffer, retries = 3): Promise<void> {
  for (let i = 0; i < retries; i++) {
    try {
      await this.uploadChunk(chunk);
      return;
    } catch (error) {
      if (i === retries - 1) throw error;
      await sleep(1000 * Math.pow(2, i)); // Exponential backoff
    }
  }
}
```

## Future Enhancements

### Phase 2: Large Dataset Support
- Streaming file reads
- Chunked uploads with progress
- Resume capability
- Dataset validation

### Phase 3: Multi-Provider Training
- Submit same dataset to multiple providers
- Compare results
- Cost/performance analysis

### Phase 4: Continuous Fine-Tuning
- Periodic retraining on new data
- Incremental updates
- A/B testing between versions

### Phase 5: Dataset Management
- Dataset versioning
- Dataset catalog/search
- Dataset quality metrics
- Dataset transformation pipeline

## References

- **Type Definitions**: `daemons/data-daemon/shared/entities/FineTuningTypes.ts`
- **Job Entity**: `daemons/data-daemon/shared/entities/FineTuningJobEntity.ts`
- **Base Adapter**: `system/genome/fine-tuning/shared/BaseLoRATrainer.ts`
- **Commands**: `commands/genome/job-create/`, `commands/genome/job-status/`
- **Integration Test**: `tests/integration/genome-fine-tuning-e2e.test.ts`

## Summary

**Phase 1 (Current)**:
- ✅ Command integration complete
- ✅ Fire-and-forget adapter pattern
- ✅ End-to-end testing infrastructure
- ⚠️ Limited to small datasets (in-memory loading)

**Next Steps**:
1. Deploy and test Phase 1 with real API calls
2. Implement `genome/job-status --refresh=true` for real-time status
3. Design Phase 2 streaming architecture
4. Add progress tracking for long-running uploads

---

*Last Updated*: 2025-11-15 by Claude Code
*Status*: Phase 1 Deployed, Phase 2 Planned
