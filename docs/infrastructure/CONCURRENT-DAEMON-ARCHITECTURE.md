# Concurrent Daemon Architecture

## Goal

Move all potentially intensive or blocking daemon operations **off the main thread** to ensure the event loop remains responsive during system startup and operation.

## Current State

```
┌─────────────────────────────────────────────────────────────────┐
│ MAIN THREAD (Node.js Event Loop)                                │
├─────────────────────────────────────────────────────────────────┤
│ JTAGSystem.setupDaemons()                                        │
│   └── Promise.all(daemonEntries.map(async entry => {            │
│         const daemon = createDaemon(entry);                      │
│         await daemon.initializeDaemon();  ← BLOCKS EVENT LOOP   │
│       }));                                                       │
│                                                                  │
│ BLOCKING OPERATIONS DURING INIT:                                 │
│ • DataDaemon: SQLite open, entity scanning, schema validation   │
│ • AIProvider: SecretManager load, adapter registration          │
│ • Logger/Console: Rust worker connection with retry             │
│ • UserDaemon: PersonaUser creation (SQLite per persona)         │
│ • Archive: Database handle registration                          │
└─────────────────────────────────────────────────────────────────┘
```

**Problems:**
1. Heavy `initialize()` methods block the main thread
2. I/O-bound operations (SQLite, file reads) starve the event loop
3. CPU-bound work (entity scanning, JSON parsing) causes jank
4. External service connections (Ollama, APIs) can timeout others

## Target Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│ MAIN THREAD (Coordinator Only)                                  │
├─────────────────────────────────────────────────────────────────┤
│ • Message routing (JTAGRouter)                                   │
│ • Event dispatch (EventManager)                                  │
│ • Lightweight state management                                   │
│ • Spawn workers, await completion signals                        │
└─────────────────────────────────────────────────────────────────┘
          │                    │                    │
          ▼                    ▼                    ▼
┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐
│ WORKER THREAD 1 │  │ WORKER THREAD 2 │  │ RUST WORKERS    │
│ (Node worker)   │  │ (Node worker)   │  │ (Unix socket)   │
├─────────────────┤  ├─────────────────┤  ├─────────────────┤
│ DataDaemon init │  │ AIProvider init │  │ • logger        │
│ • SQLite open   │  │ • Secret load   │  │ • data-daemon   │
│ • Entity scan   │  │ • Adapter reg   │  │ • archive       │
│ • Schema valid  │  │ • Health check  │  │ • embedding     │
└─────────────────┘  └─────────────────┘  │ • search        │
                                          │ • inference     │
                                          └─────────────────┘
```

## Implementation Phases

### Phase 1: Deferred Non-Critical Initialization

**Pattern**: Initialize core functionality immediately, defer heavy work.

```typescript
// DaemonBase enhancement
abstract class DaemonBase {
  // Quick init - register with router, basic setup
  async initializeDaemon(): Promise<void> {
    this._lifecycleState = DaemonLifecycleState.STARTING;
    this.router.registerSubscriber(this.subpath, this);

    // FAST: Core setup only
    await this.initializeCore();

    this._lifecycleState = DaemonLifecycleState.READY;
    await this.flushStartupQueue();

    // DEFERRED: Heavy work in background
    this.initializeDeferred();
  }

  // Override for minimal startup (REQUIRED)
  protected abstract initializeCore(): Promise<void>;

  // Override for heavy work (OPTIONAL, runs after READY)
  protected initializeDeferred(): void {
    // Default: no-op
  }
}
```

**Apply to:**
- `AIProviderDaemon`: Core = register priority list, Deferred = adapter health checks
- `UserDaemon`: Core = event subscriptions, Deferred = ensure persona clients
- `TrainingDaemon`: Core = event subscriptions, Deferred = load training rooms
- `RoomMembershipDaemon`: Core = event subscriptions, Deferred = ensure all users in rooms

### Phase 2: Worker Thread Pool for TypeScript Operations

**Pattern**: CPU-intensive work runs in Node.js worker threads.

```typescript
// New: system/core/workers/DaemonWorkerPool.ts
import { Worker } from 'worker_threads';

interface WorkerTask<T> {
  type: string;
  payload: any;
  resolve: (value: T) => void;
  reject: (error: Error) => void;
}

class DaemonWorkerPool {
  private workers: Worker[] = [];
  private taskQueue: WorkerTask<any>[] = [];
  private readonly poolSize: number;

  constructor(poolSize: number = 4) {
    this.poolSize = poolSize;
  }

  async initialize(): Promise<void> {
    for (let i = 0; i < this.poolSize; i++) {
      const worker = new Worker('./daemon-worker.js');
      worker.on('message', this.handleWorkerMessage.bind(this, i));
      worker.on('error', this.handleWorkerError.bind(this, i));
      this.workers.push(worker);
    }
  }

  // Execute task in worker thread
  async execute<T>(type: string, payload: any): Promise<T> {
    return new Promise((resolve, reject) => {
      this.taskQueue.push({ type, payload, resolve, reject });
      this.dispatchNext();
    });
  }

  // Tasks that can run in workers:
  // - Entity scanning (file system + JSON parsing)
  // - Config file loading
  // - Secret decryption
  // - Schema validation
}

// daemon-worker.js
const { parentPort } = require('worker_threads');

parentPort.on('message', async ({ type, payload, taskId }) => {
  try {
    let result;
    switch (type) {
      case 'scan-entities':
        result = await scanEntities(payload.path);
        break;
      case 'load-config':
        result = await loadConfig(payload.path);
        break;
      case 'validate-schema':
        result = validateSchema(payload.entities);
        break;
    }
    parentPort.postMessage({ taskId, result });
  } catch (error) {
    parentPort.postMessage({ taskId, error: error.message });
  }
});
```

**Apply to:**
- `DataDaemon`: Entity scanning, schema validation
- `AIProviderDaemon`: Config parsing, secret loading
- `SessionDaemon`: Session file loading

### Phase 3: Rust Worker Integration (Heavy I/O)

**Pattern**: Move all heavy I/O to Rust workers via Unix socket or gRPC.

**Already implemented:**
- `logger` - File I/O batching
- `data-daemon` - SQLite operations, WAL cleanup
- `archive` - Cold storage management
- `embedding` - ONNX inference
- `search` - BoW/BM25 algorithms
- `inference-grpc` - LLM inference

**To implement:**
```rust
// workers/daemon-init/src/main.rs
// Consolidate daemon initialization into Rust

use tokio::sync::mpsc;

#[derive(Deserialize)]
enum InitTask {
    OpenDatabase { path: String },
    ScanEntities { glob: String },
    LoadSecrets { config_path: String },
    ValidateSchema { entities: Vec<EntityDef> },
}

#[derive(Serialize)]
enum InitResult {
    DatabaseOpened { handle_id: u64 },
    EntitiesScanned { entities: Vec<EntityDef> },
    SecretsLoaded { keys: Vec<String> },
    SchemaValid { errors: Vec<String> },
}

async fn handle_init_task(task: InitTask) -> InitResult {
    match task {
        InitTask::OpenDatabase { path } => {
            let db = rusqlite::Connection::open(&path)?;
            // Return handle to TypeScript
        }
        // ...
    }
}
```

### Phase 4: Dependency-Aware Parallel Startup

**Pattern**: Start daemons in waves based on dependency graph.

```typescript
// system/core/system/shared/DaemonOrchestrator.ts

interface DaemonDependency {
  daemon: string;
  waitFor: string[];  // Daemons that must be READY first
  phase: 'critical' | 'integration' | 'lightweight';
}

const DAEMON_GRAPH: DaemonDependency[] = [
  // Phase 1: Critical path (no dependencies, start immediately)
  { daemon: 'data', waitFor: [], phase: 'critical' },
  { daemon: 'command', waitFor: [], phase: 'critical' },
  { daemon: 'events', waitFor: [], phase: 'critical' },

  // Phase 2: Integration (wait for data)
  { daemon: 'user', waitFor: ['data'], phase: 'integration' },
  { daemon: 'room-membership', waitFor: ['data'], phase: 'integration' },
  { daemon: 'training', waitFor: ['data'], phase: 'integration' },
  { daemon: 'archive', waitFor: ['data'], phase: 'integration' },
  { daemon: 'ai-provider', waitFor: [], phase: 'integration' },  // No data dep

  // Phase 3: Lightweight (no dependencies, can start anytime)
  { daemon: 'health', waitFor: [], phase: 'lightweight' },
  { daemon: 'widget', waitFor: [], phase: 'lightweight' },
  { daemon: 'proxy', waitFor: [], phase: 'lightweight' },
  { daemon: 'governance', waitFor: [], phase: 'lightweight' },
  { daemon: 'logger', waitFor: [], phase: 'lightweight' },
  { daemon: 'console', waitFor: ['logger'], phase: 'lightweight' },
];

class DaemonOrchestrator {
  async startAll(): Promise<void> {
    const readyDaemons = new Set<string>();
    const pendingDaemons = new Map<string, DaemonBase>();

    // Start all daemons that have no dependencies
    const noDeps = DAEMON_GRAPH.filter(d => d.waitFor.length === 0);
    await Promise.all(noDeps.map(d => this.startDaemon(d.daemon)));

    // Process remaining daemons as dependencies resolve
    for await (const readyEvent of this.daemonReadyEvents()) {
      readyDaemons.add(readyEvent.daemon);

      // Find daemons whose dependencies are now satisfied
      const canStart = DAEMON_GRAPH.filter(d =>
        !readyDaemons.has(d.daemon) &&
        d.waitFor.every(dep => readyDaemons.has(dep))
      );

      await Promise.all(canStart.map(d => this.startDaemon(d.daemon)));
    }
  }
}
```

## Metrics & Observability

```typescript
// Track initialization performance
interface DaemonInitMetrics {
  daemon: string;
  phase: 'core' | 'deferred';
  startTime: number;
  endTime: number;
  blockedMainThread: boolean;  // Did this run on main thread?
  workerUsed: 'none' | 'node-worker' | 'rust-worker';
}

// Emit metrics during startup
Events.emit('system:daemon:init-complete', {
  daemon: this.name,
  metrics: {
    coreInitMs: coreEnd - coreStart,
    deferredInitMs: deferredEnd - deferredStart,
    totalMs: Date.now() - constructionTime,
    queuedMessages: this.startupQueueSize,
  }
});
```

## Migration Path

### Step 1: Audit & Classify (This Document)
- [x] Identify heavy daemons
- [x] Map dependencies
- [x] Document blocking operations

### Step 2: Deferred Initialization ✅ COMPLETE
- [x] Add `initialize()` / `initializeDeferred()` to DaemonBase
- [x] Migrate heavy daemons to use deferred pattern
- [x] Verify startup queue handles early messages
- [x] Parallel adapter registration in AIProviderDaemon (3.2x faster)

**Performance Results:**
| Daemon | Before | After | Improvement |
|--------|--------|-------|-------------|
| DataDaemon CORE | N/A | 179ms | Baseline |
| UserDaemon CORE | N/A | 5ms | Very fast |
| AIProvider CORE | 12,000ms | 3,711ms | **3.2x faster** |

### Step 3: Worker Thread Pool
- [ ] Implement DaemonWorkerPool
- [ ] Move entity scanning to worker
- [ ] Move config loading to worker
- [ ] Move secret loading to worker

### Step 4: Rust Worker Consolidation (HIGH PRIORITY)
- [ ] Create `daemon-init` Rust worker
- [ ] Move SQLite open to Rust
- [ ] Move schema validation to Rust
- [ ] Unified Unix socket protocol

### Step 5: Dependency Orchestrator
- [ ] Implement DaemonOrchestrator
- [ ] Add daemon dependency declarations
- [ ] Wave-based parallel startup
- [ ] Metrics dashboard for init times

## Memory Leak Patterns (Critical Findings)

TypeScript/Node.js memory management has proven problematic. These patterns caused 6.88GB+ RAM usage:

### Pattern 1: Untracked Event Subscriptions

**Problem:** Event subscriptions created without storing unsubscribe functions.

```typescript
// ❌ BAD - No way to clean up
Events.subscribe('data:chat_messages:created', this.handleMessage);

// ✅ GOOD - Track for cleanup
private _eventUnsubscribes: (() => void)[] = [];

const unsub = Events.subscribe('data:chat_messages:created', this.handleMessage);
this._eventUnsubscribes.push(unsub);

// In shutdown():
for (const unsub of this._eventUnsubscribes) {
  unsub();
}
this._eventUnsubscribes = [];
```

**Fixed in:** `PersonaUser.ts`, `UserDaemonServer.ts`

### Pattern 2: Unbounded Caches

**Problem:** Caches that grow indefinitely without eviction.

```typescript
// ❌ BAD - Grows forever
private _roomNameCache = new Map<string, string>();

// ✅ GOOD - Clear on shutdown OR use LRU
async shutdown(): Promise<void> {
  this._roomNameCache.clear();
}
```

**Fixed in:** `PersonaUser.ts` (clear on shutdown)

### Pattern 3: Closure Leaks in Async Loops

**Problem:** Closures capturing references that prevent GC.

```typescript
// ❌ BAD - Interval holds reference to entire class
setInterval(() => {
  this.checkHealth();  // 'this' keeps entire object alive
}, 30000);

// ✅ GOOD - Use registerInterval for tracked cleanup
this.registerInterval('health-check', () => this.checkHealth(), 30000);
```

**Pattern in:** `ServerDaemonBase.ts` `registerInterval()` method

### Pattern 4: ResponseCorrelator Timeout Leaks

**Problem:** Pending requests not cleaned up on disconnect.

```typescript
// In ResponseCorrelator.ts - already implemented:
rejectAll(reason: string): void {
  for (const [id, request] of this.pendingRequests.entries()) {
    clearTimeout(request.timeout);
    request.reject(new Error(reason));
  }
  this.pendingRequests.clear();
}
```

**Status:** Already handled in `ResponseCorrelator.ts`

---

## Rust Worker Priority (Essential for Memory Safety)

**Rationale:** Node.js GC is unpredictable and TypeScript provides no memory safety guarantees. Rust workers eliminate entire categories of memory bugs.

### Priority 1: PersonaUser State Management (CRITICAL)

Move PersonaUser's long-running state to Rust:
- Inbox queue management
- Energy/mood state tracking
- Room subscription tracking
- Message rate limiting

**Why:** PersonaUsers run for hours/days. Memory accumulation here is catastrophic.

```rust
// workers/persona-state/src/main.rs
struct PersonaState {
    user_id: Uuid,
    inbox: VecDeque<InboxItem>,      // Bounded queue
    energy: f32,
    mood: MoodState,
    room_subscriptions: HashSet<Uuid>, // Clear ownership
}

impl PersonaState {
    fn tick(&mut self) -> Vec<Action> {
        // All state management in Rust = no GC leaks
    }
}
```

### Priority 2: Event Bus Subscriptions (HIGH)

Move event subscription tracking to Rust:
- Automatic cleanup when subscriber disconnects
- Reference counting for subscription lifetime
- Zero-copy event dispatch

```rust
struct EventBus {
    subscriptions: HashMap<EventType, Vec<WeakRef<Subscriber>>>,
    // WeakRef = automatic cleanup when subscriber dropped
}
```

### Priority 3: Cache Management (MEDIUM)

Rust-managed LRU caches with strict bounds:
- Room name cache
- User entity cache
- Message deduplication cache

```rust
use lru::LruCache;

struct CacheManager {
    room_names: LruCache<Uuid, String>,  // Fixed size, auto-eviction
    users: LruCache<Uuid, UserEntity>,
}
```

### Priority 4: Long-Running Timers (MEDIUM)

Move health monitoring and polling to Rust:
- No closure leaks possible
- Deterministic resource cleanup
- Tokio runtime for efficient async

---

## Success Criteria

### Performance
1. **Main thread never blocked >50ms** during daemon initialization
2. **Time to first command** <500ms (basic routing works immediately)
3. **Full system ready** <3s (all daemons READY)
4. **No lost messages** during startup (startup queue handles all)
5. **Graceful degradation** if worker unavailable (fallback to main thread)

### Memory Safety
6. **Node.js process <500MB** after 1 hour of operation
7. **No memory growth** during idle periods (flat line on heap graph)
8. **All event subscriptions tracked** with cleanup on shutdown
9. **All caches bounded** with explicit size limits or LRU eviction
10. **Rust workers for all long-running state** (PersonaUser, EventBus)

## Related Documents

- `daemons/command-daemon/shared/DaemonBase.ts` - Lifecycle states and startup queue
- `daemons/command-daemon/server/ServerDaemonBase.ts` - Concurrency primitives (RateLimiter, Semaphore)
- `system/user/server/PersonaUser.ts` - Event subscription cleanup pattern
- `system/core/shared/ResponseCorrelator.ts` - Request timeout management
- `workers/` - Existing Rust worker implementations
- `ARCHITECTURE-RULES.md` - Type system and environment rules
