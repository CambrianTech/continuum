# CBAR RTOS Architecture Analysis

## What Makes cbar Fast and Efficient

### 1. **Base Class Does All The Work**

**cbar pattern:**
```cpp
// Base class: QueueThread<T>
template <class T> class QueueThread : public CBThread {
    virtual void run() {
        while (!m_isAborted) {
            m_frameMutex.lock();
            m_frameCondition.timedWait(m_frameMutex, 10 + 100 * int(1 + m_priority));

            if (m_frameQueue.size() && !m_isAborted) {
                T item = m_frameQueue.front();
                m_frameQueue.pop();
                m_frameMutex.unlock();

                handleItem(item);  // ← Only this is overridden
            }
        }
    }

    virtual bool handleItem(T item) = 0;  // Pure virtual
};
```

**Implementation is TINY:**
```cpp
class CBP_PlaneAnalyzer : public CBP_AnalyzerThread {
    virtual bool handleFrame(CBAR_VideoFramePtr frame) {
        // Just process the frame, base handles threading
        return processPlaneDetection(frame);
    }
};
```

**Key insight:** Base class handles:
- Thread lifecycle
- Queue management
- Mutex/condition variable logic
- Priority-based wakeup timing
- Abort/flush logic

**Implementations only define**: `handleItem()`

---

### 2. **Constructor Passes Entire Parent Object**

**cbar pattern:**
```cpp
struct CBP_PlaneAnalyzer::Impl {
    Impl(CBP_PlaneAnalyzer *parent) : m_parent(parent) {}

    CBP_PlaneAnalyzer *m_parent;  // ← Can access anything

    bool handleFrame(cbar::CBAR_VideoFramePtr frame) {
        // Access renderer through parent
        auto renderer = CBP_RenderingEngine::sharedInstance();
        auto tracker = renderer->getAnalyzerOfType<CBP_FeatureTracker>();

        // Access parent's methods
        auto anchors = m_parent->getAnchors();
    }
};
```

**Why this is fast:**
- No event emission overhead
- Direct property access
- No indirection layers
- Parent already has what you need

**Compare to our current approach:**
```typescript
// ❌ SLOW: Pass individual properties
constructor(personaId: UUID, inbox: PersonaInbox, memory: WorkingMemory) {
  // Now must pass 20 properties individually
}

// ✅ FAST: Pass entire persona
constructor(persona: PersonaUser) {
  this.persona = persona;
  // Access everything: persona.inbox, persona.memory, persona.state
}
```

---

### 3. **Pipeline Composition, Not Layering**

**cbar pattern:**
```cpp
// CBP_Analyzer is a CONTAINER, not a layer
class CBP_Analyzer : public CBAR_VideoThread {
    std::vector<std::shared_ptr<CBP_AnalyzerThread>> m_analyzers;

    void analyzeFrame(CBAR_VideoFramePtr frame) {
        // Dispatch to all analyzers in parallel
        for (auto &analyzer : m_analyzers) {
            analyzer->addItem(frame);  // Non-blocking queue push
        }
    }

    // Get any analyzer by type
    template<class T> std::shared_ptr<T> ofType() {
        for (const auto &analyzer : m_analyzers) {
            if (auto casted = std::dynamic_pointer_cast<T>(analyzer)) {
                return casted;
            }
        }
        return nullptr;
    }
};
```

**Usage:**
```cpp
// Analyzers access each other directly
auto tracker = renderer->getAnalyzerOfType<CBP_FeatureTracker>();
auto planeAnalyzer = renderer->getAnalyzerOfType<CBP_PlaneAnalyzer>();
```

**Why this is fast:**
- Each analyzer runs in its own thread
- No blocking between analyzers
- Direct access to other analyzers
- No middleware, no indirection

---

### 4. **Priority-Based Adaptive Timing**

**cbar pattern:**
```cpp
// Wait time adapts to priority
m_frameCondition.timedWait(m_frameMutex, 10 + 100 * int(1 + m_priority));

// Priority levels
enum CBThreadPriority {
    CBThreadPriorityHighest = 0,   // 10ms wait
    CBThreadPriorityHigh,           // 110ms wait
    CBThreadPriorityModerate,       // 210ms wait
    CBThreadPriorityDefault,        // 310ms wait
    CBThreadPriorityLow,            // 410ms wait
    CBThreadPriorityLowest,         // 510ms wait
};
```

**Adaptive wakeup:**
```cpp
if (m_priority == CBThreadPriorityHigh || m_wakeupTriggered || !m_hasRun) {
    m_wakeupTriggered = false;
    m_frameCondition.signal();  // Wake immediately
}
```

**Why this is fast:**
- High-priority threads check every 10ms
- Low-priority threads check every 510ms
- Manual wakeup for urgent events
- No unnecessary spinning

---

### 5. **Pimpl Idiom (Private Implementation)**

**cbar pattern:**
```cpp
// Header (public API)
class CBP_PlaneAnalyzer : public CBP_AnalyzerThread {
public:
    virtual bool handleFrame(cbar::CBAR_VideoFramePtr frame);

private:
    struct Impl;  // Forward declaration
    std::unique_ptr<Impl> m_pImpl;  // Opaque pointer
};

// Implementation file
struct CBP_PlaneAnalyzer::Impl {
    Impl(CBP_PlaneAnalyzer *parent) : m_parent(parent) {}

    CBP_PlaneAnalyzer *m_parent;

    // All state private to implementation
    Eigen::Vector3f m_groundCenter;
    CBMutex m_anchorsMutex;
    bool m_hasGround = false;
};
```

**Why this is fast:**
- Reduces compilation dependencies
- Hides implementation details
- Allows aggressive optimization
- Parent pointer gives full access back

---

## What's Slow in Our Current Approach

### 1. **Event Emission Overhead**

**Current (slow):**
```typescript
// Every interaction goes through event system
Events.emit('memory:consolidated', { count: 10 });
Events.subscribe('memory:consolidated', handler);
```

**cbar (fast):**
```cpp
// Direct property access
auto anchors = m_parent->getAnchors();
```

### 2. **Individual Property Passing**

**Current (slow):**
```typescript
constructor(personaId: UUID, inbox: PersonaInbox, memory: WorkingMemory, ...) {
  // Pass 10 properties individually
}
```

**cbar (fast):**
```cpp
Impl(CBP_PlaneAnalyzer *parent) : m_parent(parent) {
  // Access everything through parent
}
```

### 3. **Layered Architecture**

**Current (slow):**
```typescript
PersonaUser
  → PersonaAutonomousLoop
    → PersonaMessageEvaluator
      → WorkingMemoryManager
        → InMemoryCognitionStorage
```

**cbar (fast):**
```cpp
CBP_Analyzer (container)
  ├─ CBP_PlaneAnalyzer (thread 1, direct access to all)
  ├─ CBP_FeatureTracker (thread 2, direct access to all)
  └─ CBP_FloorSegmenter (thread 3, direct access to all)
```

### 4. **No Shared Base Class**

**Current (slow):**
```typescript
// Every subprocess reinvents threading
class MemoryConsolidationWorker {
  private async serviceLoop(): Promise<void> {
    while (this.running) {
      // Manually implement loop logic
      await this.sleep(100);
    }
  }
}
```

**cbar (fast):**
```cpp
// Base class does all the work
class MemoryConsolidationWorker : public PersonaSubprocess<ConsolidationTask> {
    // Just implement handleTask()
    virtual bool handleTask(ConsolidationTask task) {
        return consolidate(task);
    }
};
```

---

## How to Fix It: TypeScript RTOS Pattern

### 1. **Create Base Subprocess Class**

```typescript
/**
 * PersonaSubprocess - Base class for all persona subprocesses
 *
 * Handles:
 * - Thread lifecycle (start/stop)
 * - Priority-based timing
 * - Queue management
 * - Parent persona access
 *
 * Implementations only override: handleTask()
 */
export abstract class PersonaSubprocess<T> {
  protected readonly persona: PersonaUser;  // Full access to parent
  protected readonly priority: SubprocessPriority;
  protected running: boolean = false;

  private queue: T[] = [];
  private wakeupSignal: boolean = false;

  constructor(persona: PersonaUser, priority: SubprocessPriority = 'default') {
    this.persona = persona;
    this.priority = priority;
  }

  async start(): Promise<void> {
    if (this.running) return;

    this.running = true;
    setImmediate(() => this.serviceLoop());
  }

  async stop(): Promise<void> {
    this.running = false;
  }

  // Add item to queue (non-blocking)
  enqueue(task: T): void {
    this.queue.push(task);

    if (this.priority === 'high' || this.wakeupSignal) {
      this.wakeupSignal = false;
      // Immediate processing for high priority
    }
  }

  // Manual wakeup
  wakeup(): void {
    this.wakeupSignal = true;
  }

  // Base class handles loop
  private async serviceLoop(): Promise<void> {
    while (this.running) {
      try {
        if (this.queue.length > 0) {
          const task = this.queue.shift()!;
          await this.handleTask(task);  // ← Only this is overridden
        }

        // Priority-based wait time
        const waitTime = this.getWaitTime();
        await this.sleep(waitTime);
      } catch (error) {
        console.error(`[${this.constructor.name}] Error:`, error);
      }
    }
  }

  // Implementations ONLY override this
  protected abstract handleTask(task: T): Promise<boolean>;

  // Priority-based timing
  private getWaitTime(): number {
    switch (this.priority) {
      case 'highest': return 10;
      case 'high': return 50;
      case 'moderate': return 100;
      case 'default': return 200;
      case 'low': return 500;
      case 'lowest': return 1000;
    }
  }

  private async sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}

export type SubprocessPriority = 'highest' | 'high' | 'moderate' | 'default' | 'low' | 'lowest';
```

### 2. **Refactor MemoryConsolidationWorker**

```typescript
interface ConsolidationTask {
  type: 'check-triggers' | 'consolidate' | 'activate';
  data?: any;
}

export class MemoryConsolidationWorker extends PersonaSubprocess<ConsolidationTask> {
  constructor(persona: PersonaUser) {
    super(persona, 'low');  // Low priority background process
  }

  // Implementation is TINY - just handle the task
  protected async handleTask(task: ConsolidationTask): Promise<boolean> {
    switch (task.type) {
      case 'check-triggers':
        return await this.checkTriggersAndDecide();

      case 'consolidate':
        return await this.consolidate(task.data);

      case 'activate':
        return await this.activate(task.data);
    }
  }

  private async checkTriggersAndDecide(): Promise<boolean> {
    // Peek at persona's inbox directly
    const inboxItems = await this.persona.inbox.peek(10);

    // Access persona's working memory directly
    const workingMemory = this.persona.workingMemory;
    const thoughts = await workingMemory.recall({ limit: 20 });

    // Detect patterns
    const patterns = await this.detectPatterns(inboxItems, thoughts);

    // Enqueue follow-up tasks (non-blocking)
    if (patterns.shouldConsolidate) {
      this.enqueue({ type: 'consolidate', data: patterns.reason });
    }

    if (patterns.shouldActivate) {
      this.enqueue({ type: 'activate', data: patterns.context });
    }

    return true;
  }

  private async consolidate(reason: string): Promise<boolean> {
    // Access persona's memory directly
    const candidates = await this.persona.workingMemory.recall({
      minImportance: 0.6,
      limit: 50
    });

    // Store in long-term
    await this.persona.longTermMemory.appendBatch(candidates);

    // Clear from working memory
    await this.persona.workingMemory.clearBatch(candidates.map(c => c.id));

    return true;
  }
}
```

### 3. **Persona as Container (Like CBP_Analyzer)**

```typescript
export class PersonaUser extends AIUser {
  // Subprocesses (like cbar analyzers)
  private memoryWorker: MemoryConsolidationWorker;
  private taskGenerator: SelfTaskGenerationWorker;
  private trainingWorker: ContinuousLearningWorker;

  async initialize(): Promise<void> {
    // Start all subprocesses (parallel, non-blocking)
    this.memoryWorker = new MemoryConsolidationWorker(this);
    this.taskGenerator = new SelfTaskGenerationWorker(this);
    this.trainingWorker = new ContinuousLearningWorker(this);

    await Promise.all([
      this.memoryWorker.start(),
      this.taskGenerator.start(),
      this.trainingWorker.start()
    ]);
  }

  async destroy(): Promise<void> {
    await Promise.all([
      this.memoryWorker.stop(),
      this.taskGenerator.stop(),
      this.trainingWorker.stop()
    ]);
  }

  // Direct access to subprocesses (like cbar's ofType<>())
  getSubprocess<T>(type: new (...args: any[]) => T): T | undefined {
    if (this.memoryWorker instanceof type) return this.memoryWorker as unknown as T;
    if (this.taskGenerator instanceof type) return this.taskGenerator as unknown as T;
    if (this.trainingWorker instanceof type) return this.trainingWorker as unknown as T;
    return undefined;
  }
}
```

---

## Speed Improvements

### Before (Current):
- ❌ Event emission overhead
- ❌ Individual property passing
- ❌ Layered indirection
- ❌ Manual loop management
- ❌ No priority system

### After (RTOS Pattern):
- ✅ Direct property access via `this.persona`
- ✅ Base class handles all threading
- ✅ Parallel subprocesses, no blocking
- ✅ Priority-based adaptive timing
- ✅ Implementations are ~50 lines, not 578

---

## Implementation Path

**Phase 1**: Create `PersonaSubprocess<T>` base class
**Phase 2**: Refactor `MemoryConsolidationWorker` to extend it
**Phase 3**: Add `SelfTaskGenerationWorker` using same pattern
**Phase 4**: Add `ContinuousLearningWorker` using same pattern
**Phase 5**: Persona becomes container, not orchestrator

**Result**: Fast, efficient, RTOS-style architecture where each subprocess enhances the whole without blocking.
