# Worker Thread Integration with Existing Architecture

## What Already Exists (Don't Duplicate)

### 1. Health Monitoring (BaseAIProviderAdapter)
**Location**: `daemons/ai-provider-daemon/shared/BaseAIProviderAdapter.ts`

**Already Provides:**
- ✅ Health checks every 30 seconds
- ✅ Automatic restart after 3 consecutive failures
- ✅ All providers (Ollama, OpenAI, Anthropic) inherit this
- ✅ Provider-specific restart logic (`restartProvider()`)

**Worker Thread Integration:**
```typescript
// Worker does NOT duplicate health monitoring
// Instead, it uses the provider's existing health system

// Inside persona-worker.js
import { OllamaAdapter } from '../../../daemons/ai-provider-daemon/shared/OllamaAdapter';

// Adapter already has health monitoring built-in
const ollama = new OllamaAdapter({ /* config */ });
await ollama.initialize();  // Starts health monitoring automatically

// Worker just uses it - if Ollama degrades, adapter handles restart
const result = await ollama.generateText(request);
// If this fails, adapter's health monitor detects and restarts Ollama
```

### 2. Provider Abstraction (AIProviderTypes)
**Location**: `daemons/ai-provider-daemon/shared/AIProviderTypes.ts`

**Already Provides:**
- ✅ Common interface for all providers (Ollama, OpenAI, Anthropic, etc.)
- ✅ TextGenerationRequest/Response types
- ✅ HealthStatus types
- ✅ Error handling (AIProviderError)

**Worker Thread Integration:**
```typescript
// Worker is PROVIDER AGNOSTIC
// Works with ANY adapter that extends BaseAIProviderAdapter

interface WorkerData {
  personaId: UUID;
  providerType: 'ollama' | 'openai' | 'anthropic';  // Configurable
  providerConfig: ProviderConfiguration;
}

// Worker instantiates correct provider
let provider: BaseAIProviderAdapter;
if (workerData.providerType === 'ollama') {
  provider = new OllamaAdapter(workerData.providerConfig);
} else if (workerData.providerType === 'openai') {
  provider = new OpenAIAdapter(workerData.providerConfig);
}
// etc.

await provider.initialize();  // All providers have same interface
```

### 3. Genome System (LoRA Virtual Memory)
**Location**: `design/GENOME-*.md` (extensive docs already written)

**Already Designed:**
- ✅ LoRA layers as virtual memory pages
- ✅ Hot/warm/cold pool management
- ✅ Genome assembly from base model + LoRA stack
- ✅ Monitoring dashboard (`genome/stats`)
- ✅ Dynamic loading/unloading

**Worker Thread Integration:**
```typescript
// Worker uses GenomeLoader to manage LoRA layers
import { GenomeLoader } from '../../../system/genome/GenomeLoader';

const genomeLoader = new GenomeLoader();

// Persona defines its LoRA stack
const genome = await genomeLoader.loadGenome({
  personaId: workerData.personaId,
  layers: [
    { type: 'base', name: 'llama3.2:1b' },
    { type: 'lora', name: 'general-coding', pool: 'hot' },
    { type: 'lora', name: 'typescript-expert', pool: 'warm' },
    { type: 'lora', name: 'persona-specific', pool: 'cold' }
  ]
});

// Inference uses assembled genome
const result = await genome.generate(request);

// GenomeLoader handles hot/warm/cold swapping automatically
// Just like CBAR CV system handles frame processing
```

### 4. Request Queue Management (OllamaRequestQueue)
**Location**: `daemons/ai-provider-daemon/shared/OllamaAdapter.ts`

**Already Provides:**
- ✅ Max 4 concurrent Ollama requests
- ✅ Queue with backpressure
- ✅ Timeout/cancellation support (AbortController)

**Worker Thread Integration:**
```typescript
// Worker doesn't need its own queue
// OllamaAdapter already queues requests internally

// Multiple workers can share same Ollama instance
const ollama = new OllamaAdapter({ maxConcurrent: 4 });

// Each worker calls generateText()
// Adapter queues them automatically
const result = await ollama.generateText(request);
// Adapter ensures max 4 concurrent, drops/rejects overload
```

---

## What Worker Threads Add (New Capabilities)

### 1. Real Parallelism (Not Async/Await)
**Problem Solved**: Single-threaded async/await blocks event loop during 3-30s inference

**Solution**: Worker threads run in separate CPU threads
```typescript
// Main thread: Never blocked
worker1.evaluateMessage(msg);  // ~1ms to send
worker2.evaluateMessage(msg);  // ~1ms to send
worker3.evaluateMessage(msg);  // ~1ms to send
// Main thread continues immediately, workers process in parallel

// Workers: Each has own event loop
// Worker 1 running 30s Ollama inference
// Worker 2 running 30s Ollama inference
// Worker 3 running 30s Ollama inference
// All in parallel (not sequential like Promise.all)
```

### 2. Temporal Confidence Decay
**Problem Solved**: AI evaluates message at t=0, but doesn't respond until t=30s - conversation has moved on

**Solution**: Decay confidence based on result age (like CBAR optical flow compensation)
```typescript
// Worker completes at t=30s with confidence=0.9
const result = {
  confidence: 0.9,
  timestamp: t0,
  completedAt: t30
};

// Main thread applies temporal decay
const age = Date.now() - result.timestamp;  // 30000ms
const decayedConfidence = result.confidence * Math.exp(-lambda * age);
// 0.9 * exp(-0.00014 * 30000) = 0.9 * 0.01 = 0.009
// Confidence heavily decayed - conversation moved on
```

### 3. Pipeline Parallelism
**Problem Solved**: Workers sit idle between messages

**Solution**: Always evaluating SOMETHING (like CBAR always processing frames)
```typescript
// Proactive evaluation - workers never idle
class WorkerPool {
  scheduleProactiveEvaluation(): void {
    setInterval(() => {
      const idleWorkers = this.workers.filter(w => w.isIdle());
      const unevaluatedMessages = this.getRecentMessages();

      for (const worker of idleWorkers) {
        if (unevaluatedMessages.length > 0) {
          const msg = unevaluatedMessages.shift();
          worker.evaluateMessage(msg);  // Pre-evaluate
        }
      }
    }, 100);
  }
}
```

---

## Integration Architecture

### Worker Initialization
```typescript
// persona-worker.js
import { parentPort, workerData } from 'worker_threads';
import { BaseAIProviderAdapter } from '../../../daemons/ai-provider-daemon/shared/BaseAIProviderAdapter';
import { OllamaAdapter } from '../../../daemons/ai-provider-daemon/shared/OllamaAdapter';
import { GenomeLoader } from '../../../system/genome/GenomeLoader';

// 1. Initialize provider (with built-in health monitoring)
const provider: BaseAIProviderAdapter = new OllamaAdapter({
  baseUrl: 'http://localhost:11434',
  maxConcurrent: 1  // One request per worker
});
await provider.initialize();  // Starts health monitoring

// 2. Load persona's genome (LoRA stack)
const genomeLoader = new GenomeLoader();
const genome = await genomeLoader.loadGenome({
  personaId: workerData.personaId,
  layers: workerData.genomeConfig.layers
});

// 3. Ready to evaluate
parentPort.postMessage({ type: 'ready', personaId: workerData.personaId });

// 4. Handle evaluation requests
parentPort.on('message', async (msg) => {
  if (msg.type === 'evaluate') {
    const startTime = Date.now();

    try {
      // Use provider's generate method (queued, monitored, recoverable)
      const result = await provider.generateText({
        prompt: buildPrompt(msg.message),
        model: genome.modelName,
        // Provider handles timeout, cancellation, health
      });

      parentPort.postMessage({
        type: 'result',
        messageId: msg.message.id,
        confidence: parseConfidence(result.text),
        timestamp: startTime,
        processingTime: Date.now() - startTime
      });

    } catch (error) {
      // Provider already tried to restart if needed
      // Worker just reports failure
      parentPort.postMessage({
        type: 'error',
        messageId: msg.message.id,
        error: error.message,
        timestamp: startTime
      });
    }
  }
});
```

### Main Thread Coordinator
```typescript
// PersonaWorkerThread.ts
import { BaseAIProviderAdapter } from '../../../daemons/ai-provider-daemon/shared/BaseAIProviderAdapter';

export class PersonaWorkerThread {
  private worker: Worker;
  private providerType: 'ollama' | 'openai' | 'anthropic';

  constructor(
    personaId: UUID,
    providerType: 'ollama' | 'openai' | 'anthropic' = 'ollama'
  ) {
    this.personaId = personaId;
    this.providerType = providerType;
  }

  async start(): Promise<void> {
    // Worker gets provider config
    this.worker = new Worker('./persona-worker.js', {
      workerData: {
        personaId: this.personaId,
        providerType: this.providerType,
        providerConfig: this.getProviderConfig(),
        genomeConfig: this.getGenomeConfig()
      }
    });

    // Wait for worker to initialize provider + genome
    return new Promise((resolve) => {
      this.once('ready', resolve);
    });
  }

  private getProviderConfig(): ProviderConfiguration {
    // Provider-specific config
    if (this.providerType === 'ollama') {
      return {
        baseUrl: 'http://localhost:11434',
        maxConcurrent: 1
      };
    }
    // etc.
  }

  private getGenomeConfig(): GenomeConfiguration {
    // Persona-specific LoRA stack
    return {
      layers: [
        { type: 'base', name: 'llama3.2:1b' },
        { type: 'lora', name: `persona-${this.personaId}`, pool: 'hot' }
      ]
    };
  }
}
```

---

## Worker Failure Handling

### Scenario 1: Provider Degradation (Ollama timeout)
```
1. Worker calls provider.generateText()
2. Ollama times out (45s)
3. Provider's health monitor detects failure
4. Provider restarts Ollama (BaseAIProviderAdapter.restartProvider())
5. Worker retry succeeds
```
**Worker does nothing special - provider handles it**

### Scenario 2: Worker Crash
```
1. Worker crashes (out of memory, unhandled exception)
2. Main thread receives 'exit' event
3. WorkerPool restarts worker
4. Worker re-initializes provider and genome
```

### Scenario 3: Worker Timeout (No Response)
```
1. Main thread sends evaluate message
2. Worker doesn't respond within timeout (30s)
3. Main thread marks worker as degraded
4. WorkerPool spawns replacement worker
5. Old worker terminated forcefully
```

### Scenario 4: Genome Thrashing (LoRA layers)
```
1. Worker requests LoRA layer not in hot/warm pool
2. GenomeLoader evicts cold layer, loads requested layer
3. If eviction rate high (thrashing detected by genome/stats)
4. Main thread adjusts pool sizes or worker count
```

---

## Testing Strategy Update

### Phase 2 Tests Now Include:
1. **Provider Integration**
   ```typescript
   // Test that worker uses provider correctly
   async function testScenario_ProviderIntegration() {
     const worker = new PersonaWorkerThread('test', 'ollama');
     await worker.start();

     // Verify provider initialized
     // Verify health monitoring running
     // Verify requests queued properly
   }
   ```

2. **Genome Loading**
   ```typescript
   // Test that worker loads LoRA stack
   async function testScenario_GenomeLoading() {
     const worker = new PersonaWorkerThread('test', 'ollama');
     // Check genome/stats shows hot/warm/cold pools
     // Check layers loaded correctly
   }
   ```

3. **Provider Degradation Recovery**
   ```typescript
   // Test that worker survives Ollama restart
   async function testScenario_ProviderRestart() {
     const worker = new PersonaWorkerThread('test', 'ollama');

     // Kill Ollama mid-inference
     exec('killall ollama');

     // Worker should recover via BaseAIProviderAdapter
     const result = await worker.evaluateMessage(msg);
     // Should succeed after restart
   }
   ```

---

## Summary: Don't Duplicate, Integrate

**Existing Systems (Use, Don't Recreate):**
- ✅ BaseAIProviderAdapter (health monitoring, restart)
- ✅ AIProviderTypes (provider abstraction)
- ✅ OllamaRequestQueue (concurrency management)
- ✅ GenomeLoader (LoRA virtual memory)
- ✅ genome/stats (monitoring dashboard)

**Worker Threads Add:**
- ✅ Real parallelism (separate CPU threads)
- ✅ Temporal confidence decay (like CBAR optical flow)
- ✅ Pipeline parallelism (always evaluating)
- ✅ Non-blocking main thread

**Integration Points:**
1. Worker instantiates BaseAIProviderAdapter (any provider)
2. Worker uses GenomeLoader for LoRA management
3. Worker reports results with timestamps
4. Main thread applies temporal decay
5. Existing monitoring (health, genome) continues working

**Next Steps:**
1. ✅ Phase 1 complete (ping-pong skeleton)
2. Phase 2: Add provider initialization to worker
3. Phase 3: Add genome loading to worker
4. Phase 4: Add temporal decay to coordinator
5. Phase 5: Production testing with real Ollama

**Files to Update:**
- `system/conversation/worker/persona-worker.js` - Add provider + genome init
- `system/conversation/worker/PersonaWorkerThread.ts` - Add provider/genome config
- `tests/integration/worker-provider-integration.test.ts` - New test suite
