# Daemon Concurrency Pattern v1.0

**Status**: CANONICAL - All generated daemons MUST follow this pattern  
**Created**: 2024-12-07  
**Pattern Version**: 1  

## Philosophy

**"Make the right thing easy, and the wrong thing hard."**

Every daemon in this system handles concurrent requests. Rather than leave concurrency as an implementation detail that developers get wrong, we bake proven patterns into the generator itself. A well-designed daemon should be:

1. **Safe by default** - Race conditions impossible
2. **Observable** - Internal state visible for debugging
3. **Resilient** - Graceful degradation under load
4. **Testable** - Concurrency behavior validated automatically

---

## The Six Layers of Daemon Concurrency

Every generated daemon implements these six layers in order:

```typescript
class ExampleDaemon extends DaemonBase {
  // Layer 0: Worker Thread Pool (keep main thread responsive)
  private workerPool: WorkerPool;

  // Layer 1: Rate Limiting (reject overload fast)
  private rateLimiter: RateLimiter;

  // Layer 2: Request Queue (serialize critical sections)
  private requestQueue: AsyncQueue<Request>;

  // Layer 3: Concurrency Control (limit parallel work)
  private semaphore: Semaphore;

  // Layer 4: Metrics (observability)
  private metrics: DaemonMetrics;

  // Layer 5: Graceful Shutdown (cleanup)
  private shutdownSignal: AbortController;
}
```

---

## Layer 0: Worker Thread Pool (THE FOUNDATION)

**Purpose**: Keep main thread responsive - ALL daemon work happens in worker threads

**Philosophy**: "You write simple code, we handle the threading."

### The Problem With Node.js

Node.js is single-threaded by default. Every daemon that does work blocks the event loop. Worker threads exist, but the API is painful - manual `postMessage`, serialization, no shared memory benefits.

**This is why you almost required Rust for daemons** - true threading with zero ceremony.

### The Solution: Auto-Generated Worker Pool

```typescript
// Generated automatically - developer never sees this
class CacheDaemonWorkerPool extends WorkerPool {
  constructor() {
    super({
      workerScript: './CacheDaemonWorker.js',
      poolSize: 4, // 4 workers by default
      strategy: 'round-robin' // or 'least-busy'
    });
  }
}

// Worker script (also generated)
// CacheDaemonWorker.js
import { parentPort } from 'worker_threads';
import { CacheDaemonLogic } from './CacheDaemonLogic';

const logic = new CacheDaemonLogic();

parentPort.on('message', async (request) => {
  const result = await logic.process(request);
  parentPort.postMessage({ id: request.id, result });
});
```

### What The Developer Writes (Simple!)

```typescript
// Developer just writes this - NO worker thread code!
export class CacheDaemonLogic {
  private cache = new Map<string, unknown>();

  async set(key: string, value: unknown): Promise<void> {
    // Just write normal code - it runs in a worker automatically
    this.cache.set(key, value);
  }

  async get(key: string): Promise<unknown> {
    return this.cache.get(key);
  }
}
```

### What The Generator Creates (Plumbing)

```typescript
// Main thread facade (generated)
export abstract class CacheDaemon extends DaemonBase {
  private workerPool: CacheDaemonWorkerPool;

  constructor(context: JTAGContext, router: JTAGRouter) {
    super('cache-daemon', context, router);

    // Generator creates worker pool automatically
    this.workerPool = new CacheDaemonWorkerPool();
  }

  async handleMessage(message: JTAGMessage): Promise<Response> {
    // Main thread: just route to worker (non-blocking!)
    return await this.workerPool.execute({
      method: message.payload.type,
      params: message.payload
    });
  }
}
```

### Benefits

✅ **Developer writes simple code** - No `postMessage`, no serialization hell
✅ **Main thread never blocks** - All work in worker pool
✅ **Auto-scaling** - Worker pool size configurable (default: CPU count)
✅ **Type-safe** - Full TypeScript types across thread boundary
✅ **Debuggable** - Workers show in debugger, can inspect state

### Advanced: Rust Workers For Performance-Critical Daemons

For CPU-heavy daemons (embedding generation, vector search, image processing):

```typescript
// Spec flag tells generator to use Rust
{
  "name": "embedding-daemon",
  "runtime": "rust", // <- Generator creates Rust worker + Node.js facade
  "jobs": [...]
}
```

**Generated structure**:
```
daemons/embedding-daemon/
  ├── shared/EmbeddingDaemon.ts       (Node.js facade)
  ├── worker/
  │   ├── Cargo.toml                  (Rust project)
  │   ├── src/lib.rs                  (Rust worker logic)
  │   └── build.rs                    (Compile to .node addon)
  └── test/
```

**Developer writes** (Rust):
```rust
// worker/src/lib.rs
use neon::prelude::*;

fn generate_embedding(mut cx: FunctionContext) -> JsResult<JsArray> {
    let text = cx.argument::<JsString>(0)?.value(&mut cx);

    // Just write normal Rust - true parallelism, zero-copy
    let embedding = model.embed(&text);

    // neon handles JS conversion
    let result = JsArray::new(&mut cx, embedding.len() as u32);
    for (i, val) in embedding.iter().enumerate() {
        let js_val = cx.number(*val);
        result.set(&mut cx, i as u32, js_val)?;
    }
    Ok(result)
}
```

**Generator creates facade** (TypeScript):
```typescript
export class EmbeddingDaemon extends DaemonBase {
  private rustWorker: NativeWorker; // Wraps Rust .node addon

  async generateEmbedding(text: string): Promise<number[]> {
    // Call Rust worker - true parallel execution
    return this.rustWorker.call('generate_embedding', text);
  }
}
```

### Configuration

```json
// DaemonSpec
{
  "name": "cache-daemon",
  "workerPool": {
    "enabled": true,           // default: true
    "poolSize": 4,             // default: os.cpus().length
    "runtime": "typescript",   // or "rust" for performance
    "strategy": "round-robin", // or "least-busy"
    "maxMemoryMB": 512        // per worker
  }
}
```

### Why This Matters

**Without Layer 0**: Developer must manually create workers, handle `postMessage`, deal with serialization, manage pool, handle crashes. Result: most developers just block the main thread.

**With Layer 0**: Developer writes simple business logic in `Logic.ts`, generator creates worker scaffolding. Main thread stays responsive automatically.

**"You're merely writing simple code for the most part, as you would have, but it's not blocking the main thread."**

---

## Layer 1: Rate Limiting

**Purpose**: Reject overload BEFORE work starts (fast failure)

```typescript
async handleMessage(message: JTAGMessage): Promise<Response> {
  // FIRST: Check rate limit (cheap operation)
  if (!await this.rateLimiter.tryAcquire()) {
    this.metrics.recordRateLimitExceeded();
    return {
      success: false,
      error: 'Rate limit exceeded - try again later',
      retryAfter: this.rateLimiter.retryAfter()
    };
  }
  
  // Continue to Layer 2...
}
```

**Configuration**:
- **Default**: 100 requests/second per daemon
- **Customizable** via DaemonSpec: `rateLimit: { requestsPerSecond: 50 }`
- **Algorithm**: Token bucket (allows bursts, smooth average)

**Why Token Bucket?**
- Allows natural bursts (10 requests at once is fine)
- Smooth sustained load (can't sustain 1000 req/sec)
- Fair across time windows

---

## Layer 2: Request Queue

**Purpose**: Serialize critical sections to prevent race conditions

```typescript
async handleMessage(message: JTAGMessage): Promise<Response> {
  // Rate limit passed...
  
  // SECOND: Queue request for ordered processing
  return await this.requestQueue.enqueue(async () => {
    // Inside queue: operations execute one-at-a-time for this queue
    // Multiple queues can run in parallel (e.g., read queue + write queue)
    
    return await this.processRequest(message);
  });
}
```

**When to Use**:
- ✅ **State mutations** (write to database, update cache)
- ✅ **Non-commutative operations** (order matters: withdraw then deposit ≠ deposit then withdraw)
- ❌ **Pure reads** (can be concurrent)
- ❌ **Independent operations** (writes to different keys)

**Advanced: Multiple Queues**:
```typescript
class DataDaemon extends DaemonBase {
  private writeQueue: AsyncQueue;  // Serializes writes
  private readQueue: AsyncQueue;   // Serializes reads (optional)
  
  async handleMessage(msg: JTAGMessage): Promise<Response> {
    if (msg.type === 'write') {
      return this.writeQueue.enqueue(() => this.handleWrite(msg));
    } else {
      // Reads can be concurrent (no queue)
      return this.handleRead(msg);
    }
  }
}
```

---

## Layer 3: Concurrency Control (Semaphore)

**Purpose**: Limit parallel expensive operations (prevent resource exhaustion)

```typescript
async processRequest(message: JTAGMessage): Promise<Response> {
  // Queue guarantees order, now limit concurrency
  
  // THIRD: Acquire semaphore slot (max N concurrent operations)
  return await this.semaphore.execute(async () => {
    // Inside semaphore: at most N of these run concurrently
    // (e.g., max 5 concurrent database queries)
    
    return await this.doExpensiveWork(message);
  });
}
```

**Default Limits**:
- **Database operations**: 10 concurrent
- **File I/O**: 5 concurrent  
- **Network requests**: 20 concurrent
- **CPU-heavy tasks**: 2 concurrent

**Customizable** via DaemonSpec:
```json
{
  "concurrency": {
    "maxConcurrent": 5,
    "queueSize": 100
  }
}
```

**Why Semaphore + Queue?**
- **Queue**: Ensures order (A before B)
- **Semaphore**: Limits load (max 5 at once)
- **Together**: Ordered + bounded = safe and performant

---

## Layer 4: Metrics & Observability

**Purpose**: Make internal state visible for debugging and monitoring

Every daemon exposes:

```typescript
class ExampleDaemon extends DaemonBase {
  getMetrics(): DaemonMetrics {
    return {
      // Rate limiting
      rateLimitRemaining: this.rateLimiter.availableTokens(),
      rateLimitHitsTotal: this.metrics.rateLimitHits,
      
      // Queue state
      queueDepth: this.requestQueue.size,
      queueMaxDepth: this.requestQueue.maxSize,
      queueProcessed: this.metrics.requestsProcessed,
      
      // Concurrency state
      activeTasks: this.semaphore.activeCount,
      maxConcurrency: this.semaphore.limit,
      
      // Performance
      avgProcessingTime: this.metrics.avgDuration,
      p95ProcessingTime: this.metrics.p95Duration,
      
      // Health
      uptime: Date.now() - this.startTime,
      errors: this.metrics.errorCount,
      successRate: this.metrics.successRate
    };
  }
}
```

**Usage**:
```bash
# Check daemon health
./jtag daemon/metrics --daemon="data-daemon"

# Output:
Rate Limit: 87/100 tokens available (13 hits today)
Queue: 3 pending, 1,247 processed (max depth: 15)
Concurrency: 5/10 active
Performance: avg 23ms, p95 45ms
Health: 99.7% success rate, 4 errors, uptime 2h 15m
```

---

## Layer 5: Graceful Shutdown

**Purpose**: Clean shutdown without losing in-flight work

```typescript
class ExampleDaemon extends DaemonBase {
  private shutdownSignal = new AbortController();
  
  async shutdown(): Promise<void> {
    console.log('Shutdown initiated - draining queue...');
    
    // 1. Stop accepting new requests
    this.shutdownSignal.abort();
    
    // 2. Wait for queue to drain (with timeout)
    await this.requestQueue.drain({ timeout: 30000 });
    
    // 3. Wait for active tasks to complete
    await this.semaphore.waitForAll({ timeout: 10000 });
    
    // 4. Cleanup resources
    await this.cleanup();
    
    console.log('Shutdown complete');
  }
  
  async handleMessage(msg: JTAGMessage): Promise<Response> {
    // Check shutdown signal before processing
    if (this.shutdownSignal.signal.aborted) {
      return { success: false, error: 'Daemon is shutting down' };
    }
    
    // Normal processing...
  }
}
```

---

## Complete Pattern Example

```typescript
import { DaemonBase } from '../../../system/daemon/shared/DaemonBase';
import { RateLimiter } from '../../../system/concurrency/RateLimiter';
import { AsyncQueue } from '../../../system/concurrency/AsyncQueue';
import { Semaphore } from '../../../system/concurrency/Semaphore';
import { DaemonMetrics } from '../../../system/daemon/shared/DaemonMetrics';

export abstract class CacheDaemon extends DaemonBase {
  // Pattern Version (for audit tool)
  static readonly PATTERN_VERSION = 1;
  
  // Concurrency primitives
  private rateLimiter: RateLimiter;
  private requestQueue: AsyncQueue<CacheRequest>;
  private semaphore: Semaphore;
  private metrics: DaemonMetrics;
  private shutdownSignal: AbortController;
  
  constructor(context: JTAGContext, router: JTAGRouter) {
    super('cache-daemon', context, router);
    
    // Initialize concurrency controls
    this.rateLimiter = new RateLimiter({ requestsPerSecond: 100 });
    this.requestQueue = new AsyncQueue({ maxSize: 1000 });
    this.semaphore = new Semaphore({ limit: 10 });
    this.metrics = new DaemonMetrics();
    this.shutdownSignal = new AbortController();
  }
  
  async handleMessage(message: JTAGMessage): Promise<CacheResponse> {
    const startTime = Date.now();
    
    try {
      // Layer 1: Rate limiting
      if (!await this.rateLimiter.tryAcquire()) {
        this.metrics.recordRateLimitExceeded();
        return {
          success: false,
          error: 'Rate limit exceeded',
          retryAfter: this.rateLimiter.retryAfter()
        };
      }
      
      // Check shutdown
      if (this.shutdownSignal.signal.aborted) {
        return { success: false, error: 'Daemon shutting down' };
      }
      
      // Layer 2: Queue for ordering
      return await this.requestQueue.enqueue(async () => {
        // Layer 3: Semaphore for concurrency control
        return await this.semaphore.execute(async () => {
          // Actual work
          const result = await this.processCacheRequest(message);
          
          // Layer 4: Record metrics
          this.metrics.recordSuccess(Date.now() - startTime);
          
          return result;
        });
      });
      
    } catch (error: unknown) {
      const err = error as Error;
      this.metrics.recordError(err);
      return { success: false, error: err.message };
    }
  }
  
  // Layer 4: Metrics endpoint
  getMetrics(): DaemonMetrics {
    return {
      rateLimitRemaining: this.rateLimiter.availableTokens(),
      rateLimitHits: this.metrics.rateLimitHits,
      queueDepth: this.requestQueue.size,
      activeTasks: this.semaphore.activeCount,
      requestsProcessed: this.metrics.totalRequests,
      avgDuration: this.metrics.avgDuration,
      errorCount: this.metrics.errorCount,
      successRate: this.metrics.successRate,
      uptime: Date.now() - this.startTime
    };
  }
  
  // Layer 5: Graceful shutdown
  async shutdown(): Promise<void> {
    this.shutdownSignal.abort();
    await this.requestQueue.drain({ timeout: 30000 });
    await this.semaphore.waitForAll({ timeout: 10000 });
    await this.cleanup();
  }
  
  protected abstract processCacheRequest(message: JTAGMessage): Promise<CacheResponse>;
  protected abstract cleanup(): Promise<void>;
}
```

---

## Generated Tests

Every daemon gets these tests automatically:

### Unit Tests (Layer Logic)
```typescript
describe('CacheDaemon - Unit', () => {
  test('rate limiter rejects after limit', async () => {
    const daemon = new CacheDaemon();
    
    // Send 101 requests (limit is 100)
    const promises = Array.from({length: 101}, () => 
      daemon.handleMessage({type: 'get', key: 'test'})
    );
    
    const results = await Promise.all(promises);
    const rateLimited = results.filter(r => !r.success && r.error.includes('Rate limit'));
    
    expect(rateLimited.length).toBeGreaterThan(0);
  });
  
  test('queue serializes operations', async () => {
    const daemon = new CacheDaemon();
    const executionOrder: number[] = [];
    
    // Send 10 requests that record execution order
    const promises = Array.from({length: 10}, (_, i) => 
      daemon.handleMessage({
        type: 'test',
        onExecute: () => executionOrder.push(i)
      })
    );
    
    await Promise.all(promises);
    
    // Verify execution was ordered
    expect(executionOrder).toEqual([0,1,2,3,4,5,6,7,8,9]);
  });
  
  test('semaphore limits concurrency', async () => {
    const daemon = new CacheDaemon();
    let maxConcurrent = 0;
    let currentConcurrent = 0;
    
    const promises = Array.from({length: 50}, () => 
      daemon.handleMessage({
        type: 'slow',
        onStart: () => {
          currentConcurrent++;
          maxConcurrent = Math.max(maxConcurrent, currentConcurrent);
        },
        onEnd: () => currentConcurrent--
      })
    );
    
    await Promise.all(promises);
    
    // Should never exceed semaphore limit
    expect(maxConcurrent).toBeLessThanOrEqual(10);
  });
});
```

### Integration Tests (Full System)
```typescript
describe('CacheDaemon - Integration', () => {
  test('handles 1000 concurrent requests without deadlock', async () => {
    const client = await jtag.connect();
    
    const promises = Array.from({length: 1000}, (_, i) => 
      client.commands['cache-daemon/set']({key: `key${i}`, value: i})
    );
    
    const results = await Promise.all(promises);
    
    // All should succeed (no deadlocks, no crashes)
    expect(results.every(r => r.success)).toBe(true);
  });
  
  test('metrics reflect actual load', async () => {
    const client = await jtag.connect();
    
    // Send 50 requests
    await Promise.all(
      Array.from({length: 50}, () => 
        client.commands['cache-daemon/get']({key: 'test'})
      )
    );
    
    // Check metrics
    const metrics = await client.commands['daemon/metrics']({daemon: 'cache-daemon'});
    
    expect(metrics.requestsProcessed).toBeGreaterThanOrEqual(50);
    expect(metrics.successRate).toBeGreaterThan(0.95); // 95%+ success
  });
  
  test('graceful shutdown drains queue', async () => {
    const client = await jtag.connect();
    
    // Send 100 long-running requests
    const promises = Array.from({length: 100}, () => 
      client.commands['cache-daemon/slow']({duration: 1000})
    );
    
    // Trigger shutdown after 500ms
    setTimeout(() => client.commands['daemon/shutdown']({daemon: 'cache-daemon'}), 500);
    
    const results = await Promise.all(promises);
    
    // All requests that started should complete
    const completed = results.filter(r => r.success);
    expect(completed.length).toBeGreaterThan(0);
  });
});
```

### Load Tests (Stress Testing)
```typescript
describe('CacheDaemon - Load', () => {
  test('sustained load of 1000 req/sec for 10 seconds', async () => {
    const client = await jtag.connect();
    const duration = 10000; // 10 seconds
    const rps = 1000;
    
    const interval = 1000 / rps;
    let requestCount = 0;
    let errorCount = 0;
    
    const startTime = Date.now();
    
    while (Date.now() - startTime < duration) {
      client.commands['cache-daemon/get']({key: 'test'})
        .then(() => requestCount++)
        .catch(() => errorCount++);
      
      await sleep(interval);
    }
    
    // Should handle most requests (some rate limiting is OK)
    expect(errorCount / requestCount).toBeLessThan(0.1); // <10% errors
  });
});
```

---

## Migration Strategy (Audit --Fix)

For existing daemons that don't follow this pattern:

```bash
# Scan existing daemons
./jtag generate/audit

# Output:
Scanning daemons for pattern compliance...

❌ HealthDaemon (pattern v0 - needs upgrade)
   Missing: RateLimiter, AsyncQueue, Semaphore, Metrics
   
❌ DataDaemon (pattern v0 - needs upgrade)
   Missing: RateLimiter, Metrics
   Has: Custom queue (should use AsyncQueue)
   
⚠️  EventDaemon (pattern v0.5 - partial)
   Has: RateLimiter
   Missing: AsyncQueue, Semaphore, Metrics

✅ CommandDaemon (pattern v1 - compliant)
   All layers implemented correctly

# Auto-migrate
./jtag generate/audit --fix

# Wraps existing handleMessage with pattern layers:
# 1. Adds rate limiter check
# 2. Wraps in AsyncQueue
# 3. Adds semaphore
# 4. Injects metrics
# 5. Generates tests
# 6. Updates PATTERN_VERSION

Migrating HealthDaemon...
  ✅ Added RateLimiter
  ✅ Added AsyncQueue
  ✅ Added Semaphore  
  ✅ Added Metrics
  ✅ Generated tests
  ✅ Updated to pattern v1

Migration complete! Run tests to verify:
  npx vitest daemons/health-daemon/test/
```

---

## Pattern Evolution

As we discover better patterns:

1. **Increment version**: `DAEMON_PATTERN_VERSION = 2`
2. **Document changes**: What improved and why
3. **Update generator**: New daemons use v2
4. **Provide migration**: `audit --fix` upgrades v1 → v2
5. **Gradual rollout**: Migrate critical daemons first

**Example Evolution** (hypothetical v2):
```
Pattern v2 adds:
- Circuit breaker (fail fast when downstream is down)
- Request deduplication (collapse identical concurrent requests)
- Priority queues (high-priority requests jump queue)
```

---

## Implementation Checklist

To implement this pattern system:

- [ ] Create concurrency primitives:
  - [ ] `system/concurrency/RateLimiter.ts`
  - [ ] `system/concurrency/AsyncQueue.ts`
  - [ ] `system/concurrency/Semaphore.ts`
  - [ ] `system/daemon/shared/DaemonMetrics.ts`

- [ ] Update DaemonGenerator:
  - [ ] Generate pattern-compliant code
  - [ ] Add PATTERN_VERSION constant
  - [ ] Generate unit/integration/load tests
  - [ ] Add metrics endpoint

- [ ] Create audit command:
  - [ ] `commands/generate/audit/` structure
  - [ ] Scan existing daemons for violations
  - [ ] Report pattern compliance
  - [ ] Implement --fix migration

- [ ] Migrate existing daemons:
  - [ ] HealthDaemon
  - [ ] DataDaemon
  - [ ] EventDaemon
  - [ ] CommandDaemon

- [ ] Documentation:
  - [ ] Update CLAUDE.md with pattern reference
  - [ ] Add examples to each daemon README
  - [ ] Create video walkthrough (optional)

---

## Summary: The Promise

By enforcing this pattern through generators:

✅ **Safety**: Race conditions become structurally impossible  
✅ **Performance**: Optimal concurrency without manual tuning  
✅ **Observability**: Always know what's happening inside  
✅ **Testability**: Concurrency behavior validated automatically  
✅ **Evolution**: Pattern improves over time, all code benefits  

**"Generate quality, don't just hope for it."**
