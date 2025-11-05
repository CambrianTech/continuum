# Worker Thread Implementation Plan - Incremental Build

## Philosophy: Get The Hard Part Right First

**HARD PART**: Threading, message passing, bidirectional communication
**EASY PART**: Once inside worker context, you're writing normal synchronous code (like `handleFrame()` in CBAR)

**Critical Principle**: Verify skeleton communication is working and responding correctly **BEFORE** adding complex implementation. Threading issues are the hard part - persona logic is easy once you're in the right context.

---

## Phase 1: Bare Skeleton - Communication Verification (Week 1, Days 1-2)

**Goal**: Prove worker thread can receive messages and respond. NO complex logic.

### Step 1.1: Create Minimal Worker (1 hour)
**File**: `system/conversation/worker/persona-worker.js`

```javascript
// MINIMAL skeleton - just prove communication works
const { parentPort, workerData } = require('worker_threads');

console.log(`🧵 PersonaWorker[${workerData.personaId}]: Starting...`);

// Listen for messages from main thread
parentPort.on('message', (msg) => {
    console.log(`🧵 PersonaWorker[${workerData.personaId}]: Received message type=${msg.type}`);

    if (msg.type === 'ping') {
        // Echo back - prove bidirectional communication works
        parentPort.postMessage({
            type: 'pong',
            timestamp: Date.now(),
            receivedAt: msg.timestamp,
            latency: Date.now() - msg.timestamp
        });
    }
});

// Signal ready
parentPort.postMessage({
    type: 'ready',
    personaId: workerData.personaId,
    timestamp: Date.now()
});

console.log(`✅ PersonaWorker[${workerData.personaId}]: Initialized and ready`);
```

### Step 1.2: Create Worker Manager (2 hours)
**File**: `system/conversation/worker/PersonaWorkerThread.ts`

```typescript
import { Worker } from 'worker_threads';
import { EventEmitter } from 'events';
import * as path from 'path';

interface WorkerMessage {
    type: 'ping' | 'evaluate' | 'abort' | 'shutdown';
    timestamp: number;
    data?: any;
}

interface WorkerResponse {
    type: 'ready' | 'pong' | 'result' | 'error';
    timestamp: number;
    data?: any;
}

/**
 * Manages a single PersonaUser worker thread.
 *
 * Similar to CBAR's QueueThread<T> pattern.
 */
export class PersonaWorkerThread extends EventEmitter {
    private worker: Worker | null = null;
    private personaId: string;
    private isReady: boolean = false;
    private messageCount: number = 0;

    constructor(personaId: string) {
        super();
        this.personaId = personaId;
    }

    async start(): Promise<void> {
        const workerPath = path.join(__dirname, 'persona-worker.js');

        console.log(`🧵 Starting worker for persona ${this.personaId}`);

        this.worker = new Worker(workerPath, {
            workerData: { personaId: this.personaId }
        });

        // Listen for messages from worker
        this.worker.on('message', (msg: WorkerResponse) => {
            this.handleWorkerMessage(msg);
        });

        this.worker.on('error', (error) => {
            console.error(`❌ Worker error for ${this.personaId}:`, error);
            this.emit('error', error);
        });

        this.worker.on('exit', (code) => {
            console.log(`🧵 Worker ${this.personaId} exited with code ${code}`);
            this.emit('exit', code);
        });

        // Wait for ready signal
        return new Promise((resolve, reject) => {
            const timeout = setTimeout(() => {
                reject(new Error(`Worker ${this.personaId} did not signal ready within 5s`));
            }, 5000);

            this.once('ready', () => {
                clearTimeout(timeout);
                resolve();
            });
        });
    }

    private handleWorkerMessage(msg: WorkerResponse): void {
        console.log(`📨 Main thread received from worker ${this.personaId}: type=${msg.type}`);

        if (msg.type === 'ready') {
            this.isReady = true;
            this.emit('ready');
        } else if (msg.type === 'pong') {
            const latency = Date.now() - msg.timestamp;
            console.log(`🏓 Pong from ${this.personaId}: latency=${latency}ms`);
            this.emit('pong', msg);
        }
    }

    /**
     * Send ping to worker - verify communication works.
     */
    async ping(): Promise<number> {
        if (!this.isReady || !this.worker) {
            throw new Error(`Worker ${this.personaId} not ready`);
        }

        const startTime = Date.now();

        this.worker.postMessage({
            type: 'ping',
            timestamp: startTime
        });

        // Wait for pong response
        return new Promise((resolve, reject) => {
            const timeout = setTimeout(() => {
                reject(new Error(`Worker ${this.personaId} did not respond to ping within 1s`));
            }, 1000);

            this.once('pong', (msg: WorkerResponse) => {
                clearTimeout(timeout);
                const latency = Date.now() - startTime;
                resolve(latency);
            });
        });
    }

    async shutdown(): Promise<void> {
        if (this.worker) {
            console.log(`🛑 Shutting down worker ${this.personaId}`);
            await this.worker.terminate();
            this.worker = null;
        }
    }
}
```

### Step 1.3: Test Skeleton Communication (30 minutes)
**File**: `system/conversation/worker/test-worker-skeleton.ts`

```typescript
import { PersonaWorkerThread } from './PersonaWorkerThread';

async function testWorkerSkeleton(): Promise<void> {
    console.log('\n=== Testing Worker Thread Skeleton ===\n');

    const worker = new PersonaWorkerThread('test-persona-123');

    try {
        // TEST 1: Start worker and wait for ready
        console.log('TEST 1: Starting worker...');
        await worker.start();
        console.log('✅ Worker started and signaled ready\n');

        // TEST 2: Ping-pong communication
        console.log('TEST 2: Testing ping-pong...');
        const latency1 = await worker.ping();
        console.log(`✅ Ping 1: ${latency1}ms\n`);

        const latency2 = await worker.ping();
        console.log(`✅ Ping 2: ${latency2}ms\n`);

        const latency3 = await worker.ping();
        console.log(`✅ Ping 3: ${latency3}ms\n`);

        // TEST 3: Rapid ping sequence (stress test communication)
        console.log('TEST 3: Rapid ping sequence (10 pings)...');
        const latencies = [];
        for (let i = 0; i < 10; i++) {
            const latency = await worker.ping();
            latencies.push(latency);
        }
        const avgLatency = latencies.reduce((a, b) => a + b, 0) / latencies.length;
        console.log(`✅ Avg latency: ${avgLatency.toFixed(2)}ms\n`);

        // TEST 4: Shutdown
        console.log('TEST 4: Shutting down worker...');
        await worker.shutdown();
        console.log('✅ Worker shut down cleanly\n');

        console.log('🎉 ALL SKELETON TESTS PASSED\n');

    } catch (error) {
        console.error('❌ SKELETON TEST FAILED:', error);
        await worker.shutdown();
        process.exit(1);
    }
}

testWorkerSkeleton();
```

### Step 1.4: Run Test - Verify Communication Works
```bash
cd /Volumes/FlashGordon/cambrian/continuum
npx tsx system/conversation/worker/test-worker-skeleton.ts
```

**Success Criteria**:
- ✅ Worker starts and signals ready within 5s
- ✅ Ping-pong works with <10ms latency
- ✅ Multiple rapid pings work without errors
- ✅ Worker shuts down cleanly
- ✅ No threading errors, no hangs, no race conditions

**If this passes, THE HARD PART IS DONE. Everything else is easy.**

---

## Phase 2: Add Simple Processing (Week 1, Days 3-4)

**Goal**: Once skeleton works, add simple message processing. Still no real inference.

### Step 2.1: Add Message Evaluation Skeleton
**Update**: `system/conversation/worker/persona-worker.js`

```javascript
const { parentPort, workerData } = require('worker_threads');

parentPort.on('message', async (msg) => {
    console.log(`🧵 Worker[${workerData.personaId}]: Received ${msg.type}`);

    if (msg.type === 'ping') {
        parentPort.postMessage({ type: 'pong', timestamp: Date.now() });
    }
    else if (msg.type === 'evaluate') {
        // MOCK evaluation - no real inference yet
        const startTime = Date.now();

        // Simulate thinking time (100-500ms random)
        const thinkTime = 100 + Math.random() * 400;
        await new Promise(resolve => setTimeout(resolve, thinkTime));

        // Mock decision (random confidence)
        const confidence = 0.3 + Math.random() * 0.6;
        const shouldRespond = confidence > 0.5;

        const processingTime = Date.now() - startTime;

        parentPort.postMessage({
            type: 'result',
            messageId: msg.message.id,
            confidence: confidence,
            shouldRespond: shouldRespond,
            reasoning: `Mock evaluation (${thinkTime.toFixed(0)}ms think time)`,
            processingTime: processingTime,
            timestamp: msg.timestamp
        });

        console.log(`✅ Worker[${workerData.personaId}]: Evaluated ${msg.message.id} in ${processingTime}ms`);
    }
});

parentPort.postMessage({ type: 'ready', personaId: workerData.personaId });
```

### Step 2.2: Add Evaluation to PersonaWorkerThread
**Update**: `system/conversation/worker/PersonaWorkerThread.ts`

```typescript
// Add to PersonaWorkerThread class:

async evaluateMessage(message: ChatMessage): Promise<EvaluationResult> {
    if (!this.isReady || !this.worker) {
        throw new Error(`Worker ${this.personaId} not ready`);
    }

    const startTime = Date.now();
    this.messageCount++;

    this.worker.postMessage({
        type: 'evaluate',
        message: message,
        timestamp: startTime
    });

    // Wait for result
    return new Promise((resolve, reject) => {
        const timeout = setTimeout(() => {
            reject(new Error(`Worker ${this.personaId} did not respond within 5s`));
        }, 5000);

        const handler = (msg: WorkerResponse) => {
            if (msg.type === 'result' && msg.data?.messageId === message.id) {
                clearTimeout(timeout);
                this.removeListener('message', handler);

                const totalLatency = Date.now() - startTime;
                console.log(`📊 Worker ${this.personaId}: Total latency ${totalLatency}ms`);

                resolve(msg.data);
            }
        };

        this.worker!.on('message', handler);
    });
}
```

### Step 2.3: Test Mock Evaluation
```typescript
// TEST: Send mock messages, verify worker responds with evaluation
const result = await worker.evaluateMessage({
    id: 'test-msg-123',
    content: 'Test message',
    senderId: 'test-user'
});

console.log('Result:', result);
// Expected: { confidence: 0.6, shouldRespond: true, reasoning: "Mock evaluation..." }
```

**Success Criteria**:
- ✅ Worker receives evaluation request
- ✅ Worker returns result with correct messageId
- ✅ Processing time reasonable (<500ms for mock)
- ✅ Multiple evaluations work in sequence
- ✅ No threading issues

---

## Phase 3: Add Real Inference (Week 1, Day 5)

**Goal**: Now that communication works, add actual Ollama inference. This is the EASY part - just normal async code inside worker.

### Step 3.1: Initialize Ollama in Worker
**Update**: `system/conversation/worker/persona-worker.js`

```javascript
// At top of worker file
const { OllamaAdapter } = require('../../../daemons/ai-provider-daemon/shared/OllamaAdapter');

let ollama = null;

async function initialize() {
    ollama = new OllamaAdapter({
        baseUrl: 'http://localhost:11434',
        maxConcurrent: 1  // One request per worker
    });

    console.log(`✅ Worker[${workerData.personaId}]: Ollama initialized`);

    parentPort.postMessage({ type: 'ready', personaId: workerData.personaId });
}

// Inside 'evaluate' handler:
else if (msg.type === 'evaluate') {
    const startTime = Date.now();

    try {
        // REAL inference now (not mock)
        const prompt = buildEvaluationPrompt(msg.message);

        const response = await ollama.generate({
            model: 'llama3.2:3b',
            prompt: prompt,
            stream: false
        });

        // Parse response for confidence/decision
        const evaluation = parseEvaluationResponse(response.text);

        const processingTime = Date.now() - startTime;

        parentPort.postMessage({
            type: 'result',
            messageId: msg.message.id,
            confidence: evaluation.confidence,
            shouldRespond: evaluation.shouldRespond,
            reasoning: evaluation.reasoning,
            processingTime: processingTime,
            timestamp: msg.timestamp
        });

    } catch (error) {
        parentPort.postMessage({
            type: 'error',
            messageId: msg.message.id,
            error: error.message,
            timestamp: msg.timestamp
        });
    }
}

// Start initialization
initialize();
```

**Success Criteria**:
- ✅ Worker initializes Ollama successfully
- ✅ Real inference works (3-30 seconds)
- ✅ Worker doesn't block main thread
- ✅ Errors handled gracefully

---

## Phase 4: Multiple Workers + Coordination (Week 2)

**Goal**: Scale to multiple workers, integrate with ThoughtStreamCoordinator.

### Step 4.1: Worker Pool Manager
```typescript
class WorkerPool {
    private workers = new Map<UUID, PersonaWorkerThread>();

    async initializeWorker(personaId: UUID): Promise<void> {
        const worker = new PersonaWorkerThread(personaId);
        await worker.start();
        this.workers.set(personaId, worker);
    }

    async evaluateMessage(message: ChatMessage): Promise<Map<UUID, EvaluationResult>> {
        // Send to ALL workers in parallel (non-blocking)
        const promises = [];
        for (const [personaId, worker] of this.workers) {
            promises.push(
                worker.evaluateMessage(message)
                    .then(result => [personaId, result])
                    .catch(error => [personaId, { error }])
            );
        }

        // Collect results as they arrive
        const results = await Promise.all(promises);
        return new Map(results);
    }
}
```

### Step 4.2: Integrate with ThoughtStreamCoordinator
- Replace `async/await` serial evaluation with worker pool
- Apply temporal decay to worker results
- Make decisions with partially available results (timeout-based)

---

## Phase 5: Persona Internal Threading (Future)

**Goal**: Personas might need internal threading for thinking/reasoning.

**Example**: Multi-step reasoning
```javascript
// Inside persona-worker.js
async function evaluateWithReasoning(message) {
    // Step 1: Quick relevance check (fast model, 100ms)
    const relevance = await quickRelevanceCheck(message);
    if (relevance < 0.3) {
        return { confidence: 0, shouldRespond: false };
    }

    // Step 2: Deep reasoning (slow model, 10s)
    const reasoning = await deepReasoning(message);

    // Step 3: Confidence calculation
    const confidence = calculateConfidence(relevance, reasoning);

    return { confidence, shouldRespond: confidence > 0.5, reasoning };
}
```

**Could be threaded internally**:
- Thread 1: Quick relevance (always running, 100ms)
- Thread 2: Deep reasoning (3-10s, queued)
- Return quick result immediately, update with deep reasoning later

---

## Test-Driven Development Strategy

**Following Existing Patterns**: See `tests/integration/*.test.ts` for reference implementations.

### Test Structure Pattern
```typescript
/**
 * Worker Thread Integration Test
 * ==============================
 *
 * Tests bidirectional communication, latency, and reliability
 * of PersonaUser worker threads.
 *
 * Success Criteria:
 * - Worker starts reliably (<5s)
 * - Ping-pong latency <10ms
 * - Multiple rapid pings without errors
 * - Clean shutdown without hangs
 */

import { runJtagCommand } from '../test-utils/CRUDTestUtils';
import { execSync } from 'child_process';

interface TestResult {
  scenario: string;
  passed: boolean;
  metrics: {
    latency?: number;
    throughput?: number;
    errorRate?: number;
  };
  notes: string;
}
```

### Phase 1: Worker Skeleton Tests

**Test File**: `tests/integration/worker-skeleton.test.ts`

**Test Scenarios**:
1. **Worker Lifecycle**
   ```typescript
   async function testScenario_WorkerStartup(): Promise<TestResult> {
     const startTime = Date.now();

     // Create worker
     const worker = new PersonaWorkerThread('test-persona-123');

     // Wait for ready signal
     await worker.start();

     const startupTime = Date.now() - startTime;
     const passed = startupTime < 5000;

     return {
       scenario: 'Worker Startup',
       passed,
       metrics: { latency: startupTime },
       notes: passed
         ? `✅ Worker started in ${startupTime}ms`
         : `❌ Worker took ${startupTime}ms (>5s limit)`
     };
   }
   ```

2. **Ping-Pong Communication**
   ```typescript
   async function testScenario_PingPong(): Promise<TestResult> {
     const worker = new PersonaWorkerThread('test-persona-123');
     await worker.start();

     const latencies: number[] = [];

     // Test 10 pings
     for (let i = 0; i < 10; i++) {
       const latency = await worker.ping();
       latencies.push(latency);
     }

     const avgLatency = latencies.reduce((a, b) => a + b, 0) / latencies.length;
     const passed = avgLatency < 10;

     await worker.shutdown();

     return {
       scenario: 'Ping-Pong Communication',
       passed,
       metrics: {
         latency: avgLatency,
         throughput: 10 / (latencies.reduce((a, b) => a + b, 0) / 1000)
       },
       notes: passed
         ? `✅ Avg latency ${avgLatency.toFixed(2)}ms`
         : `❌ Avg latency ${avgLatency.toFixed(2)}ms (>10ms limit)`
     };
   }
   ```

3. **Rapid Fire Stress Test**
   ```typescript
   async function testScenario_RapidFire(): Promise<TestResult> {
     const worker = new PersonaWorkerThread('test-persona-123');
     await worker.start();

     const startTime = Date.now();
     const promises = [];

     // Send 100 pings concurrently
     for (let i = 0; i < 100; i++) {
       promises.push(worker.ping().catch(() => -1));
     }

     const results = await Promise.all(promises);
     const elapsed = Date.now() - startTime;

     const errorCount = results.filter(r => r === -1).length;
     const errorRate = errorCount / results.length;
     const passed = errorRate < 0.01; // <1% error rate

     await worker.shutdown();

     return {
       scenario: 'Rapid Fire Stress Test',
       passed,
       metrics: {
         throughput: 100 / (elapsed / 1000),
         errorRate
       },
       notes: passed
         ? `✅ ${errorCount}/100 errors (${(errorRate * 100).toFixed(1)}%)`
         : `❌ ${errorCount}/100 errors (${(errorRate * 100).toFixed(1)}% >1% limit)`
     };
   }
   ```

4. **Clean Shutdown**
   ```typescript
   async function testScenario_CleanShutdown(): Promise<TestResult> {
     const worker = new PersonaWorkerThread('test-persona-123');
     await worker.start();

     const startTime = Date.now();
     await worker.shutdown();
     const shutdownTime = Date.now() - startTime;

     const passed = shutdownTime < 1000;

     return {
       scenario: 'Clean Shutdown',
       passed,
       metrics: { latency: shutdownTime },
       notes: passed
         ? `✅ Shutdown in ${shutdownTime}ms`
         : `❌ Shutdown took ${shutdownTime}ms (>1s limit)`
     };
   }
   ```

**Main Test Runner**:
```typescript
async function runWorkerSkeletonTests() {
  console.log('\n🧪 WORKER THREAD SKELETON TEST SUITE');
  console.log('='.repeat(60));

  const results: TestResult[] = [];

  try {
    results.push(await testScenario_WorkerStartup());
    results.push(await testScenario_PingPong());
    results.push(await testScenario_RapidFire());
    results.push(await testScenario_CleanShutdown());
  } catch (error) {
    console.error('\n❌ Test suite failed:', error);
    process.exit(1);
  }

  // Summary
  console.log('\n\n📊 TEST RESULTS SUMMARY');
  console.log('='.repeat(60));

  const passed = results.filter(r => r.passed).length;
  const total = results.length;
  const passRate = (passed / total * 100).toFixed(0);

  results.forEach(r => {
    const status = r.passed ? '✅' : '❌';
    console.log(`${status} ${r.scenario}`);
    console.log(`   ${r.notes}`);
  });

  console.log('\n📈 AGGREGATE METRICS');
  console.log('='.repeat(60));
  console.log(`Pass Rate: ${passed}/${total} (${passRate}%)`);

  // Save results for comparison
  const resultsSummary = {
    timestamp: new Date().toISOString(),
    phase: 'Phase 1: Skeleton',
    passRate: `${passRate}%`,
    details: results
  };

  const fs = await import('fs');
  const path = await import('path');
  const resultsDir = path.join(process.cwd(), '.continuum/sessions/validation');
  const resultsFile = path.join(resultsDir, 'worker-skeleton-results-latest.json');

  await fs.promises.mkdir(resultsDir, { recursive: true });
  await fs.promises.writeFile(resultsFile, JSON.stringify(resultsSummary, null, 2));

  console.log('\n💾 Results saved to:', resultsFile);

  if (passRate >= '100') {
    console.log('✅ ALL TESTS PASSED - Ready for Phase 2');
    process.exit(0);
  } else {
    console.log('❌ SOME TESTS FAILED - Fix before proceeding');
    process.exit(1);
  }
}

// Run tests
runWorkerSkeletonTests().catch(error => {
  console.error('❌ Test runner failed:', error);
  process.exit(1);
});
```

### Phase 2: Mock Evaluation Tests

**Test File**: `tests/integration/worker-mock-evaluation.test.ts`

**Test Scenarios**:
1. Mock evaluation returns result with correct messageId
2. Multiple evaluations in sequence
3. Processing time reasonable (<500ms for mock)
4. Timeout handling (worker doesn't respond)

### Phase 3: Real Inference Tests

**Test File**: `tests/integration/worker-real-inference.test.ts`

**Test Scenarios**:
1. Real Ollama inference completes successfully
2. Main thread never blocked during 30s inference
3. Worker recovers from Ollama timeout
4. Multiple workers run in parallel without interference

### Phase 4: Integration Tests with ThoughtStreamCoordinator

**Test File**: `tests/integration/worker-coordinator-integration.test.ts`

**Test Scenarios**:
1. Multiple workers evaluate same message in parallel
2. Temporal decay applied correctly
3. Decision made with partial results (timeout-based)
4. AI-AI conversation flows naturally (<500ms response)

### Test Execution Pattern

**Run tests via npm script**:
```bash
cd src/debug/jtag
npm run test:worker-skeleton
npm run test:worker-evaluation
npm run test:worker-integration
```

**Add to package.json**:
```json
{
  "scripts": {
    "test:worker-skeleton": "npx tsx tests/integration/worker-skeleton.test.ts",
    "test:worker-evaluation": "npx tsx tests/integration/worker-mock-evaluation.test.ts",
    "test:worker-inference": "npx tsx tests/integration/worker-real-inference.test.ts",
    "test:worker-integration": "npx tsx tests/integration/worker-coordinator-integration.test.ts",
    "test:worker-all": "npm run test:worker-skeleton && npm run test:worker-evaluation && npm run test:worker-inference && npm run test:worker-integration"
  }
}
```

### TDD Development Cycle

**FOR EACH PHASE**:

1. **Write Test First** (Red)
   ```bash
   # Write test that WILL fail (code doesn't exist yet)
   vim tests/integration/worker-skeleton.test.ts

   # Run test - should fail
   npm run test:worker-skeleton
   ```

2. **Implement Minimum Code** (Green)
   ```bash
   # Implement just enough to pass test
   vim system/conversation/worker/PersonaWorkerThread.ts
   vim system/conversation/worker/persona-worker.js

   # Run test - should pass
   npm run test:worker-skeleton
   ```

3. **Refactor** (Clean)
   ```bash
   # Improve code quality without changing behavior
   # Run test again - should still pass
   npm run test:worker-skeleton
   ```

4. **Commit When Green**
   ```bash
   git add .
   git commit -m "feat: Phase 1 worker skeleton with ping-pong tests"
   ```

### Success Gates

**Cannot proceed to next phase until**:
- ✅ All tests in current phase pass (100% pass rate)
- ✅ No threading errors or race conditions
- ✅ Clean shutdown without hangs
- ✅ Results saved to `.continuum/sessions/validation/`

### Integration with CI/CD

**Pre-commit hook**:
```bash
# .git/hooks/pre-commit
npm run test:worker-all
if [ $? -ne 0 ]; then
  echo "❌ Worker tests failed - fix before committing"
  exit 1
fi
```

---

## Success Metrics Per Phase

### Phase 1 Success:
- ✅ Worker starts reliably
- ✅ Ping-pong <10ms latency
- ✅ No threading errors after 1000 pings
- ✅ Clean shutdown

### Phase 2 Success:
- ✅ Mock evaluation works
- ✅ Result matches request (correct messageId)
- ✅ Multiple evaluations work
- ✅ No blocking of main thread

### Phase 3 Success:
- ✅ Real Ollama inference works in worker
- ✅ 3-30s inference doesn't block main thread
- ✅ Error handling works (Ollama down, timeout)
- ✅ Worker recoverable from errors

### Phase 4 Success:
- ✅ 3-5 workers run in parallel
- ✅ Temporal decay applied correctly
- ✅ Coordinator makes decisions with partial results
- ✅ Conversation flows naturally (<500ms response)

---

## Key Insights from CBAR Architecture

1. **QueueThread<T> with size=1**: Drop old work if processing too slow
   - Our equivalent: Drop old messages if worker queue full

2. **timedWait with pthread_cond_t**: Don't busy-wait, block efficiently
   - Our equivalent: Use `Atomics.wait()` or event-based waiting

3. **Fast path + Slow path**: FeatureTracker (60fps) + SemanticSegmenter (3s)
   - Our equivalent: Coordinator decision (100ms timeout) + Worker inference (3-30s)

4. **Temporal compensation**: Adjust stale results forward in time
   - Our equivalent: Confidence decay based on result age

5. **Pipeline parallelism**: Always processing SOMETHING
   - Our equivalent: Workers always evaluating recent messages proactively

---

## Critical Debugging Tools

### Add to Worker
```javascript
// Inside worker for debugging
parentPort.on('message', (msg) => {
    const receiveTime = Date.now();
    const queueDelay = receiveTime - msg.timestamp;

    console.log(`🧵 Worker: Received after ${queueDelay}ms in queue`);

    // Process...

    const totalTime = Date.now() - msg.timestamp;
    console.log(`🧵 Worker: Total time ${totalTime}ms (${queueDelay}ms queue + ${totalTime - queueDelay}ms processing)`);
});
```

### Add to Main Thread
```typescript
const sendTime = Date.now();
worker.postMessage({ type: 'evaluate', message, timestamp: sendTime });

// Later when result arrives:
const latency = Date.now() - sendTime;
console.log(`📊 Main: Round-trip latency ${latency}ms`);
```

---

## Risk Mitigation

### Risk: Worker Deadlock
**Mitigation**: Always use timeouts on `Promise` wrappers for worker communication

### Risk: Worker Crash
**Mitigation**: Worker pool manager restarts crashed workers automatically

### Risk: Memory Leak
**Mitigation**: Profile with `process.memoryUsage()` in worker, shut down and restart periodically

### Risk: Message Order
**Mitigation**: Include sequence numbers in messages, detect out-of-order delivery

---

## Dependencies

**Required**:
- Node.js `worker_threads` module (built-in)
- TypeScript configuration for worker compilation

**Optional but Recommended**:
- `SharedArrayBuffer` for atomic synchronization
- Worker thread pool library (if we need more than 5 workers)

---

## Timeline

**Week 1**:
- Days 1-2: Phase 1 (Skeleton) - THE HARD PART
- Days 3-4: Phase 2 (Mock processing)
- Day 5: Phase 3 (Real inference)

**Week 2**:
- Days 1-3: Phase 4 (Multiple workers + coordination)
- Days 4-5: Testing + polish

**Week 3+**:
- Phase 5 (Internal persona threading) - OPTIONAL, FUTURE

---

## Conclusion

**The Critical Path**: Get Phase 1 skeleton working perfectly. Once bidirectional communication works reliably, everything else is straightforward synchronous code inside the worker context.

**User's Insight**: Threading/IPC is the hard part. Once you're inside `handleMessage()` in the worker, you're writing normal code - it's like being in `main()`.

**Next Step**: Start with Phase 1 skeleton implementation and verify communication works before adding ANY complex logic.
