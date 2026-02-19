# Training System Implementation Checklist

**Use this checklist to implement the training system phase by phase**

---

## Phase 1: Core Training Infrastructure ⏱️ Week 1

### Entity Definitions

- [ ] Create `system/data/entities/TrainingSessionEntity.ts`
  - [ ] All fields defined per spec
  - [ ] Validation logic implemented
  - [ ] Index decorators applied
  - [ ] Export from `system/data/entities/index.ts`

- [ ] Create `system/data/entities/TrainingCheckpointEntity.ts`
  - [ ] All fields defined per spec
  - [ ] Validation logic implemented
  - [ ] Export from index

- [ ] Create `system/data/entities/TrainingMetricsEntity.ts`
  - [ ] All fields defined per spec
  - [ ] Time-series optimized indexes
  - [ ] Export from index

- [ ] Create `system/data/entities/TrainingLogEntity.ts`
  - [ ] All fields defined per spec
  - [ ] Log levels enum
  - [ ] Export from index

- [ ] Register entities in `EntityRegistry`
  - [ ] Add to `EntityRegistry.ts`
  - [ ] Add to schema migrations

### Command: training/prepare

- [ ] Create `commands/training/prepare/shared/TrainingPrepareTypes.ts`
  - [ ] `TrainingPrepareParams` interface
  - [ ] `TrainingPrepareResult` interface
  - [ ] Factory functions

- [ ] Create `commands/training/prepare/server/TrainingPrepareServerCommand.ts`
  - [ ] Read JSONL line-by-line
  - [ ] Compute quality scores
  - [ ] Insert into SQLite
  - [ ] Return dbHandle
  - [ ] Error handling

- [ ] Create `commands/training/prepare/browser/TrainingPrepareBrowserCommand.ts`
  - [ ] Browser-side stub (delegates to server)

- [ ] Register command in `CommandRegistry`

- [ ] Unit tests: `tests/unit/training-prepare.test.ts`
  - [ ] Test JSONL parsing
  - [ ] Test quality scoring
  - [ ] Test SQLite insertion
  - [ ] Test error cases

### Command: training/start

- [ ] Create `commands/training/start/shared/TrainingStartTypes.ts`
  - [ ] `TrainingStartParams` interface
  - [ ] `TrainingStartResult` interface
  - [ ] Factory functions

- [ ] Create `commands/training/start/server/TrainingStartServerCommand.ts`
  - [ ] Validate params
  - [ ] Create TrainingSessionEntity
  - [ ] Query examples from prepared DB
  - [ ] Estimate cost and duration
  - [ ] Queue training job
  - [ ] Return sessionId immediately

- [ ] Create browser command stub

- [ ] Register command in CommandRegistry

- [ ] Unit tests: `tests/unit/training-start.test.ts`
  - [ ] Test param validation
  - [ ] Test example querying
  - [ ] Test cost estimation
  - [ ] Test job queuing

### TrainingOrchestrator

- [ ] Create `system/genome/fine-tuning/server/TrainingOrchestrator.ts`
  - [ ] Job queue (in-memory for Phase 1)
  - [ ] Provider routing
  - [ ] Progress tracking
  - [ ] Error handling
  - [ ] Checkpoint saving
  - [ ] Metrics streaming

- [ ] Implement methods:
  - [ ] `execute(sessionId)` - Main training loop
  - [ ] `loadSession(sessionId)` - Load from DB
  - [ ] `updateProgress(sessionId, progress)` - Update DB
  - [ ] `saveMetrics(sessionId, metrics)` - Insert metrics
  - [ ] `handleError(sessionId, error)` - Error handling

- [ ] Unit tests: `tests/unit/training-orchestrator.test.ts`
  - [ ] Test job execution
  - [ ] Test progress tracking
  - [ ] Test error handling

### Integration with PEFTLoRAAdapter

- [ ] Modify `system/genome/fine-tuning/server/adapters/PEFTLoRAAdapter.ts`
  - [ ] Export temp JSONL from prepared DB
  - [ ] Stream metrics during training
  - [ ] Save checkpoints every epoch
  - [ ] Log Python subprocess output

- [ ] Add metrics streaming:
  - [ ] Insert `TrainingMetricsEntity` every 100 steps
  - [ ] Parse loss from Python output
  - [ ] Track learning rate schedule

- [ ] Add checkpoint saving:
  - [ ] Save adapter state every epoch
  - [ ] Insert `TrainingCheckpointEntity`
  - [ ] Clean up old checkpoints

### Integration Tests

- [ ] Create `tests/integration/training-end-to-end.test.ts`
  - [ ] Test: Prepare dataset from JSONL
  - [ ] Test: Start training session
  - [ ] Test: Wait for completion
  - [ ] Test: Verify adapter saved
  - [ ] Test: Verify metrics recorded

- [ ] Test with small dataset:
  - [ ] 100 examples
  - [ ] TinyLlama-1.1B
  - [ ] 1 epoch
  - [ ] Should complete in ~5 minutes

### Documentation

- [ ] Update CLAUDE.md with training system section
- [ ] Add examples to TRAINING-SYSTEM-QUICK-REFERENCE.md
- [ ] Document new entities in ARCHITECTURE-RULES.md

### Phase 1 Success Criteria

- [ ] `./jtag training/prepare` imports JSONL → SQLite
- [ ] `./jtag training/start` queues training job
- [ ] Training runs end-to-end with PEFTLoRAAdapter
- [ ] TrainingSessionEntity tracks status and progress
- [ ] Adapter saved to `.continuum/genome/training/<session-id>/adapter/`
- [ ] All tests passing

---

## Phase 2: Status Monitoring & Metrics ⏱️ Week 2

### Command: training/status

- [ ] Create `commands/training/status/shared/TrainingStatusTypes.ts`
  - [ ] `TrainingStatusParams` interface
  - [ ] `TrainingStatusResult` interface

- [ ] Create `commands/training/status/server/TrainingStatusServerCommand.ts`
  - [ ] Load TrainingSessionEntity
  - [ ] Optionally load metrics
  - [ ] Optionally load logs
  - [ ] Return complete status

- [ ] Create browser command stub
- [ ] Register command
- [ ] Unit tests

### Command: training/metrics

- [ ] Create `commands/training/metrics/shared/TrainingMetricsTypes.ts`
- [ ] Create `commands/training/metrics/server/TrainingMetricsServerCommand.ts`
  - [ ] Query TrainingMetricsEntity time-series
  - [ ] Support sampling (every Nth point)
  - [ ] Compute summary statistics
- [ ] Create browser command stub
- [ ] Register command
- [ ] Unit tests

### Command: training/list

- [ ] Create `commands/training/list/shared/TrainingListTypes.ts`
- [ ] Create `commands/training/list/server/TrainingListServerCommand.ts`
  - [ ] Query sessions with filters
  - [ ] Support pagination
  - [ ] Sort by date (newest first)
- [ ] Create browser command stub
- [ ] Register command
- [ ] Unit tests

### Command: training/stop

- [ ] Create `commands/training/stop/shared/TrainingStopTypes.ts`
- [ ] Create `commands/training/stop/server/TrainingStopServerCommand.ts`
  - [ ] Load session
  - [ ] Kill Python subprocess (or cancel API job)
  - [ ] Optionally save checkpoint
  - [ ] Update status to 'cancelled'
- [ ] Create browser command stub
- [ ] Register command
- [ ] Unit tests

### Metrics Streaming

- [ ] Update `TrainingOrchestrator.execute()`
  - [ ] Insert `TrainingMetricsEntity` every 100 steps
  - [ ] Parse metrics from Python output
  - [ ] Handle parsing errors gracefully

- [ ] Update `PEFTLoRAAdapter.trainLoRA()`
  - [ ] Emit progress events
  - [ ] Pass sessionId for metrics tracking

### Integration Tests

- [ ] Test status polling during training
- [ ] Test metrics querying
- [ ] Test listing with filters
- [ ] Test stopping training mid-execution

### Phase 2 Success Criteria

- [ ] `./jtag training/status` shows real-time progress
- [ ] `./jtag training/metrics` returns loss curves
- [ ] `./jtag training/list` filters sessions correctly
- [ ] `./jtag training/stop` gracefully cancels training
- [ ] All tests passing

---

## Phase 3: Adapter Deployment ⏱️ Week 3

### Command: adapter/deploy

- [ ] Create `commands/adapter/deploy/shared/AdapterDeployTypes.ts`
- [ ] Create `commands/adapter/deploy/server/AdapterDeployServerCommand.ts`
  - [ ] Load TrainingSessionEntity
  - [ ] Validate adapter files exist
  - [ ] Create GenomeLayerEntity
  - [ ] Copy adapter files to genome/adapters/
  - [ ] Calculate embedding
  - [ ] Optionally add to GenomeEntity
  - [ ] Mark training examples as "used"
- [ ] Create browser command stub
- [ ] Register command
- [ ] Unit tests

### Command: adapter/test

- [ ] Create `commands/adapter/test/shared/AdapterTestTypes.ts`
- [ ] Create `commands/adapter/test/server/AdapterTestServerCommand.ts`
  - [ ] Load adapter
  - [ ] Load validation dataset
  - [ ] Run inference
  - [ ] Compute perplexity
  - [ ] Compare base vs fine-tuned outputs
  - [ ] Return recommendation
- [ ] Create browser command stub
- [ ] Register command
- [ ] Unit tests

### Command: adapter/version

- [ ] Create `commands/adapter/version/shared/AdapterVersionTypes.ts`
- [ ] Create `commands/adapter/version/server/AdapterVersionServerCommand.ts`
  - [ ] List all GenomeLayerEntities for persona + traitType
  - [ ] Sort by creation date
  - [ ] Include metrics comparison
- [ ] Create browser command stub
- [ ] Register command
- [ ] Unit tests

### Command: adapter/rollback

- [ ] Create `commands/adapter/rollback/shared/AdapterRollbackTypes.ts`
- [ ] Create `commands/adapter/rollback/server/AdapterRollbackServerCommand.ts`
  - [ ] Load target GenomeLayerEntity
  - [ ] Update GenomeEntity to use target layer
  - [ ] Remove newer layer from genome (keep in DB)
- [ ] Create browser command stub
- [ ] Register command
- [ ] Unit tests

### Integration with GenomeManager

- [ ] Modify `system/genome/fine-tuning/server/GenomeManager.ts`
  - [ ] Load deployed adapters from genome/adapters/
  - [ ] Track adapter usage (lastUsedAt)
  - [ ] Coordinate with LRU eviction
  - [ ] Check for stale adapters (30+ days)

### Integration Tests

- [ ] Test complete flow: train → test → deploy
- [ ] Test adapter quality validation
- [ ] Test genome stats after deployment
- [ ] Test version management
- [ ] Test rollback

### Phase 3 Success Criteria

- [ ] `./jtag adapter/deploy` creates GenomeLayerEntity
- [ ] Adapter files copied to correct location
- [ ] `./jtag adapter/test` validates quality
- [ ] `./jtag ai/genome/stats` shows deployed adapters
- [ ] All tests passing

---

## Phase 4: Continuous Learning ⏱️ Week 4

### PersonaUser Modifications

- [ ] Add fields to `system/user/server/PersonaUser.ts`:
  - [ ] `activeTrainingSessions: Set<UUID>`
  - [ ] `lastTrainingCheck: number`

- [ ] Implement `checkTrainingSessions()`:
  - [ ] Iterate `activeTrainingSessions`
  - [ ] Call `training/status` for each
  - [ ] Handle completed sessions
  - [ ] Handle failed sessions
  - [ ] Remove from set when done

- [ ] Implement `onTrainingCompleted()`:
  - [ ] Call `adapter/test`
  - [ ] If quality good, call `adapter/deploy`
  - [ ] Log improvement metrics
  - [ ] Notify user (optional)

- [ ] Implement `onTrainingFailed()`:
  - [ ] Log error
  - [ ] Create task for human review
  - [ ] Analyze failure cause

- [ ] Integrate into `serviceInbox()`:
  - [ ] Call `checkTrainingSessions()` every loop
  - [ ] Throttle to once per 10 seconds

### Self-Task Generation

- [ ] Implement `generateSelfTasks()`:
  - [ ] Count unused interactions
  - [ ] If >= 100, create training task
  - [ ] Set priority based on urgency

- [ ] Implement `countUnusedInteractions()`:
  - [ ] Query `chat_messages` where `senderId = this.entity.id`
  - [ ] Filter by `used_in_training = false`
  - [ ] Return count

- [ ] Modify `processTask()`:
  - [ ] Handle `taskType: 'training'`
  - [ ] Build TrainingDataset from conversations
  - [ ] Call `training/start` programmatically
  - [ ] Add sessionId to `activeTrainingSessions`

### Interaction Quality Scoring

- [ ] Add `used_in_training` field to ChatMessageEntity:
  - [ ] Boolean flag (default: false)
  - [ ] Update schema migration

- [ ] Implement quality scoring:
  - [ ] Track conversation coherence
  - [ ] Detect user corrections
  - [ ] Score based on feedback
  - [ ] Store in message metadata

- [ ] Mark messages as used:
  - [ ] After training completes
  - [ ] Update `used_in_training = true`
  - [ ] Use `data/update` command

### Automatic Retraining Triggers

- [ ] Time-based trigger:
  - [ ] Check `lastTrainedAt` on layer
  - [ ] If > 30 days, create training task

- [ ] Threshold-based trigger:
  - [ ] If 100+ unused interactions, create task

- [ ] Quality-based trigger:
  - [ ] Track response quality over time
  - [ ] If drop > 10%, create training task

### Integration Tests

- [ ] Test self-task generation after 100 interactions
- [ ] Test training completion handler
- [ ] Test auto-deployment
- [ ] Test quality scoring
- [ ] Test marking messages as used

### Phase 4 Success Criteria

- [ ] PersonaUser creates training task after 100 interactions
- [ ] Training runs automatically without human intervention
- [ ] Adapter auto-deploys if quality good
- [ ] System logs show continuous improvement
- [ ] All tests passing

---

## Phase 5: Multi-Provider Support ⏱️ Week 5

### OpenAI Integration

- [ ] Implement `system/genome/fine-tuning/server/adapters/OpenAILoRAAdapter.ts`
  - [ ] `uploadDataset()` - Upload JSONL to OpenAI API
  - [ ] `createFineTuneJob()` - Create fine-tuning job
  - [ ] `pollJobStatus()` - Poll for completion
  - [ ] `getFineTunedModel()` - Get model ID
  - [ ] Store API model ID in TrainingSessionEntity

- [ ] Add cost estimation:
  - [ ] Count tokens in dataset
  - [ ] Multiply by pricing (from FINE-TUNING-PROVIDER-RESEARCH.md)
  - [ ] Store in `estimatedCost`

- [ ] Add API key management:
  - [ ] Read from environment variable
  - [ ] Validate key before training
  - [ ] Handle quota errors

- [ ] Unit tests
- [ ] Integration tests with mock API

### DeepSeek Integration

- [ ] Implement `system/genome/fine-tuning/server/adapters/DeepSeekLoRAAdapter.ts`
  - [ ] Similar to OpenAI adapter
  - [ ] DeepSeek-specific API calls
  - [ ] Different pricing model

- [ ] Unit tests
- [ ] Integration tests with mock API

### Cost Tracking

- [ ] Add cost fields to TrainingSessionEntity:
  - [ ] `estimatedCost: number` (already exists)
  - [ ] `actualCost: number` (already exists)

- [ ] Update `training/start`:
  - [ ] Show estimated cost before training
  - [ ] Require confirmation if cost > threshold

- [ ] Update adapters:
  - [ ] Track actual cost from API response
  - [ ] Store in TrainingSessionEntity

- [ ] Add cost reporting:
  - [ ] Total cost per persona
  - [ ] Cost trends over time

### Provider Failover

- [ ] Implement `ProviderFailoverStrategy`:
  - [ ] If PEFT fails (GPU unavailable), try OpenAI
  - [ ] If OpenAI fails (quota), try DeepSeek
  - [ ] Log failover decisions

- [ ] Update `TrainingOrchestrator`:
  - [ ] Catch provider errors
  - [ ] Attempt failover
  - [ ] Update session with failover info

### Integration Tests

- [ ] Test OpenAI end-to-end (with mock API)
- [ ] Test DeepSeek end-to-end (with mock API)
- [ ] Test cost estimation
- [ ] Test failover logic

### Phase 5 Success Criteria

- [ ] `./jtag training/start --provider=openai` works end-to-end
- [ ] Cost estimate shown before training
- [ ] Actual cost tracked in session
- [ ] Failover works when provider fails
- [ ] All tests passing

---

## Phase 6: Production Hardening ⏱️ Week 6+

### Retry Logic

- [ ] Implement exponential backoff for API failures
- [ ] Implement resume from checkpoint on crash
- [ ] Add retry configuration (max attempts, backoff multiplier)

### Alerting

- [ ] Add training failure notifications
- [ ] Add quality degradation alerts
- [ ] Add cost threshold warnings
- [ ] Integrate with system notification daemon

### Monitoring Dashboard

- [ ] Design dashboard UI (browser widget?)
- [ ] Training session overview table
- [ ] Adapter quality trends chart
- [ ] Cost tracking chart
- [ ] Real-time progress for active sessions

### Performance Optimization

- [ ] Support parallel training sessions
- [ ] Batch dataset preparation
- [ ] Cache embeddings for similarity search
- [ ] Optimize database queries

### Security

- [ ] Validate training data for malicious examples
- [ ] Sandbox Python subprocess (use Docker?)
- [ ] Encrypt API keys in database
- [ ] Add RBAC for training commands

### Phase 6 Success Criteria

- [ ] System recovers from crashes automatically
- [ ] Alerts fire on training failures
- [ ] Dashboard shows real-time metrics
- [ ] No security vulnerabilities found in audit
- [ ] All tests passing

---

## Testing Strategy

### Unit Tests (Fast)

Run after every code change:
```bash
npx vitest tests/unit/training-*.test.ts --run
```

### Integration Tests (Slow)

Run before committing:
```bash
# Quick test (5 minutes)
npx vitest tests/integration/training-end-to-end.test.ts --run

# Full suite (30 minutes)
npx vitest tests/integration/ --run
```

### Manual Testing

Test with real models before deploying:
```bash
# TinyLlama test (5-10 minutes)
./jtag training/prepare --datasetPath=test-100-examples.jsonl
./jtag training/start --provider=peft \
  --baseModel=TinyLlama/TinyLlama-1.1B-Chat-v1.0 \
  --maxExamples=100 --hyperparameters='{"epochs": 1}'

# Phi-2 test (30 minutes)
./jtag training/start --provider=peft \
  --baseModel=microsoft/phi-2 \
  --maxExamples=500 --hyperparameters='{"epochs": 3}'
```

---

## Code Review Checklist

Before marking a phase complete:

### Type Safety
- [ ] No `any` types
- [ ] No `unknown` types without guards
- [ ] All entity fields have decorators
- [ ] All command params/results use factories

### Error Handling
- [ ] All async operations wrapped in try/catch
- [ ] Errors logged with context
- [ ] User-friendly error messages
- [ ] No silent failures

### Documentation
- [ ] JSDoc comments on all public methods
- [ ] README updated if needed
- [ ] CLAUDE.md updated with new commands
- [ ] Examples in QUICK-REFERENCE updated

### Testing
- [ ] Unit tests cover all branches
- [ ] Integration tests cover happy path
- [ ] Integration tests cover error cases
- [ ] Manual testing completed

### Performance
- [ ] No N+1 queries
- [ ] Database queries use indexes
- [ ] Large datasets streamed (not loaded in memory)
- [ ] No blocking operations on main thread

### Security
- [ ] User input validated
- [ ] SQL injection prevented (use parameterized queries)
- [ ] API keys never logged
- [ ] File paths validated (no directory traversal)

---

## Git Workflow

### Branch Strategy
```bash
# Create feature branch
git checkout -b feature/training-system-phase-1

# Make changes
git add .
git commit -m "feat: implement training/prepare command"

# Push to remote
git push -u origin feature/training-system-phase-1

# Create PR when phase complete
gh pr create --title "feat: Training System Phase 1" \
  --body "Implements core training infrastructure..."
```

### Commit Messages
```
feat: implement training/prepare command
fix: handle JSONL parsing errors
refactor: extract quality scoring logic
test: add unit tests for TrainingSessionEntity
docs: update CLAUDE.md with training commands
```

### PR Description Template
```markdown
## Phase N: [Phase Name]

### What Changed
- Implemented X command
- Added Y entity
- Integrated with Z system

### Testing
- [ ] Unit tests passing
- [ ] Integration tests passing
- [ ] Manual testing completed

### Screenshots/Logs
[Paste command output]

### Checklist
- [ ] All items from phase checklist completed
- [ ] Documentation updated
- [ ] No regressions in existing tests
```

---

## Common Pitfalls

### 1. Forgetting to run `npm start`
After editing TypeScript files, ALWAYS run:
```bash
cd src
npm start  # Wait 90+ seconds
```

### 2. Using relative imports instead of path aliases
```typescript
// ❌ WRONG
import { BaseEntity } from '../../../../system/data/entities/BaseEntity';

// ✅ CORRECT
import { BaseEntity } from '@system/data/entities/BaseEntity';
```

### 3. Mixing server/browser code in shared files
```typescript
// ❌ WRONG - shared/TrainingPrepareTypes.ts
import * as fs from 'fs'; // Node.js only!

// ✅ CORRECT - server/TrainingPrepareServerCommand.ts
import * as fs from 'fs'; // Server-only imports stay in server/
```

### 4. Not validating entity data
```typescript
// ❌ WRONG
const session = new TrainingSessionEntity();
await DataDaemon.create('training_sessions', session);

// ✅ CORRECT
const session = new TrainingSessionEntity();
const validation = session.validate();
if (!validation.success) {
  throw new Error(validation.error);
}
await DataDaemon.create('training_sessions', session);
```

### 5. Blocking the main thread
```typescript
// ❌ WRONG - blocks for 20 minutes
const result = await trainAdapter(dataset);

// ✅ CORRECT - returns immediately, poll for status
const sessionId = await startTraining(dataset);
// Later: poll with training/status
```

---

## Next Steps After Completion

Once all phases are complete:

1. **Production Deploy**
   - Deploy to staging environment
   - Run smoke tests
   - Monitor for errors
   - Deploy to production

2. **User Documentation**
   - Write user guide for training commands
   - Create video tutorials
   - Document common workflows

3. **Optimization**
   - Profile training performance
   - Optimize database queries
   - Add caching where appropriate

4. **Future Features**
   - DPO training (preference alignment)
   - Multi-GPU training
   - Distributed training
   - Adapter merging/composition

---

**Last Updated**: November 7, 2025
**Version**: 1.0
