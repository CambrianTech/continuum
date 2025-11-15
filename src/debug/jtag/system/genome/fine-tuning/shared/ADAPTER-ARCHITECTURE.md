# LoRA Adapter Architecture

**Location**: Next to `BaseLoRATrainer.ts`

---

## Core Principle: Universal Handle Pattern

Fine-tuning is async (minutes to days). The base class provides a universal pattern that works for **all** providers:

- **Remote APIs** (OpenAI, Together, Fireworks)
- **Local training** (Ollama, MLX, PEFT)
- **Weird APIs** (Fireworks dataset names, custom endpoints)

---

## Base Class Responsibilities

`BaseLoRATrainer` orchestrates the universal flow:

1. **Start training** → Get handle → Persist to database → Return immediately
2. **Check status** → Load session → Query provider → Update database → Return status

**Subclasses just implement two primitives** - base handles everything else.

---

## The Two Primitives

Every adapter implements these:

```typescript
abstract class BaseLoRATrainer {
  /**
   * Start training, return handle immediately
   *
   * Remote APIs: Upload data, create job, return jobId
   * Local training: Spawn process, return processId
   * Weird APIs: Handle their quirks, return whatever identifier they give
   */
  protected abstract _startTraining(
    request: LoRATrainingRequest
  ): Promise<TrainingHandle>;

  /**
   * Query current status from provider
   *
   * Remote APIs: HTTP request to check job status
   * Local training: Check process status, read progress file
   * Weird APIs: Whatever they need to check status
   */
  protected abstract _queryStatus(
    session: TrainingSessionEntity
  ): Promise<TrainingStatus>;
}
```

---

## Universal Public API

Base class provides these to callers (genome/train command):

```typescript
abstract class BaseLoRATrainer {
  /**
   * Start training - returns immediately with handle
   */
  async trainLoRA(request: LoRATrainingRequest): Promise<LoRATrainingResult> {
    this.validateRequest(request);

    // 1. Start training (subclass primitive)
    const handle = await this._startTraining(request);

    // 2. Persist session with handle
    const session = await this._persistSession(request, handle);

    // 3. Return immediately
    return {
      success: true,
      sessionId: session.id,
      providerJobId: handle.jobId,
      status: 'running'
    };
  }

  /**
   * Check status - fast query
   */
  async checkStatus(sessionId: UUID): Promise<TrainingStatus> {
    // 1. Load session from database
    const session = await this._loadSession(sessionId);

    // 2. Query provider (subclass primitive)
    const status = await this._queryStatus(session);

    // 3. Update database if changed
    if (status.status !== session.status) {
      await this._updateSession(session.id, status);
    }

    // 4. Return current status
    return status;
  }
}
```

---

## Type Definitions

```typescript
/**
 * Handle returned by _startTraining()
 * Contains whatever identifier(s) needed to track this training job
 */
interface TrainingHandle {
  /** Primary identifier (jobId, processId, etc.) */
  jobId: string;

  /** Optional secondary identifiers */
  fileId?: string;        // For cleanup
  datasetName?: string;   // Fireworks-style
  processId?: number;     // Local training

  /** Provider-specific metadata */
  metadata?: Record<string, unknown>;
}

/**
 * Status returned by _queryStatus()
 */
interface TrainingStatus {
  status: 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';
  progress?: number;      // 0-1 if available
  modelId?: string;       // When completed
  error?: string;         // If failed

  /** Provider-specific data */
  metadata?: Record<string, unknown>;
}
```

---

## Example Implementations

### Remote API (OpenAI)

```typescript
class OpenAILoRAAdapter extends BaseLoRATrainer {
  protected async _startTraining(request): Promise<TrainingHandle> {
    // 1. Upload training data
    const fileId = await this.uploadFile(request.dataset);

    // 2. Create fine-tuning job
    const response = await fetch('https://api.openai.com/v1/fine_tuning/jobs', {
      method: 'POST',
      body: JSON.stringify({
        training_file: fileId,
        model: request.baseModel,
        hyperparameters: { n_epochs: request.epochs }
      })
    });

    const { id: jobId } = await response.json();

    // 3. Return handle immediately
    return { jobId, fileId };
  }

  protected async _queryStatus(session): Promise<TrainingStatus> {
    const response = await fetch(
      `https://api.openai.com/v1/fine_tuning/jobs/${session.providerJobId}`
    );

    const job = await response.json();

    return {
      status: this.mapStatus(job.status),
      modelId: job.fine_tuned_model,
      error: job.error?.message
    };
  }
}
```

### Local Training (Ollama)

```typescript
class OllamaLoRAAdapter extends BaseLoRATrainer {
  protected async _startTraining(request): Promise<TrainingHandle> {
    // 1. Export training data to disk
    const dataPath = await this.exportDataset(request.dataset);

    // 2. Spawn llama.cpp process
    const process = spawn('llama-finetune', [
      '--model', request.baseModel,
      '--train-data', dataPath,
      '--rank', String(request.rank),
      '--epochs', String(request.epochs)
    ]);

    // 3. Return handle immediately
    return {
      jobId: process.pid.toString(),
      processId: process.pid,
      metadata: { dataPath }
    };
  }

  protected async _queryStatus(session): Promise<TrainingStatus> {
    // Check if process is still running
    const isRunning = await this.isProcessRunning(session.metadata.processId);

    if (!isRunning) {
      // Check exit status / output file for completion
      const result = await this.checkTrainingOutput(session.metadata.dataPath);
      return result;
    }

    // Read progress from log file
    const progress = await this.readProgressFile(session.metadata.dataPath);

    return {
      status: 'running',
      progress: progress.epochsComplete / progress.totalEpochs
    };
  }
}
```

### Weird API (Fireworks with dataset names)

```typescript
class FireworksLoRAAdapter extends BaseLoRATrainer {
  protected async _startTraining(request): Promise<TrainingHandle> {
    // 1. Upload dataset with unique name
    const datasetName = `dataset-${Date.now()}`;
    await this.uploadDataset(datasetName, request.dataset);

    // 2. Create job with dataset NAME (not file ID)
    const response = await fetch(
      `https://api.fireworks.ai/v1/accounts/${this.accountId}/jobs`,
      {
        method: 'POST',
        body: JSON.stringify({
          dataset: datasetName,  // ← Different from OpenAI!
          baseModel: request.baseModel
        })
      }
    );

    const { id: jobId } = await response.json();

    // 3. Return handle
    return {
      jobId,
      datasetName,  // Need this for their API
      metadata: { accountId: this.accountId }
    };
  }

  protected async _queryStatus(session): Promise<TrainingStatus> {
    // Their API requires accountId in URL
    const response = await fetch(
      `https://api.fireworks.ai/v1/accounts/${session.metadata.accountId}/jobs/${session.providerJobId}`
    );

    const job = await response.json();

    return {
      status: this.mapStatus(job.state),
      modelId: job.output_model
    };
  }
}
```

---

## Database Persistence

Base class uses `TrainingSessionEntity` (already exists):

```typescript
// When starting training
await Commands.execute('data/create', {
  collection: 'training_sessions',
  data: {
    providerJobId: handle.jobId,
    provider: this.providerId,
    status: 'running',
    personaId: request.personaId,
    metadata: handle.metadata,
    startedAt: Date.now()
  }
});

// When checking status
const session = await Commands.execute('data/read', {
  collection: 'training_sessions',
  id: sessionId
});

// When updating status
await Commands.execute('data/update', {
  collection: 'training_sessions',
  id: sessionId,
  data: {
    status: status.status,
    modelId: status.modelId,
    updatedAt: Date.now()
  }
});
```

---

## Benefits

**Universal**: Works for remote APIs, local training, and weird APIs

**Simple**: Subclasses just implement 2 methods

**Non-blocking**: Everything returns immediately with handles

**Crash-proof**: Handles persisted in database, survives restarts

**Testable**: Each primitive is independently testable

**Extensible**: New providers just implement the 2 primitives

---

## Command Integration

```bash
# Start training - returns immediately
./jtag genome/train \
  --personaId=helper-ai \
  --provider=openai \
  --baseModel=gpt-4o-mini-2024-07-18 \
  --epochs=1

# Returns: { sessionId: "abc-123", providerJobId: "ftjob-xyz" }

# Check status anytime (even days later)
./jtag genome/training-status --sessionId=abc-123

# Returns: { status: "running", progress: 0.7 }
```

---

## Optional: Background Watcher

Separate daemon (not in adapters) can poll active sessions:

```typescript
// Runs independently, emits events
setInterval(async () => {
  const sessions = await Commands.execute('data/list', {
    collection: 'training_sessions',
    filter: { status: 'running' }
  });

  for (const session of sessions) {
    const adapter = getAdapter(session.provider);
    const status = await adapter.checkStatus(session.id);

    if (status.status !== session.status) {
      Events.emit('training:status-changed', {
        sessionId: session.id,
        newStatus: status.status
      });
    }
  }
}, 30000);  // Poll every 30 seconds
```

---

## Summary

**Base class** = Universal orchestration (start → persist → return, load → query → update)

**Subclasses** = Two primitives (`_startTraining`, `_queryStatus`)

**Result** = Clean, elegant, works for everything
