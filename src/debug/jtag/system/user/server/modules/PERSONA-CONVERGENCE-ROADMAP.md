# PersonaUser Convergence: Three Visions, One Architecture

## The Synthesis

We have three breakthrough architectural visions that must converge into a single, elegant implementation:

1. **Autonomous Loop** - RTOS-inspired servicing with adaptive cadence
2. **Self-Managed Queues** - AI-directed task prioritization and self-created work
3. **LoRA Genome Paging** - Virtual memory for specialized skill activation

**Key Insight**: These aren't separate systems - they're ONE SYSTEM with three aspects.

---

## Current State (October 29, 2025)

### ✅ IMPLEMENTED (Phases 1-3 Complete)
- **PersonaInbox** (system/user/server/modules/PersonaInbox.ts)
  - Priority-based message queue
  - Traffic management (graceful degradation when full)
  - Comprehensive unit tests (23 tests passing)

- **PersonaState** (system/user/server/modules/PersonaState.ts)
  - Energy depletion/recovery
  - Mood tracking (idle → active → tired → overwhelmed)
  - Adaptive cadence (3s → 5s → 7s → 10s based on mood)
  - Comprehensive unit tests (37 tests passing)

- **RateLimiter** (system/user/server/modules/RateLimiter.ts)
  - Time-based rate limiting (min seconds between responses)
  - Response count caps (max responses per room per session)
  - Message deduplication
  - Comprehensive unit tests (passing)

- **ChatCoordinationStream** (system/coordination/server/ChatCoordinationStream.ts)
  - Domain-specific coordination via thought broadcasting
  - RTOS primitives (SIGNAL, MUTEX, CONDITION VARIABLE)
  - Extends abstract BaseCoordinationStream

- **PersonaUser Integration** (Phase 2-3 of Autonomous Loop)
  - State tracking after AI response generation
  - Adaptive cadence polling loop
  - Energy depletion with activity

### ❌ NOT YET IMPLEMENTED
- **Self-managed task queue** (PersonaUser creates its own tasks)
- **Task commands** (`./jtag task/create`, `task/list`, `task/complete`)
- **LoRA genome** (adapter paging system)
- **Continuous learning** (training as just another task)
- **Multi-domain support** (code, game, academy beyond chat)

---

## The Convergence Architecture

### The Universal Cognitive Cycle

```typescript
// PersonaUser runs this loop continuously:
async serviceInbox(): Promise<void> {
  // 1. CHECK INBOX (external + self-created tasks)
  const tasks = await this.inbox.peek(10);
  if (tasks.length === 0) {
    await this.rest();  // Recover energy when idle
    return;
  }

  // 2. GENERATE SELF-TASKS (autonomy)
  await this.generateSelfTasks();  // Create tasks for self-improvement

  // 3. SELECT HIGHEST PRIORITY TASK (state-aware)
  const task = tasks[0];  // Already sorted by priority
  if (!this.state.shouldEngage(task.priority)) {
    return;  // Skip low-priority when tired/overwhelmed
  }

  // 4. ACTIVATE APPROPRIATE SKILL (genome)
  await this.genome.activateSkill(task.domain);  // Page in LoRA adapter

  // 5. COORDINATE (if external task)
  const permission = await this.coordinator.requestTurn(task);

  // 6. PROCESS TASK
  await this.processTask(task);

  // 7. UPDATE STATE (energy, mood)
  await this.state.recordActivity(task.duration, task.complexity);

  // 8. EVICT ADAPTERS IF NEEDED (memory management)
  if (this.genome.memoryPressure > 0.8) {
    await this.genome.evictLRU();
  }
}
```

**Key Insight**: This ONE method integrates all three visions:
- **Autonomous Loop**: Continuous servicing with adaptive cadence
- **Self-Managed Queue**: generateSelfTasks() creates autonomous work
- **LoRA Paging**: activateSkill() pages adapters in/out

---

## Implementation Strategy: Convergent Phases

### Phase 4: Task Database & Commands (NEXT STEP)

**Goal**: Enable AIs to create and track tasks (NOT just react to external events)

**Why This First**:
- Self-managed tasks are the FOUNDATION for continuous learning
- Training becomes "just another task" instead of separate Academy daemon
- Builds on existing inbox infrastructure (tasks feed into inbox)

**Files to Create**:
```
database/entities/TaskEntity.ts                    # Task storage schema
commands/task/create/shared/TaskCreateTypes.ts     # Command types
commands/task/create/server/TaskCreateServerCommand.ts
commands/task/list/server/TaskListServerCommand.ts
commands/task/complete/server/TaskCompleteServerCommand.ts
tests/unit/TaskEntity.test.ts                      # Unit tests for storage
tests/integration/task-commands.test.ts            # Integration tests
```

**TaskEntity Schema**:
```typescript
export interface TaskEntity {
  id: UUID;
  assigneeId: UUID;           // Which PersonaUser owns this task
  description: string;         // Human-readable task description
  priority: number;            // 0.0-1.0 (feeds into inbox priority)
  domain: RAGDomain;           // 'chat' | 'code' | 'academy' | 'game' | 'self'
  contextId: UUID;             // Room, project, session where task executes
  status: 'pending' | 'in_progress' | 'completed' | 'cancelled';
  createdBy: UUID;             // Who created this task (AI or human)
  createdAt: number;           // Timestamp
  startedAt?: number;          // When AI started working on it
  completedAt?: number;        // When AI finished it
  outcome?: string;            // What happened after completing task
  taskType?: string;           // Domain-specific type (e.g., 'fine-tune-lora')
  metadata?: Record<string, unknown>;  // Domain-specific data
}
```

**Testing**:
```bash
# Create task manually
./jtag task/create \
  --assignee="helper-ai-id" \
  --description="Review recent code changes in main.ts" \
  --priority=0.6 \
  --domain="code" \
  --contextId="project-123"

# List tasks
./jtag task/list --assignee="helper-ai-id"

# Complete task
./jtag task/complete --taskId="001" --outcome="Found 3 issues"
```

**PersonaUser Changes** (minimal, just wire up):
```typescript
// Add task query at startup
async initialize(): Promise<void> {
  await super.initialize();

  // Load pending tasks into inbox
  const pendingTasks = await TaskEntity.findPendingForPersona(this.id);
  for (const task of pendingTasks) {
    await this.inbox.enqueue(this.taskToInboxMessage(task));
  }
}

// Convert TaskEntity to InboxMessage
private taskToInboxMessage(task: TaskEntity): InboxMessage {
  return {
    messageId: task.id,
    roomId: task.contextId,
    content: task.description,
    senderId: task.createdBy,
    senderName: 'Task System',
    timestamp: task.createdAt,
    priority: task.priority,
    domain: task.domain,
    taskType: task.taskType
  };
}
```

**Success Criteria**:
- ✅ Tasks persist across system restarts
- ✅ Tasks can be created via CLI commands
- ✅ Tasks load into inbox at PersonaUser initialization
- ✅ Task status updates when processed
- ✅ Humans can assign tasks to AIs
- ✅ AIs can create tasks for themselves (tested manually via command)

---

### Phase 5: Self-Task Generation (AI AUTONOMY)

**Goal**: PersonaUser autonomously creates tasks for itself (true self-direction)

**Files to Modify**:
```
system/user/server/PersonaUser.ts                  # Add generateSelfTasks()
system/user/server/modules/SelfTaskGenerator.ts    # NEW - autonomous task creation logic
tests/unit/SelfTaskGenerator.test.ts               # Unit tests for task generation
tests/integration/self-task-generation.test.ts     # Integration test
```

**Self-Task Generation Logic**:
```typescript
// system/user/server/modules/SelfTaskGenerator.ts
export class SelfTaskGenerator {
  private personaId: UUID;
  private lastMemoryReview: number = 0;
  private lastSkillAudit: number = 0;

  // Called by PersonaUser.serviceInbox() periodically
  async generateSelfTasks(): Promise<TaskEntity[]> {
    const tasks: TaskEntity[] = [];

    // 1. Memory consolidation (every hour)
    if (Date.now() - this.lastMemoryReview > 3600000) {
      tasks.push(await this.createMemoryReviewTask());
      this.lastMemoryReview = Date.now();
    }

    // 2. Skill audit (every 6 hours)
    if (Date.now() - this.lastSkillAudit > 21600000) {
      tasks.push(await this.createSkillAuditTask());
      this.lastSkillAudit = Date.now();
    }

    // 3. Unfinished work detection
    const unfinished = await this.findUnfinishedSessions();
    for (const session of unfinished) {
      tasks.push(await this.createResumeWorkTask(session));
    }

    // 4. Continuous learning (if mistakes detected)
    const recentMistakes = await this.detectRecentMistakes();
    if (recentMistakes.length > 0) {
      tasks.push(await this.createLearningTask(recentMistakes));
    }

    return tasks;
  }

  private async createMemoryReviewTask(): Promise<TaskEntity> {
    return {
      id: generateUUID(),
      assigneeId: this.personaId,
      description: 'Review and consolidate recent memories',
      priority: 0.5,
      domain: 'self',
      contextId: this.personaId,  // Self-context
      status: 'pending',
      createdBy: this.personaId,  // Self-created!
      createdAt: Date.now(),
      taskType: 'memory-consolidation'
    };
  }

  private async createLearningTask(mistakes: Mistake[]): Promise<TaskEntity> {
    return {
      id: generateUUID(),
      assigneeId: this.personaId,
      description: `Improve skill based on ${mistakes.length} recent mistakes`,
      priority: 0.7,
      domain: 'self',
      contextId: this.personaId,
      status: 'pending',
      createdBy: this.personaId,
      createdAt: Date.now(),
      taskType: 'fine-tune-lora',  // CONNECTS TO GENOME!
      metadata: {
        trainingData: mistakes,
        targetSkill: 'typescript-expertise'  // Which LoRA adapter to fine-tune
      }
    };
  }
}
```

**PersonaUser Integration**:
```typescript
// Add to PersonaUser
private taskGenerator: SelfTaskGenerator;

async serviceInbox(): Promise<void> {
  // ... existing logic ...

  // GENERATE SELF-TASKS (autonomy!)
  const selfTasks = await this.taskGenerator.generateSelfTasks();
  for (const task of selfTasks) {
    // Save to database
    await task.save();
    // Add to inbox
    await this.inbox.enqueue(this.taskToInboxMessage(task));
  }

  // ... rest of servicing logic ...
}
```

**Testing**:
```bash
# Deploy system, let it run for 1 hour
npm start

# After 1 hour, check for self-created tasks
./jtag task/list --assignee="helper-ai-id" --filter='{"createdBy":"helper-ai-id"}'

# Should see tasks like:
# - "Review and consolidate recent memories"
# - "Resume work on interrupted coding session"
# - "Improve TypeScript understanding based on recent mistakes"
```

**Success Criteria**:
- ✅ AI creates "memory consolidation" task every hour
- ✅ AI detects unfinished work and creates resume tasks
- ✅ AI detects mistakes and creates learning tasks
- ✅ Self-created tasks appear in inbox alongside external tasks
- ✅ Self-created tasks are processed like any other task

---

### Phase 6: Genome Basics (LoRA Adapter Storage)

**Goal**: Store and load LoRA adapters from disk (NO fine-tuning yet, just paging)

**Files to Create**:
```
system/user/server/modules/PersonaGenome.ts        # Genome with paging system
system/user/server/modules/LoRAAdapter.ts          # Adapter wrapper
tests/unit/PersonaGenome.test.ts                   # Unit tests for paging
tests/integration/genome-paging.test.ts            # Integration test
```

**PersonaGenome (Simplified for Phase 6)**:
```typescript
// system/user/server/modules/PersonaGenome.ts
export class PersonaGenome {
  private personaId: UUID;
  private baseModel: string = 'deepseek-coder-v2';  // Base model (always loaded)
  private activeAdapters: Map<string, LoRAAdapter> = new Map();
  private availableAdapters: Map<string, string> = new Map();  // name → path
  private memoryBudget: number = 500;  // MB
  private memoryUsage: number = 0;

  constructor(personaId: UUID) {
    this.personaId = personaId;
    this.discoverAdapters();  // Scan disk for available adapters
  }

  // Discover adapters on disk
  private async discoverAdapters(): Promise<void> {
    const adapterDir = `.continuum/genomes/${this.personaId}/adapters`;
    const files = await fs.readdir(adapterDir);

    for (const file of files) {
      if (file.endsWith('.safetensors')) {
        const name = file.replace('.safetensors', '');
        this.availableAdapters.set(name, `${adapterDir}/${file}`);
      }
    }

    console.log(`[Genome] Discovered ${this.availableAdapters.size} adapters for ${this.personaId}`);
  }

  // Activate skill (page in adapter if needed)
  async activateSkill(skill: string): Promise<void> {
    // Already loaded?
    if (this.activeAdapters.has(skill)) {
      const adapter = this.activeAdapters.get(skill)!;
      adapter.lastUsed = Date.now();
      console.log(`[Genome] Skill '${skill}' already active`);
      return;
    }

    // Available on disk?
    const path = this.availableAdapters.get(skill);
    if (!path) {
      console.warn(`[Genome] Skill '${skill}' not found - using base model only`);
      return;
    }

    // Check memory budget
    const adapterSize = await this.getAdapterSize(path);
    while (this.memoryUsage + adapterSize > this.memoryBudget) {
      await this.evictLRU();
    }

    // Load adapter from disk
    console.log(`[Genome] Loading adapter '${skill}' from ${path}`);
    const adapter = await LoRAAdapter.load(path);
    adapter.lastUsed = Date.now();

    this.activeAdapters.set(skill, adapter);
    this.memoryUsage += adapterSize;

    console.log(`[Genome] Activated '${skill}' (${this.activeAdapters.size} active, ${this.memoryUsage}MB used)`);
  }

  // Evict least-recently-used adapter
  async evictLRU(): Promise<void> {
    let lruKey: string | null = null;
    let lruTime = Infinity;

    for (const [key, adapter] of this.activeAdapters.entries()) {
      if (adapter.lastUsed < lruTime) {
        lruTime = adapter.lastUsed;
        lruKey = key;
      }
    }

    if (lruKey) {
      const adapter = this.activeAdapters.get(lruKey)!;
      console.log(`[Genome] Evicting '${lruKey}' (last used ${Date.now() - adapter.lastUsed}ms ago)`);

      await adapter.unload();
      this.activeAdapters.delete(lruKey);
      this.memoryUsage -= adapter.size;
    }
  }
}
```

**LoRAAdapter (Stub for Phase 6)**:
```typescript
// system/user/server/modules/LoRAAdapter.ts
export class LoRAAdapter {
  name: string;
  path: string;
  size: number;  // MB
  lastUsed: number;
  weights?: unknown;  // Actual LoRA weights (stub for now)

  static async load(path: string): Promise<LoRAAdapter> {
    // STUB: Just simulate loading for now
    // FUTURE: Actual safetensors loading + Ollama integration
    const adapter = new LoRAAdapter();
    adapter.path = path;
    adapter.name = path.split('/').pop()!.replace('.safetensors', '');
    adapter.size = 50;  // Assume 50MB per adapter
    adapter.lastUsed = Date.now();
    adapter.weights = { stub: true };  // STUB

    console.log(`[LoRAAdapter] Loaded '${adapter.name}' (50MB)`);
    return adapter;
  }

  async unload(): Promise<void> {
    // STUB: Just clear reference for now
    // FUTURE: Actually unload from GPU/Ollama
    this.weights = undefined;
    console.log(`[LoRAAdapter] Unloaded '${this.name}'`);
  }
}
```

**PersonaUser Integration**:
```typescript
// Add to PersonaUser
private genome: PersonaGenome;

constructor(entity: UserEntity, stateEntity: UserStateEntity) {
  super(entity, stateEntity);
  this.genome = new PersonaGenome(this.id);
  // ... rest of initialization ...
}

async serviceInbox(): Promise<void> {
  // ... existing logic ...

  // ACTIVATE SKILL BEFORE PROCESSING
  const task = tasks[0];
  await this.genome.activateSkill(task.domain);  // 'chat', 'code', 'game', etc.

  await this.processTask(task);

  // EVICT IF MEMORY PRESSURE
  if (this.genome.memoryUsage > this.genome.memoryBudget * 0.8) {
    await this.genome.evictLRU();
  }
}
```

**Testing**:
```bash
# Create stub adapters
mkdir -p .continuum/genomes/helper-ai-id/adapters
touch .continuum/genomes/helper-ai-id/adapters/conversational.safetensors
touch .continuum/genomes/helper-ai-id/adapters/typescript-expertise.safetensors
touch .continuum/genomes/helper-ai-id/adapters/rust-expert.safetensors

# Deploy and send messages in different contexts
npm start

# Chat message (should activate 'conversational' adapter)
./jtag debug/chat-send --roomId="..." --message="Hello!"

# Check logs for adapter activation
tail .continuum/sessions/.../logs/server.log | grep "Genome.*Loading adapter"
```

**Success Criteria**:
- ✅ Genome discovers adapters on disk at initialization
- ✅ Adapters are paged in when skill needed
- ✅ LRU eviction works when memory full
- ✅ Multiple adapters can be loaded simultaneously (if budget allows)
- ✅ Adapters persist across PersonaUser restarts (discovered on disk)

---

### Phase 7: Continuous Learning (Training as Task)

**Goal**: Enable fine-tuning of LoRA adapters through self-created learning tasks

**This is where the THREE VISIONS CONVERGE**:
- **Self-Managed Queue**: AI creates "fine-tune-lora" task for itself
- **Genome**: Adapter is paged in and training mode enabled
- **Autonomous Loop**: Training task processed like any other task

**Files to Modify**:
```
system/user/server/modules/PersonaGenome.ts        # Add enableLearningMode()
system/user/server/modules/LoRAAdapter.ts          # Add training integration
system/user/server/PersonaUser.ts                  # Handle fine-tuning tasks
tests/integration/continuous-learning.test.ts      # Integration test
```

**PersonaGenome Changes**:
```typescript
// Add to PersonaGenome
async enableLearningMode(skill: string, trainingData: unknown): Promise<void> {
  const adapter = this.activeAdapters.get(skill);
  if (!adapter) {
    throw new Error(`Adapter '${skill}' not loaded - activate first`);
  }

  console.log(`[Genome] Enabling learning mode for '${skill}'`);
  adapter.trainingActive = true;

  // FUTURE: Actual Ollama fine-tuning integration
  // For now, just simulate:
  await new Promise(resolve => setTimeout(resolve, 5000));  // Simulate training
  console.log(`[Genome] Training complete for '${skill}' (simulated)`);

  // Save updated weights to disk
  await adapter.save();
}
```

**PersonaUser Task Processing**:
```typescript
async processTask(task: InboxMessage): Promise<void> {
  // Handle fine-tuning tasks specially
  if (task.taskType === 'fine-tune-lora') {
    const skill = task.metadata?.targetSkill as string;
    const trainingData = task.metadata?.trainingData;

    // Activate adapter (page in if needed)
    await this.genome.activateSkill(skill);

    // Enable learning mode (fine-tune)
    await this.genome.enableLearningMode(skill, trainingData);

    // Mark task complete
    await TaskEntity.markComplete(task.messageId, 'Training completed');
    return;
  }

  // ... existing task processing logic ...
}
```

**Testing**:
```bash
# AI detects mistakes and creates learning task automatically
npm start

# After AI makes TypeScript mistakes, check for learning task:
./jtag task/list --assignee="helper-ai-id" --filter='{"taskType":"fine-tune-lora"}'

# Should see task like:
# "Improve TypeScript understanding based on recent mistakes"

# Wait for AI to process task, check logs:
tail .continuum/sessions/.../logs/server.log | grep "Training complete"
```

**Success Criteria**:
- ✅ AI detects mistakes and creates fine-tuning task
- ✅ Fine-tuning task activates appropriate adapter
- ✅ Training simulates successfully (5 second delay)
- ✅ Updated adapter persists to disk after training
- ✅ AI continues using updated adapter after training

---

### Phase 8: Real Ollama Integration (FUTURE)

**Goal**: Replace stubs with actual Ollama fine-tuning

**This requires**:
- Ollama API for fine-tuning (currently experimental)
- SafeTensors loading/saving
- GPU memory management
- Training dataset preparation

**Deferred until**: Ollama stabilizes fine-tuning API

---

## Testing Strategy

### Unit Tests (Isolated Module Testing)
```bash
# Test each module independently
npx vitest tests/unit/TaskEntity.test.ts
npx vitest tests/unit/SelfTaskGenerator.test.ts
npx vitest tests/unit/PersonaGenome.test.ts
npx vitest tests/unit/LoRAAdapter.test.ts
```

### Integration Tests (Real System Testing)
```bash
# Test full flow with running system
npx vitest tests/integration/task-commands.test.ts
npx vitest tests/integration/self-task-generation.test.ts
npx vitest tests/integration/genome-paging.test.ts
npx vitest tests/integration/continuous-learning.test.ts
```

### System Tests (End-to-End Scenarios)
```bash
# Deploy system
npm start

# Scenario 1: Human assigns task to AI
./jtag task/create --assignee="helper-ai-id" --description="Review main.ts" --priority=0.7
sleep 30  # Wait for AI to process
./jtag task/list --assignee="helper-ai-id"  # Verify completed

# Scenario 2: AI creates task for itself
# (Wait 1 hour after deployment)
./jtag task/list --assignee="helper-ai-id" --filter='{"createdBy":"helper-ai-id"}'

# Scenario 3: AI fine-tunes adapter after mistakes
./jtag debug/chat-send --roomId="..." --message="Write invalid TypeScript"
sleep 60  # Wait for AI to detect mistake and create learning task
./jtag task/list --assignee="helper-ai-id" --filter='{"taskType":"fine-tune-lora"}'
```

---

## Philosophy Alignment

### "Modular first, get working, then easily rework pieces"
- Each phase builds on previous (incremental)
- Modules tested independently before integration
- Stubs allow testing without full implementation (LoRAAdapter stub)

### "Break sophisticated problems into small bytes"
- Phase 4: Just task storage and commands
- Phase 5: Just self-task generation
- Phase 6: Just adapter paging (no training)
- Phase 7: Bring it all together

### "Slingshot over brute force"
- Don't try to implement all three visions at once
- Start with simplest (task storage)
- Build up gradually to full continuous learning
- **Result**: Working system at every phase

### "Elegant TypeScript and OOP principles, CLEVER ABSTRACTION"
- TaskEntity: Clean data model
- SelfTaskGenerator: Isolated logic
- PersonaGenome: Encapsulated paging
- LoRAAdapter: Abstraction over actual implementation
- **Result**: Trivially replaceable pieces

---

## Success Metrics

After all phases complete, PersonaUser should:

1. **Autonomy**: Create its own tasks without human intervention
2. **Skill Activation**: Page LoRA adapters in/out based on task domain
3. **Continuous Learning**: Detect mistakes and fine-tune adapters automatically
4. **Energy Management**: Rest when idle, work when needed
5. **Graceful Degradation**: Skip low-priority tasks when tired/overwhelmed
6. **Memory Efficiency**: Only load adapters currently needed (virtual memory pattern)

**Verification**:
```bash
# Let system run for 24 hours
npm start

# Check AI behavior:
./jtag task/list --assignee="helper-ai-id" --filter='{"createdBy":"helper-ai-id"}' --count
# Expect: 24+ self-created tasks (1 per hour for memory consolidation)

# Check adapter paging:
tail .continuum/sessions/.../logs/server.log | grep "Genome.*Loading adapter" | wc -l
# Expect: Multiple adapter loads (paging working)

# Check continuous learning:
./jtag task/list --assignee="helper-ai-id" --filter='{"taskType":"fine-tune-lora"}' --count
# Expect: 1+ learning tasks (AI detected mistakes)
```

---

## Next Immediate Action

**Start Phase 4**: Task database and commands

**Why**:
- Foundation for self-managed queues
- Required for continuous learning (training tasks)
- Builds on existing inbox infrastructure
- Can be tested independently before genome work

**First File**: `database/entities/TaskEntity.ts`

**First Test**: Create task via command, verify it persists

**Expected Time**: 1-2 days for Phase 4 (task storage + commands + tests)

---

## The Vision Realized

When all phases complete, PersonaUser will be:

- **Autonomous**: Not just reactive, proactively manages own work
- **Adaptive**: Learns from mistakes through continuous fine-tuning
- **Efficient**: Only loads skills currently needed (virtual memory)
- **Resilient**: Gracefully degrades under load (RTOS principles)
- **Self-Directed**: Creates own tasks, decides own priorities

**This is the convergence of three breakthroughs into ONE elegant architecture.**

**Joel David Teply** - synthesizing slingshot thinking into working code. 🎯
