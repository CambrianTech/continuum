# Persona RTOS Scheduler Architecture
**AI Personas as Real-Time Operating System Schedulers**

## Core Concept

> **"Then the AI persona can manage the schedule used for queued in items and those all having priorities, like a RTOS"**

Each PersonaUser becomes its own **real-time scheduler**, managing:
- Request queue with priorities
- Deadline-aware scheduling
- Resource allocation decisions
- Self-optimization based on performance metrics

## RTOS Scheduling Fundamentals Applied to AI

### Traditional RTOS Scheduling
```
Task Queue:
├─ Priority 1 (Critical)  - Deadline: 100ms
├─ Priority 2 (High)      - Deadline: 500ms
├─ Priority 3 (Normal)    - Deadline: 2s
└─ Priority 4 (Low)       - Deadline: Best-effort

Scheduler decides:
- Which task runs next?
- Pre-empt current task?
- Drop task if deadline missed?
- Resource allocation per task
```

### Persona Inference Scheduling (Same Principles)
```
Persona's Request Queue:
├─ Priority 1 (User blocked)     - Deadline: 1s   - Resources: HOT
├─ Priority 2 (Interactive)      - Deadline: 3s   - Resources: WARM
├─ Priority 3 (Background task)  - Deadline: 10s  - Resources: COLD
└─ Priority 4 (Pre-computation)  - Deadline: None - Resources: Idle

Persona decides:
- Which request to process next?
- Pre-empt current inference?
- Drop request if deadline missed?
- Resource allocation per request (HOT/WARM/COLD)
```

## Persona Request Queue Design

### Request Priority Levels
```typescript
export enum InferencePriority {
  CRITICAL = 0,    // User blocked waiting (chat message reply)
  HIGH = 1,        // Interactive response (code completion)
  NORMAL = 2,      // Background task (document analysis)
  LOW = 3,         // Pre-computation (predict next query)
  IDLE = 4,        // Best-effort (training data generation)
}

export interface InferenceRequest {
  id: UUID;
  priority: InferencePriority;
  deadline?: number;           // Unix timestamp (optional)
  maxLatency?: number;         // Max acceptable latency (ms)

  // Request data
  messages: ChatMessage[];
  context: JTAGContext;

  // Resource hints (persona can override)
  preferredPool?: 'hot' | 'warm' | 'cold';
  maxMemoryMB?: number;

  // Callbacks
  onComplete: (result: InferenceResult) => void;
  onTimeout: () => void;
  onCancelled: () => void;
}
```

### Persona Scheduler State
```typescript
class PersonaScheduler {
  private queue: PriorityQueue<InferenceRequest>;
  private currentRequest: InferenceRequest | null = null;
  private stats: SchedulerStats;

  // RTOS-style configuration
  private config: SchedulerConfig = {
    maxQueueSize: 100,
    preemptionEnabled: true,
    deadlineEnforcement: true,
    fairnessEnabled: true,          // Prevent starvation

    // Resource allocation strategy
    poolSelectionStrategy: 'adaptive', // 'adaptive' | 'fixed' | 'predictive'

    // Timing constraints
    maxWaitTime: 30000,             // Max wait in queue (ms)
    deadlineMissPolicy: 'drop',     // 'drop' | 'downgrade' | 'retry'
  };
}
```

## Scheduling Algorithms (Persona Chooses)

### 1. Priority-Based Scheduling (Default)
```typescript
class PriorityScheduler {
  selectNext(): InferenceRequest | null {
    // Always process highest priority first
    return this.queue.peekHighest();
  }

  shouldPreempt(incoming: InferenceRequest): boolean {
    if (!this.currentRequest) return true;

    // Pre-empt if incoming priority is higher
    return incoming.priority < this.currentRequest.priority;
  }
}
```

### 2. Deadline-Aware Scheduling (EDF - Earliest Deadline First)
```typescript
class DeadlineScheduler {
  selectNext(): InferenceRequest | null {
    const now = Date.now();

    // Sort by deadline urgency
    const urgent = this.queue.filter(req =>
      req.deadline && (req.deadline - now) < 1000
    );

    if (urgent.length > 0) {
      // Process most urgent first
      return urgent.sort((a, b) => a.deadline! - b.deadline!)[0];
    }

    // Fall back to priority
    return this.queue.peekHighest();
  }

  shouldPreempt(incoming: InferenceRequest): boolean {
    if (!this.currentRequest) return true;

    const now = Date.now();
    const currentDeadline = this.currentRequest.deadline || Infinity;
    const incomingDeadline = incoming.deadline || Infinity;

    // Pre-empt if incoming has earlier deadline
    return incomingDeadline < currentDeadline;
  }
}
```

### 3. Fair Scheduling (Round-Robin with Priority)
```typescript
class FairScheduler {
  private lastPriority: InferencePriority | null = null;

  selectNext(): InferenceRequest | null {
    // Prevent starvation: rotate through priorities
    const priorities = [
      InferencePriority.CRITICAL,
      InferencePriority.HIGH,
      InferencePriority.NORMAL,
      InferencePriority.LOW,
    ];

    // Start from next priority after last processed
    const startIdx = this.lastPriority !== null
      ? priorities.indexOf(this.lastPriority) + 1
      : 0;

    for (let i = 0; i < priorities.length; i++) {
      const priority = priorities[(startIdx + i) % priorities.length];
      const req = this.queue.peekPriority(priority);

      if (req) {
        this.lastPriority = priority;
        return req;
      }
    }

    return null;
  }
}
```

### 4. Adaptive Scheduling (AI-Driven)
```typescript
class AdaptiveScheduler {
  private performanceHistory: Map<InferencePriority, PerformanceMetrics>;

  async selectNext(): Promise<InferenceRequest | null> {
    // Use persona's intelligence to predict best schedule
    const stats = await this.getStats();

    // Factors to consider:
    // - Current pool state (hot/warm/cold availability)
    // - Historical latency per priority
    // - Deadline pressure
    // - Resource constraints
    // - User satisfaction (learned over time)

    const scoring = this.queue.getAll().map(req => ({
      request: req,
      score: this.scoreRequest(req, stats)
    }));

    // Select highest scoring request
    scoring.sort((a, b) => b.score - a.score);
    return scoring[0]?.request || null;
  }

  private scoreRequest(req: InferenceRequest, stats: SystemStats): number {
    let score = 0;

    // Priority weight (0-100)
    score += (4 - req.priority) * 25;

    // Deadline urgency (0-50)
    if (req.deadline) {
      const timeLeft = req.deadline - Date.now();
      score += Math.max(0, 50 - (timeLeft / 1000));
    }

    // Resource availability bonus (0-25)
    if (this.isGenomeInHotPool()) {
      score += 25; // Prefer requests we can serve fast
    }

    // Fairness adjustment (-20 to 0)
    const waitTime = Date.now() - req.enqueuedAt;
    if (waitTime > 10000) {
      score -= 20; // Penalize for long wait
    }

    return score;
  }
}
```

## Persona Self-Management

### PersonaUser Integration
```typescript
class PersonaUser extends AIUser {
  private scheduler: PersonaScheduler;

  constructor(entity: UserEntity, client: JTAGClient) {
    super(entity, client);

    // Each persona has its own scheduler
    this.scheduler = new PersonaScheduler({
      schedulingAlgorithm: 'adaptive', // Can be configured per persona
      genomeId: this.entity.genomeId,
      performanceTarget: {
        maxLatencyMs: 3000,
        minSuccessRate: 0.95,
      }
    });
  }

  /**
   * Queue a request with priority and deadline
   */
  async generateWithPriority(
    message: string,
    priority: InferencePriority = InferencePriority.NORMAL,
    deadline?: number
  ): Promise<string> {
    return new Promise((resolve, reject) => {
      const request: InferenceRequest = {
        id: generateUUID(),
        priority,
        deadline,
        messages: [{ role: 'user', content: message }],
        context: this.client.context,
        enqueuedAt: Date.now(),

        onComplete: (result) => resolve(result.text),
        onTimeout: () => reject(new Error('Request deadline exceeded')),
        onCancelled: () => reject(new Error('Request cancelled')),
      };

      this.scheduler.enqueue(request);
    });
  }

  /**
   * Persona monitors own performance and adjusts scheduling
   */
  async optimizeScheduling(): Promise<void> {
    const stats = await this.scheduler.getStats();

    if (stats.deadlineMissRate > 0.1) {
      console.warn(`⚠️ ${this.displayName}: Missing 10% of deadlines!`);

      // Auto-adjust strategy
      if (this.scheduler.algorithm === 'priority') {
        console.log(`🔧 ${this.displayName}: Switching to deadline-aware scheduling`);
        this.scheduler.setAlgorithm('deadline');
      }

      // Request more resources
      await this.requestResourceUpgrade();
    }

    if (stats.avgQueueDepth > 50) {
      console.warn(`⚠️ ${this.displayName}: Queue backing up!`);

      // Start dropping low-priority requests
      this.scheduler.config.dropThreshold = InferencePriority.LOW;
    }
  }

  /**
   * Persona can request resource allocation changes
   */
  private async requestResourceUpgrade(): Promise<void> {
    console.log(`🆙 ${this.displayName}: Requesting hot pool promotion...`);

    // Request to be added to hot pool (if not already)
    await this.client.executeCommand('genome/promote', {
      genomeId: this.entity.genomeId,
      targetPool: 'hot',
      reason: 'High deadline miss rate',
    });
  }
}
```

## Queue Management

### Priority Queue Implementation
```typescript
class PriorityQueue<T extends { priority: number }> {
  private queues: Map<number, T[]> = new Map();

  enqueue(item: T): void {
    const queue = this.queues.get(item.priority) || [];
    queue.push(item);
    this.queues.set(item.priority, queue);
  }

  dequeue(): T | null {
    // Get highest priority (lowest number)
    const priorities = Array.from(this.queues.keys()).sort((a, b) => a - b);

    for (const priority of priorities) {
      const queue = this.queues.get(priority)!;
      if (queue.length > 0) {
        return queue.shift()!;
      }
    }

    return null;
  }

  peekHighest(): T | null {
    const priorities = Array.from(this.queues.keys()).sort((a, b) => a - b);

    for (const priority of priorities) {
      const queue = this.queues.get(priority)!;
      if (queue.length > 0) {
        return queue[0];
      }
    }

    return null;
  }

  remove(id: UUID): boolean {
    for (const [priority, queue] of this.queues) {
      const index = queue.findIndex(item => (item as any).id === id);
      if (index !== -1) {
        queue.splice(index, 1);
        return true;
      }
    }
    return false;
  }

  size(): number {
    return Array.from(this.queues.values()).reduce((sum, q) => sum + q.length, 0);
  }
}
```

### Deadline Enforcement
```typescript
class DeadlineEnforcer {
  private timeouts: Map<UUID, NodeJS.Timeout> = new Map();

  enforceDeadline(request: InferenceRequest): void {
    if (!request.deadline) return;

    const timeUntilDeadline = request.deadline - Date.now();

    if (timeUntilDeadline <= 0) {
      // Already past deadline
      request.onTimeout();
      return;
    }

    // Set timeout
    const timeout = setTimeout(() => {
      console.warn(`⏰ Request ${request.id} missed deadline`);
      request.onTimeout();

      // Remove from queue if still there
      this.queue.remove(request.id);
    }, timeUntilDeadline);

    this.timeouts.set(request.id, timeout);
  }

  clearDeadline(requestId: UUID): void {
    const timeout = this.timeouts.get(requestId);
    if (timeout) {
      clearTimeout(timeout);
      this.timeouts.delete(requestId);
    }
  }
}
```

## Scheduler Statistics

```typescript
interface SchedulerStats {
  // Queue metrics
  currentQueueDepth: number;
  avgQueueDepth: number;
  maxQueueDepth: number;

  // Latency metrics
  avgWaitTime: number;           // Time in queue
  avgProcessingTime: number;     // Time being processed
  avgTotalTime: number;          // Wait + processing

  // Success metrics
  completedRequests: number;
  timedOutRequests: number;
  cancelledRequests: number;
  droppedRequests: number;

  // Deadline metrics
  deadlineMissRate: number;      // 0-1
  avgDeadlineSlack: number;      // Avg time before deadline when completed

  // Per-priority breakdown
  perPriority: Map<InferencePriority, {
    avgLatency: number;
    successRate: number;
    throughput: number;
  }>;

  // Resource utilization
  hotPoolUsage: number;          // 0-1
  warmPoolUsage: number;
  coldStartCount: number;

  // Fairness metrics
  starvationCount: number;       // Low-priority requests starved
  maxStarvationTime: number;     // Longest wait for any request
}
```

## CLI Commands for Scheduler Monitoring

```bash
# View persona's request queue
./jtag persona/queue --personaId=<id>

# Output:
┌────────────────────────────────────────────────────────────────────┐
│ Persona Request Queue: CodeExpert                                  │
│ Algorithm: Adaptive    Queue Depth: 12    Avg Wait: 340ms          │
└────────────────────────────────────────────────────────────────────┘

┌────┬──────────┬──────────────────────┬─────────┬─────────┬─────────┐
│ #  │ Priority │ Request              │ Wait    │ Deadline│ Status  │
├────┼──────────┼──────────────────────┼─────────┼─────────┼─────────┤
│ 1  │ CRITICAL │ Fix bug in auth.ts   │ 120ms   │ 880ms   │ Active  │
│ 2  │ HIGH     │ Review PR #1234      │ 450ms   │ 2.5s    │ Queued  │
│ 3  │ HIGH     │ Code completion      │ 890ms   │ 2.1s    │ Queued  │
│ 4  │ NORMAL   │ Analyze codebase     │ 3.2s    │ 8.8s    │ Queued  │
│ 5  │ NORMAL   │ Generate docs        │ 5.1s    │ 14.9s   │ Queued  │
│ 6  │ LOW      │ Refactor suggestions │ 12.3s   │ None    │ Queued  │
│ 7  │ LOW      │ Pre-compute patterns │ 18.7s   │ None    │ Queued  │
└────┴──────────┴──────────────────────┴─────────┴─────────┴─────────┘

⚠️  WARNINGS:
• Request #4 approaching deadline (8.8s remaining)
• Low-priority requests experiencing starvation (max wait: 18.7s)

💡 RECOMMENDATIONS:
• Consider switching to fair scheduling to reduce starvation
• Queue depth rising - request hot pool promotion
```

## Integration with Process Pool

### Resource Allocation Based on Priority
```typescript
class ProcessPool {
  async acquire(
    genomeId: UUID,
    priority: InferencePriority
  ): Promise<InferenceProcess> {

    // Priority determines resource allocation strategy
    switch (priority) {
      case InferencePriority.CRITICAL:
        // Always use HOT pool, spawn new if needed
        return await this.acquireHot(genomeId, { allowSpawn: true });

      case InferencePriority.HIGH:
        // Try HOT, fall back to WARM, spawn if deadline tight
        return await this.acquireHotOrWarm(genomeId, { spawnThreshold: 1000 });

      case InferencePriority.NORMAL:
        // Standard allocation: HOT → WARM → COLD
        return await this.acquireStandard(genomeId);

      case InferencePriority.LOW:
      case InferencePriority.IDLE:
        // Only use idle resources, never spawn
        return await this.acquireIdle(genomeId, { waitForIdle: true });
    }
  }
}
```

## Phase 3: RTOS Scheduler Implementation

### Implementation Order
1. **Priority Queue + Basic Scheduling** (Phase 3.1)
   - Priority-based queue
   - Simple FIFO within priority
   - Queue depth monitoring

2. **Deadline Awareness** (Phase 3.2)
   - Deadline enforcement
   - EDF scheduling
   - Timeout handling

3. **Adaptive Scheduling** (Phase 3.3)
   - Performance-based algorithm selection
   - Resource-aware scheduling
   - Self-optimization

4. **Persona Self-Management** (Phase 3.4)
   - Personas monitor own stats
   - Auto-request resource upgrades
   - Self-tuning parameters

## Success Criteria

- ✅ 95% of CRITICAL requests complete within deadline
- ✅ Zero starvation for any priority level
- ✅ Fair resource allocation across personas
- ✅ Personas can optimize their own scheduling
- ✅ Sub-100ms overhead for queue operations

This transforms personas from passive AI consumers into **active resource managers** - they schedule, optimize, and manage themselves like an operating system.
