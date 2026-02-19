# Training Event Architecture

## Problem

Current training flow has potential timeouts:
1. Dataset uploads can be slow for large files
2. Status polling can block if called synchronously
3. No clear separation between "start" and "monitor"

## Solution: Command + Handle Pattern

### Core Principle

```
Command returns handle → Reuse handle for subsequent calls → Events emit on change
```

### Commands

```bash
# Start training - returns sessionId handle immediately
./jtag genome/train --personaId=helper-ai --trait=tone_and_voice --dataset=path.jsonl
# Returns: { success: true, sessionId: "abc-123", status: "started" }

# Check status using handle
./jtag genome/train/status --sessionId=abc-123
# Returns: { status: "running", progress: 45, ... }

# List active training sessions
./jtag genome/train/list --personaId=helper-ai
# Returns: [{ sessionId, status, trait, startedAt }, ...]

# Cancel training
./jtag genome/train/cancel --sessionId=abc-123
```

### The Flow

```
genome/train              genome/train/status         Events
────────────              ───────────────────         ──────
    │                           │                        │
    ├─► _startTraining()        ├─► load session         │
    │   (upload, create)        │   from DB              │
    │                           │                        │
    ├─► persist to DB           ├─► _queryStatus()       │
    │                           │   (quick API call)     │
    │                           │                        │
    ├─► emit                    ├─► update DB if         ├─► training:started
    │   training:started        │   status changed       ├─► training:progress
    │                           │                        ├─► training:completed
    └─► return { sessionId }    ├─► emit if changed      ├─► training:failed
                                │                        │
                                └─► return status        └─► PersonaUser reacts
```

### Events

```typescript
// Training lifecycle events
Events.emit('training:started', {
  sessionId: UUID,
  personaId: UUID,
  provider: string,
  traitType: string
});

Events.emit('training:progress', {
  sessionId: UUID,
  progress: number,  // 0-100
  status: 'running' | 'validating' | 'queued'
});

Events.emit('training:completed', {
  sessionId: UUID,
  personaId: UUID,
  modelId: string,           // Provider's model ID
  ollamaModelName?: string,  // If registered with Ollama
  traitType: string,
  metrics: { finalLoss, epochs, examples }
});

Events.emit('training:failed', {
  sessionId: UUID,
  personaId: UUID,
  error: string
});
```

### TrainingMonitorDaemon

Background daemon that polls active training sessions:

```typescript
class TrainingMonitorDaemon {
  private pollInterval = 30000; // 30 seconds

  async start(): Promise<void> {
    setInterval(() => this.pollActiveSessions(), this.pollInterval);
  }

  private async pollActiveSessions(): Promise<void> {
    // 1. Query DB for sessions with status='running'
    const sessions = await DataDaemon.query<TrainingSessionEntity>({
      collection: 'training_sessions',
      filter: { status: 'running' }
    });

    // 2. Check each session (parallel, with timeout per request)
    await Promise.allSettled(
      sessions.data.map(session => this.checkSession(session))
    );
  }

  private async checkSession(session: TrainingSessionEntity): Promise<void> {
    const adapter = getFineTuningAdapter(session.provider);
    if (!adapter) return;

    // Quick status check (should be < 5 seconds)
    const status = await Promise.race([
      adapter.checkStatus(session.id),
      timeout(5000)  // Hard timeout
    ]);

    // Emit appropriate event
    if (status.status === 'completed') {
      Events.emit('training:completed', {
        sessionId: session.id,
        personaId: session.personaId,
        modelId: status.modelId,
        traitType: session.metadata?.traitType,
        metrics: status.metadata
      });
    } else if (status.status === 'failed') {
      Events.emit('training:failed', {
        sessionId: session.id,
        personaId: session.personaId,
        error: status.error
      });
    } else if (status.progress !== session.metadata?.lastProgress) {
      Events.emit('training:progress', {
        sessionId: session.id,
        progress: status.progress,
        status: status.status
      });
    }
  }
}
```

### PersonaUser Subscription

```typescript
// In PersonaUser constructor or init
Events.subscribe('training:completed', async (event) => {
  if (event.personaId !== this.id) return;

  // Register the trained adapter
  this.memory.genome.registerAdapter({
    name: event.traitType,
    domain: event.traitType,
    path: event.modelId,
    ollamaModelName: event.ollamaModelName
  });

  this.log.info(`🎓 Training complete for ${event.traitType}!`);
});
```

### Upload Handling

For large dataset uploads, use chunked streaming:

```typescript
async _startTraining(request: LoRATrainingRequest): Promise<TrainingHandle> {
  // 1. Export dataset locally (fast, no network)
  const datasetPath = await this.exportDatasetToJSONL(request.dataset, tmpPath);

  // 2. Upload with streaming (handles large files)
  const fileId = await this.streamUpload(datasetPath, {
    chunkSize: 1024 * 1024,  // 1MB chunks
    timeout: 30000,          // 30s per chunk
    retries: 3
  });

  // 3. Create job (fast API call)
  const jobId = await this.createJob(fileId, request);

  return { jobId, fileId };
}
```

### Benefits

1. **No blocking**: `trainLoRA()` returns in seconds
2. **Crash-proof**: Sessions persist in DB, monitor resumes on restart
3. **Timeout-safe**: Each poll has hard timeout, failures don't cascade
4. **Event-driven**: PersonaUsers react to events, no polling in user code
5. **Scalable**: One monitor handles all personas' training sessions

### Implementation Order

1. Add `training:*` events to Events.ts
2. Create TrainingMonitorDaemon (simple polling loop)
3. Update PersonaUser to subscribe to events
4. Update adapters to emit events on start
5. Test with Ollama (local, fast feedback)
6. Test with remote APIs (OpenAI, DeepSeek)

### File Structure

```
commands/genome/train/          # Start training, return handle
commands/genome/train/status/   # Check status using handle
commands/genome/train/list/     # List active sessions
commands/genome/train/cancel/   # Cancel training

daemons/training-monitor-daemon/  # Background poller (optional)
├── shared/
│   └── TrainingMonitorTypes.ts
├── server/
│   └── TrainingMonitorDaemonServer.ts
└── README.md
```

## Implementation Status

### Done
- [x] FineTuningAdapterFactory - returns correct adapter per provider
- [x] Async handle pattern in BaseLoRATrainerServer
- [x] DeepSeek adapter refactored to async pattern
- [x] PersonaTaskExecutor uses factory

### TODO
- [ ] Create genome/train command (use generator)
- [ ] Create genome/train/status command
- [ ] Create genome/train/list command
- [ ] Add training:* events to Events.ts
- [ ] PersonaUser subscribe to training:completed

### Generate Commands

```bash
# Create spec file
cat > /tmp/genome-train.spec.json << 'EOF'
{
  "name": "genome/train",
  "description": "Start LoRA training for a persona",
  "params": [
    { "name": "personaId", "type": "UUID", "description": "Persona to train" },
    { "name": "trait", "type": "TraitType", "description": "Trait adapter to train" },
    { "name": "dataset", "type": "string", "optional": true, "description": "Path to JSONL dataset" },
    { "name": "examples", "type": "TrainingExample[]", "optional": true, "description": "Inline examples" }
  ],
  "results": [
    { "name": "sessionId", "type": "UUID", "description": "Handle for status/cancel" },
    { "name": "status", "type": "string", "description": "started | queued | failed" }
  ],
  "accessLevel": "internal",
  "environment": "server"
}
EOF

npx tsx generator/CommandGenerator.ts /tmp/genome-train.spec.json commands
```
