# PersonaSubprocess Pattern - Making New Processes Trivial

**Inspired by cbar's `QueueThread<T>` architecture**

---

## The Pattern

### 1. Base Class Does Everything

```typescript
export abstract class PersonaSubprocess<T> {
  protected readonly persona: PersonaUser;  // Full access to parent

  // Base handles:
  // - Thread lifecycle (start/stop)
  // - Queue management (enqueue/flush)
  // - Priority-based timing
  // - Error handling
  // - Service loop

  // Implementations ONLY override this:
  protected abstract handleTask(task: T): Promise<boolean>;
}
```

### 2. Implementations Are Tiny

**Example: Self-Task Generation Subprocess (~50 lines)**

```typescript
interface TaskGenerationTask {
  type: 'check-tasks' | 'generate-task';
}

export class SelfTaskGenerationSubprocess extends PersonaSubprocess<TaskGenerationTask> {
  constructor(persona: PersonaUser) {
    super(persona, { priority: 'low', name: 'TaskGeneration' });
  }

  // This is ALL you implement
  protected async handleTask(task: TaskGenerationTask): Promise<boolean> {
    if (task.type === 'check-tasks') {
      return await this.checkForNeededTasks();
    } else {
      return await this.generateTask();
    }
  }

  private async checkForNeededTasks(): Promise<boolean> {
    // Access persona directly
    const capacity = await this.persona.workingMemory.getCapacity('global');

    if (capacity.used / capacity.max > 0.8) {
      this.enqueue({ type: 'generate-task' });
    }

    return true;
  }

  private async generateTask(): Promise<boolean> {
    // Create task directly in persona's inbox
    await this.persona.inbox.add({
      type: 'internal-task',
      priority: 0.6,
      data: { action: 'consolidate-memory' }
    });

    return true;
  }
}
```

**That's it!** ~50 lines, base class handles everything else.

---

## Continuous Processes (No Queue)

For always-running processes like memory consolidation:

```typescript
export abstract class PersonaContinuousSubprocess extends PersonaSubprocess<void> {
  // No queue, just continuous ticking

  protected abstract tick(): Promise<void>;
}
```

**Example: Memory Consolidation**

```typescript
export class MemoryConsolidationSubprocess extends PersonaContinuousSubprocess {
  constructor(persona: PersonaUser) {
    super(persona, { priority: 'low', name: 'MemoryConsolidation' });
  }

  // Called every cycle
  protected async tick(): Promise<void> {
    // Check persona's inbox (direct access)
    const inboxItems = await this.persona.inbox.peek(10);

    // Check persona's working memory (direct access)
    const thoughts = await this.persona.workingMemory.recall({ limit: 20 });

    // Detect patterns and decide
    if (await this.shouldConsolidate(inboxItems, thoughts)) {
      await this.consolidate();
    }
  }
}
```

---

## Adding New Subprocesses

### Step 1: Define Task Type (if using queue)

```typescript
interface MyTask {
  type: 'action1' | 'action2';
  data?: any;
}
```

### Step 2: Extend Base Class

```typescript
export class MySubprocess extends PersonaSubprocess<MyTask> {
  constructor(persona: PersonaUser) {
    super(persona, {
      priority: 'moderate',  // Choose priority
      name: 'MyProcess',
      maxQueueSize: 50
    });
  }

  protected async handleTask(task: MyTask): Promise<boolean> {
    // Implement your logic
    // Access persona directly: this.persona.*

    return true;
  }
}
```

### Step 3: Add to PersonaUser

```typescript
export class PersonaUser extends AIUser {
  private mySubprocess: MySubprocess;

  async initialize(): Promise<void> {
    // ... existing init

    this.mySubprocess = new MySubprocess(this);
    await this.mySubprocess.start();
  }

  async destroy(): Promise<void> {
    await this.mySubprocess.stop();
    // ... existing cleanup
  }
}
```

**Done!** That's the entire process.

---

## Subprocess Communication

### 1. Direct Property Access (Fastest)

```typescript
// Subprocess A accesses subprocess B through persona
protected async handleTask(task: MyTask): Promise<boolean> {
  // Access another subprocess directly
  const otherSubprocess = this.persona.getSubprocess(OtherSubprocess);

  if (otherSubprocess) {
    otherSubprocess.enqueue({ type: 'do-something' });
  }

  return true;
}
```

### 2. Enqueue Tasks (Non-Blocking)

```typescript
// Subprocess enqueues work for itself
this.enqueue({ type: 'follow-up-action' });

// Or for another subprocess
this.persona.someOtherSubprocess.enqueue({ type: 'action' });
```

### 3. Manual Wakeup (Urgent)

```typescript
// Wake up high-priority subprocess immediately
this.persona.memoryWorker.wakeup();
```

---

## Priority System

```typescript
type SubprocessPriority = 'highest' | 'high' | 'moderate' | 'default' | 'low' | 'lowest';

// Wait times (like cbar):
// highest: 10ms
// high: 50ms
// moderate: 100ms
// default: 200ms
// low: 500ms
// lowest: 1000ms
```

**Usage:**
- `highest`: Real-time chat response
- `high`: Tool execution
- `moderate`: Task processing
- `default`: General work
- `low`: Background consolidation
- `lowest`: Analytics, logging

---

## Examples of New Subprocesses

### 1. Continuous Learning Subprocess

```typescript
interface LearningTask {
  type: 'capture-interaction' | 'fine-tune';
  data: any;
}

export class ContinuousLearningSubprocess extends PersonaSubprocess<LearningTask> {
  constructor(persona: PersonaUser) {
    super(persona, { priority: 'lowest', name: 'ContinuousLearning' });
  }

  protected async handleTask(task: LearningTask): Promise<boolean> {
    if (task.type === 'capture-interaction') {
      // Capture interaction to training dataset
      await this.captureInteraction(task.data);
    } else if (task.type === 'fine-tune') {
      // Trigger fine-tuning job
      await this.triggerFineTuning();
    }

    return true;
  }

  private async captureInteraction(data: any): Promise<void> {
    // Access persona's genome directly
    await this.persona.genome.captureInteraction(data);
  }
}
```

### 2. Self-Task Generation Subprocess

```typescript
export class SelfTaskGenerationSubprocess extends PersonaContinuousSubprocess {
  constructor(persona: PersonaUser) {
    super(persona, { priority: 'low', name: 'SelfTaskGeneration' });
  }

  protected async tick(): Promise<void> {
    // Check if persona is idle
    const isIdle = this.persona.inbox.getDepth() === 0;

    if (isIdle) {
      // Generate self-task
      await this.generateIdleTask();
    }
  }

  private async generateIdleTask(): Promise<void> {
    // Create task in persona's inbox
    await this.persona.inbox.add({
      type: 'self-task',
      priority: 0.3,
      data: { action: 'memory-curation' }
    });
  }
}
```

### 3. Health Monitoring Subprocess

```typescript
interface HealthCheckTask {
  type: 'check-memory' | 'check-performance';
}

export class HealthMonitoringSubprocess extends PersonaSubprocess<HealthCheckTask> {
  constructor(persona: PersonaUser) {
    super(persona, { priority: 'low', name: 'HealthMonitoring' });
  }

  protected async handleTask(task: HealthCheckTask): Promise<boolean> {
    if (task.type === 'check-memory') {
      const capacity = await this.persona.workingMemory.getCapacity('global');

      if (capacity.used / capacity.max > 0.9) {
        console.warn(`⚠️ [${this.persona.displayName}] Memory pressure: ${capacity.used}/${capacity.max}`);

        // Trigger consolidation
        this.persona.memoryWorker.wakeup();
      }
    }

    return true;
  }

  // Periodic health checks
  protected async tick(): Promise<void> {
    this.enqueue({ type: 'check-memory' });
    this.enqueue({ type: 'check-performance' });
  }
}
```

---

## Benefits vs Old Approach

### Old (Slow, Complex):
```typescript
class MemoryConsolidationWorker {
  private running: boolean = false;

  constructor(personaId: UUID, inbox: PersonaInbox, memory: WorkingMemory, ...) {
    // Pass 10 properties individually
  }

  async start(): Promise<void> {
    this.running = true;
    setImmediate(() => this.serviceLoop());
  }

  private async serviceLoop(): Promise<void> {
    while (this.running) {
      try {
        // Manual loop logic
        const triggers = await this.checkTriggers();

        if (triggers.shouldConsolidate) {
          await this.consolidate();
        }

        await this.sleep(100);  // Manual timing
      } catch (error) {
        // Manual error handling
      }
    }
  }

  // ... 578 lines total
}
```

### New (Fast, Simple):
```typescript
class MemoryConsolidationSubprocess extends PersonaContinuousSubprocess {
  constructor(persona: PersonaUser) {
    super(persona, { priority: 'low' });  // One line
  }

  protected async tick(): Promise<void> {
    // Just implement logic
    // Base handles everything else
  }

  // ... ~100 lines total
}
```

**Reduction:** 578 lines → 100 lines (82% less code)

---

## Testing

Subprocesses are easy to test:

```typescript
describe('MySubprocess', () => {
  let persona: PersonaUser;
  let subprocess: MySubprocess;

  beforeEach(() => {
    persona = createTestPersona();
    subprocess = new MySubprocess(persona);
  });

  it('should process tasks', async () => {
    await subprocess.start();
    subprocess.enqueue({ type: 'action1' });

    // Wait for processing
    await new Promise(resolve => setTimeout(resolve, 300));

    expect(subprocess.getQueueSize()).toBe(0);
  });

  afterEach(async () => {
    await subprocess.stop();
  });
});
```

---

## Summary

**Adding a new subprocess:**
1. Define task type (if using queue)
2. Extend `PersonaSubprocess<T>` or `PersonaContinuousSubprocess`
3. Implement `handleTask()` or `tick()` (~20-50 lines)
4. Add to PersonaUser initialization

**No need to:**
- ❌ Implement service loop
- ❌ Handle queue management
- ❌ Implement timing logic
- ❌ Handle errors
- ❌ Pass properties individually
- ❌ Emit events

**Result:** Trivial to create new processes that can work independently or together, without bottlenecks.
