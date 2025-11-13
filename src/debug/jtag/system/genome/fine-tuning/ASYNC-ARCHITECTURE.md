# Fine-Tuning Async Architecture

**Core Principle**: Fine-tuning takes minutes to days. Never block. Return handles immediately.

---

## The Problem

Fine-tuning is slow:
- **OpenAI**: 5-15 minutes (proven: job succeeded after test timeout)
- **Local training**: Hours to days
- **Can't block**: No promise should wait this long

---

## The Solution: Handle Pattern

Just like DataDaemon's `dbHandle` pattern:

1. **Start operation** → Return handle immediately
2. **Store handle** in database (survives restarts)
3. **Check status** anytime with handle
4. **Optional polling** in background daemon

---

## Architecture

### Adapter Interface

```typescript
interface LoRAAdapter {
  // Fast: Start training, return handle
  startTraining(request: LoRATrainingRequest): Promise<{
    providerJobId: string;  // The handle
    fileId?: string;        // For cleanup
  }>;

  // Fast: Query status from provider
  checkStatus(providerJobId: string): Promise<{
    status: 'pending' | 'running' | 'completed' | 'failed';
    progress?: number;      // 0-1 if available
    modelId?: string;       // When completed
    error?: string;         // If failed
  }>;
}
```

### Command Flow

**`./jtag genome/train`**:
```typescript
// 1. Start training (fast)
const { providerJobId, fileId } = await adapter.startTraining(request);

// 2. Create persisted session entity
const session = await Commands.execute('data/create', {
  collection: 'training_sessions',
  data: {
    providerJobId,        // The handle!
    provider: 'openai',
    status: 'running',
    fileId,
    personaId: request.personaId,
    startedAt: Date.now()
  }
});

// 3. Return immediately
return { sessionId: session.id, providerJobId };
```

**`./jtag genome/training-status --sessionId=xyz`**:
```typescript
// 1. Load session from database
const session = await Commands.execute('data/read', {
  collection: 'training_sessions',
  id: sessionId
});

// 2. Check current status (fast API call)
const status = await adapter.checkStatus(session.providerJobId);

// 3. Update entity if changed
if (status.status !== session.status) {
  await Commands.execute('data/update', {
    collection: 'training_sessions',
    id: sessionId,
    data: {
      status: status.status,
      modelId: status.modelId,
      updatedAt: Date.now()
    }
  });
}

// 4. Return current status
return status;
```

---

## Database Persistence

**Why**: Handles must survive server restarts.

**Entity**: `TrainingSessionEntity` (already exists!)
```typescript
{
  id: UUID,
  providerJobId: string,      // The handle
  provider: 'openai' | 'deepseek' | ...,
  status: 'pending' | 'running' | 'completed' | 'failed',
  fileId?: string,
  modelId?: string,           // Set when completed
  personaId: UUID,
  startedAt: number,
  completedAt?: number
}
```

**Benefits**:
- Server restarts? Sessions still there
- Need status days later? Just query
- Training continues on provider's servers regardless

---

## Optional: Background Polling Daemon

**Not required for MVP**, but nice to have:

```typescript
class TrainingWatcherDaemon {
  async pollActiveSessions() {
    // 1. Query running sessions
    const sessions = await Commands.execute('data/list', {
      collection: 'training_sessions',
      filter: { status: 'running' }
    });

    // 2. Check each (batch, not one-by-one)
    for (const session of sessions) {
      const status = await adapter.checkStatus(session.providerJobId);

      // 3. Update if changed
      if (status.status !== session.status) {
        await Commands.execute('data/update', { ... });

        // 4. Emit event
        Events.emit('training:status-changed', {
          sessionId: session.id,
          status: status.status
        });
      }
    }

    // 5. Wait before next poll (e.g., 30 seconds)
    await new Promise(resolve => setTimeout(resolve, 30000));
  }
}
```

**Characteristics**:
- Runs in background (doesn't block anything)
- Batch polling (efficient)
- Delayed events (30s lag is fine)
- Optional (users can manually check status instead)

---

## OpenAI Example (Proven Working)

**Test Results** (2025-11-13):
- Job ID: `ftjob-W0031UXLmy7Ayt5DpyWach3T`
- Status: ✅ Succeeded
- Model: `ft:gpt-4o-mini-2024-07-18:personal::CbUFSyrR`
- Duration: ~10 minutes

**Implementation**:
```typescript
// Start training (fast)
async startTraining(request) {
  // 1. Upload file
  const fileId = await this.uploadFile(jsonlPath);

  // 2. Create job
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
  return { providerJobId: jobId, fileId };
}

// Check status (fast)
async checkStatus(jobId) {
  const response = await fetch(
    `https://api.openai.com/v1/fine_tuning/jobs/${jobId}`
  );

  const job = await response.json();

  return {
    status: job.status,  // 'validating_files' | 'queued' | 'running' | 'succeeded' | 'failed'
    modelId: job.fine_tuned_model,
    error: job.error?.message
  };
}
```

---

## Comparison: Bad vs Good

### ❌ BAD (Blocking)
```typescript
async trainLoRA(request) {
  const jobId = await startJob();

  // Block for 10 minutes!!!
  while (await checkStatus(jobId) !== 'completed') {
    await sleep(5000);
  }

  return result;  // Finally returns after 10 minutes
}
```

**Problems**:
- Blocks the thread
- Can't check status independently
- Loses job if server restarts
- Arbitrary timeout (what if it takes 20 minutes?)

### ✅ GOOD (Async with Handle)
```typescript
async startTraining(request) {
  const jobId = await startJob();
  return { providerJobId: jobId };  // Returns immediately
}

async checkStatus(providerJobId) {
  return await queryAPI(providerJobId);  // Fast, anytime
}
```

**Benefits**:
- Returns immediately
- Can check status whenever needed
- Survives restarts (handle in database)
- No arbitrary timeouts

---

## Summary

**Adapters**:
- `startTraining()` - Fast, returns handle
- `checkStatus(handle)` - Fast, queries API

**Commands**:
- `genome/train` - Calls `startTraining()`, stores handle, returns immediately
- `genome/training-status` - Loads handle, calls `checkStatus()`, updates database

**Database**:
- `TrainingSessionEntity` - Persists handle + status
- Survives restarts, no data loss

**Optional**:
- Background daemon polls active sessions
- Emits events on status changes
- Delayed/batched is fine

**No blocking. No threads in adapters. Just handles and fast API calls.**
