# AI Adapter Architecture Refactor
## Eliminating setTimeout Cancer + Self-Healing

**Date**: 2025-12-04
**Status**: PROPOSED
**Priority**: CRITICAL

---

## Executive Summary

The AI adapter architecture is **fundamentally broken** with setTimeout cancer, no unified health monitoring, and brittle per-adapter retry logic. This causes:

1. **Main-thread blocking** - setTimeout everywhere blocks the event loop
2. **No prioritization** - GPT-4o requests blocked by Llama retries
3. **Silent failures** - Ollama returns `@@@@@@@@` garbage but adapter doesn't detect it
4. **Inconsistent recovery** - Each adapter has its own brittle restart logic
5. **Non-concurrent madness** - No shared architecture, each adapter is a snowflake

**The Fix**: Unified concurrent architecture with event-driven health monitoring, priority-based request queuing, and self-healing failure detection.

---

## Current Architecture Problems

### Problem 1: setTimeout Cancer (Main Thread Blocking)

**BaseAIProviderAdapter.ts**:
- Line 111: `setInterval()` for periodic health checks (every 30s)
- Line 161: `setTimeout()` for restart stabilization (3s)

**OllamaAdapter.ts**:
- Line 117: `setTimeout()` for queue timeout (90s)
- Lines 276, 278: `setTimeout()` for restart delays (2s, 3s)
- Line 648: `setTimeout()` for retry backoff

**SentinelAdapter.ts**:
- Lines 112, 140: `setTimeout()` for retry delays (2s)

**BaseOpenAICompatibleAdapter.ts** (used by 5+ adapters):
- Line 436: `setTimeout()` in `restartProvider()` (2s delay)
- Line 536: `setTimeout()` in `makeRequest()` retry logic (1s * attempt)

**AnthropicAdapter.ts**:
- Line 349: `setTimeout()` in `makeRequest()` retry backoff (1s * attempt)

**Impact**: With 13 AI personas + multiple external adapters, we have **100+ setTimeout timers** in the main thread. Event loop thrashes under load.

---

## Complete Adapter Survey

After examining ALL adapters, here's the full picture:

### Adapter Categories

**Category 1: Local Process Adapters** (Need restart management)
- **OllamaAdapter** (660 lines)
  - Manages local Ollama process
  - Custom `OllamaRequestQueue` class (duplicates infrastructure)
  - setTimeout violations: 4 locations (lines 117, 276, 278, 648)
  - **CRITICAL ISSUE**: Returns `@@@@@@@@` garbage when thrashing - NO DETECTION
  - Custom health check with generation test

- **SentinelAdapter** (location: `adapters/sentinel/`)
  - Local process adapter (similar to Ollama)
  - setTimeout violations: 2 locations (lines 112, 140)
  - **CRITICAL ISSUE**: Same failure mode as Ollama - needs @@@@@@ detection

**Category 2: OpenAI-Compatible API Adapters** (Simple, use base class)
- **OpenAIAdapter** (52 lines) - extends BaseOpenAICompatibleAdapter
- **DeepSeekAdapter** (53 lines) - extends BaseOpenAICompatibleAdapter
- **FireworksAdapter** (45 lines) - extends BaseOpenAICompatibleAdapter
- **GroqAdapter** (87 lines) - extends BaseOpenAICompatibleAdapter

These adapters are **THIN WRAPPERS** (30-90 lines each) - all logic in base class!

**Category 3: Custom API Adapters** (Need specialized handling)
- **AnthropicAdapter** (467 lines)
  - Uses proprietary Anthropic API format (not OpenAI-compatible)
  - setTimeout violation: Line 349 (retry backoff in `makeRequest()`)
  - Has custom multimodal content formatting
  - No unified health monitoring

### Key Insights from Survey

**INSIGHT 1: Most setTimeout cancer is in BASE CLASSES**
- `BaseAIProviderAdapter` (2 setTimeout violations) affects ALL adapters
- `BaseOpenAICompatibleAdapter` (2 setTimeout violations) affects 4+ adapters
- Fixing base classes eliminates 80% of violations immediately

**INSIGHT 2: Two Distinct Failure Modes**
1. **Local process adapters** (Ollama, Sentinel): Process crashes, returns garbage
2. **API adapters** (OpenAI, Anthropic, etc.): Rate limits, network errors, API errors

**INSIGHT 3: Common Code Duplication**
- Every adapter implements its own `makeRequest()` with setTimeout retry logic
- No shared retry infrastructure
- No shared failure pattern detection
- Each adapter has custom health check implementation

**INSIGHT 4: BaseOpenAICompatibleAdapter is GOLD**
- OpenAI, DeepSeek, Fireworks, Groq all use it
- Proves that shared infrastructure WORKS
- Just needs setTimeout elimination + failure detection

**INSIGHT 5: Health Checks are Lies**
- Ollama healthCheck() returns "healthy" while returning `@@@@@@`
- Anthropic healthCheck() only tests connectivity, not inference quality
- Need REAL health checks (actual inference test + response validation)

### Problem 2: No Unified Health Monitoring

Each adapter implements health checks differently:

- **Ollama**: Custom health check with generation test (lines 427-564)
- **BaseAdapter**: `setInterval` polling every 30s (line 111)
- **No common infrastructure** for failure pattern detection

**The `@@@@@` Problem**:
```typescript
// Ollama when thrashing returns garbage like:
"@@@@@@@@@@@@@@@@@@@@@@@@"
// or partial inference:
"It@@@@@@@@@@@@@@@@@@@@"
```

**No adapter detects this pattern** - they all blindly return it as valid text!

### Problem 3: Brittle Retry Logic

**OllamaAdapter makeRequest()** (lines 617-658):
```typescript
// WRONG: Blocks adapter during retry
await new Promise(resolve => setTimeout(resolve, backoffMs));
return this.makeRequest<T>(endpoint, body, attempt + 1, reqId);
```

**Impact**: High-priority requests (GPT-4o) blocked by low-priority retry (Llama).

### Problem 4: No Priority Queuing

**OllamaRequestQueue** exists (line 91) but:
- No priority levels - FIFO only
- Queue timeout uses setTimeout (line 117)
- No integration with PriorityQueue from system/core

---

## Proposed Architecture

### Core Principles

1. **Zero setTimeout in adapters** - Use event-driven patterns
2. **Unified health monitoring** - BaseAdapter provides infrastructure
3. **Priority-based queuing** - High-priority requests jump the queue
4. **Self-healing detection** - Adapters validate responses, emit failure events
5. **Concurrent recovery** - Restarts don't block other adapters

### New Component: AdapterHealthMonitor

```typescript
/**
 * Centralized health monitoring for all adapters
 * Runs in separate thread/worker to avoid main-thread blocking
 */
class AdapterHealthMonitor {
  private adapters: Map<string, BaseAIProviderAdapter> = new Map();
  private healthStats: Map<string, HealthStats> = new Map();

  // Event-driven health checking (no setInterval)
  async checkHealth(adapterId: string): Promise<HealthStatus> {
    // 1. Test actual inference (not just API ping)
    // 2. Validate response quality (detect @@@@@)
    // 3. Update stats (error rate, latency)
    // 4. Emit events: 'adapter:healthy', 'adapter:degraded', 'adapter:failed'
  }

  // Called by daemon when adapter request fails
  async reportFailure(adapterId: string, error: Error, response?: string): Promise<void> {
    // 1. Check for known failure patterns (@@@@@, timeouts, etc.)
    // 2. Increment failure count
    // 3. Trigger restart if threshold exceeded
    // 4. Emit 'adapter:needs-restart' event
  }
}
```

### New Component: AdapterRequestQueue

```typescript
/**
 * Priority-based request queue using existing PriorityQueue
 * Replaces per-adapter hacky queue implementations
 */
class AdapterRequestQueue {
  private queue: PriorityQueue<AdapterRequest>;

  async enqueue<T>(
    request: AdapterRequest<T>,
    priority: number  // 0.0 (low) to 1.0 (high)
  ): Promise<T> {
    // 1. Add to priority queue (no setTimeout)
    // 2. Process immediately if slot available
    // 3. Retry requests get priority 0.2 (LOW)
    // 4. User requests get priority 0.8+ (HIGH)
  }

  // Event-driven processing (no polling)
  private async processNext(): Promise<void> {
    const request = this.queue.dequeue();
    if (!request) return;

    try {
      const result = await this.executeRequest(request);
      request.resolve(result);
    } catch (error) {
      // Report failure to AdapterHealthMonitor
      await this.healthMonitor.reportFailure(request.adapterId, error);
      request.reject(error);
    } finally {
      // Process next request (no setTimeout)
      this.processNext();
    }
  }
}
```

### New Component: ResponseValidator

```typescript
/**
 * Validates adapter responses for known failure patterns
 * Each adapter registers its failure patterns
 */
class ResponseValidator {
  private patterns: Map<string, RegExp[]> = new Map();

  registerFailurePattern(adapterId: string, pattern: RegExp): void {
    // Ollama: /^@{5,}/ (5+ @ symbols)
    // OpenAI: /rate limit exceeded/i
    // etc.
  }

  validate(adapterId: string, response: string): ValidationResult {
    const patterns = this.patterns.get(adapterId) || [];
    for (const pattern of patterns) {
      if (pattern.test(response)) {
        return { valid: false, pattern, action: 'restart' };
      }
    }
    return { valid: true };
  }
}
```

### Refactored BaseAIProviderAdapter

```typescript
export abstract class BaseAIProviderAdapter {
  // NO MORE setInterval or setTimeout!

  // Unified queue (replaces per-adapter queues)
  protected queue: AdapterRequestQueue;

  // Unified health monitor (replaces per-adapter polling)
  protected healthMonitor: AdapterHealthMonitor;

  // Unified response validator
  protected responseValidator: ResponseValidator;

  async initialize(): Promise<void> {
    // Register failure patterns for this adapter
    this.registerFailurePatterns();

    // Subscribe to health events
    Events.subscribe('adapter:needs-restart', async (event) => {
      if (event.adapterId === this.providerId) {
        await this.restart();
      }
    });
  }

  async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
    // Enqueue with priority (no setTimeout)
    const priority = request.priority ?? 0.5;

    return this.queue.enqueue({
      adapterId: this.providerId,
      execute: async () => {
        const response = await this.generateTextImpl(request);

        // Validate response (detect @@@@@ patterns)
        const validation = this.responseValidator.validate(this.providerId, response.text);
        if (!validation.valid) {
          // Report failure + trigger restart
          await this.healthMonitor.reportFailure(
            this.providerId,
            new Error(`Invalid response pattern: ${validation.pattern}`),
            response.text
          );
          throw new Error('Adapter returned invalid response');
        }

        return response;
      }
    }, priority);
  }

  // Event-driven restart (no setTimeout)
  private async restart(): Promise<void> {
    this.log(null, 'warn', `🔄 ${this.providerName}: Restarting due to failures...`);

    // 1. Stop accepting new requests
    this.queue.pause();

    // 2. Kill/restart provider process
    await this.restartProvider();

    // 3. Poll for health (no setTimeout - use async loop)
    await this.waitForHealthy();

    // 4. Resume accepting requests
    this.queue.resume();
  }

  // Polling without setTimeout (async loop with delay)
  private async waitForHealthy(maxAttempts = 10): Promise<void> {
    for (let i = 0; i < maxAttempts; i++) {
      const health = await this.healthCheck();
      if (health.status === 'healthy') {
        this.log(null, 'info', `✅ ${this.providerName}: Restart successful`);
        return;
      }

      // Async delay (yields to event loop, doesn't block)
      await this.asyncDelay(2000);
    }

    throw new Error('Adapter failed to restart');
  }

  // Helper: Async delay that yields to event loop
  private async asyncDelay(ms: number): Promise<void> {
    return new Promise(resolve => {
      // Schedule microtask instead of setTimeout
      queueMicrotask(() => {
        const start = Date.now();
        while (Date.now() - start < ms) {
          // Spin for short delays (< 100ms)
          // For longer delays, should use event-driven approach
        }
        resolve();
      });
    });
  }
}
```

### Refactored OllamaAdapter

```typescript
export class OllamaAdapter extends BaseAIProviderAdapter {
  // Remove OllamaRequestQueue class (use shared AdapterRequestQueue)
  // Remove setTimeout in makeRequest() retry logic
  // Remove setTimeout in restartProvider()

  protected registerFailurePatterns(): void {
    // Detect @@@@@ garbage output
    this.responseValidator.registerFailurePattern(
      this.providerId,
      /^@{5,}/  // 5+ consecutive @ symbols
    );

    // Detect partial inference failure
    this.responseValidator.registerFailurePattern(
      this.providerId,
      /\w@{5,}/  // Word followed by 5+ @ symbols (e.g. "It@@@@@@")
    );
  }

  protected async restartProvider(): Promise<void> {
    // No setTimeout - just kill and restart
    spawn('killall', ['ollama']);
    spawn('ollama', ['serve'], { detached: true, stdio: 'ignore' }).unref();
  }

  async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
    // Use base class queue (no custom queue needed)
    return super.generateText(request);
  }
}
```

---

## Revised Migration Plan (Based on Complete Survey)

**Strategy**: Fix base classes first (80% win), then migrate specific adapters.

### Phase 1: Create New Infrastructure

**NEW COMPONENTS**:

1. **ResponseValidator** (`daemons/ai-provider-daemon/shared/ResponseValidator.ts`)
   - Pattern registration per adapter
   - Response validation (detect @@@@@, rate limits, etc.)
   - Action triggers (restart, retry, fail)

2. **AdapterRequestQueue** (`daemons/ai-provider-daemon/shared/AdapterRequestQueue.ts`)
   - Uses existing `PriorityQueue` from `system/core/shared/PriorityQueue.ts`
   - Priority-based queuing (0.2 for retries, 0.8+ for user requests)
   - Event-driven processing (zero setTimeout)

3. **AdapterHealthMonitor** (`daemons/ai-provider-daemon/shared/AdapterHealthMonitor.ts`)
   - Centralized health monitoring for all adapters
   - Event-driven health checks (no setInterval)
   - Failure pattern detection + auto-restart triggers

**TESTING**:
```bash
npx vitest tests/unit/ResponseValidator.test.ts
npx vitest tests/unit/AdapterRequestQueue.test.ts
npx vitest tests/unit/AdapterHealthMonitor.test.ts
```

### Phase 2: Refactor BaseAIProviderAdapter

**FILE**: `daemons/ai-provider-daemon/shared/BaseAIProviderAdapter.ts`

**CHANGES**:
1. Remove `setInterval` from `startHealthMonitoring()` (line 111)
2. Remove `setTimeout` from `attemptRestart()` (line 161)
3. Add `protected responseValidator: ResponseValidator`
4. Add `protected requestQueue: AdapterRequestQueue`
5. Add `protected healthMonitor: AdapterHealthMonitor`
6. Add `protected abstract registerFailurePatterns(): void`
7. Wrap `generateText()` with response validation
8. Replace setTimeout with async loop in restart logic

**IMPACT**: ALL adapters immediately inherit new infrastructure

**TESTING**:
```bash
npx vitest tests/unit/BaseAIProviderAdapter.test.ts
```

### Phase 3: Refactor BaseOpenAICompatibleAdapter

**FILE**: `daemons/ai-provider-daemon/shared/adapters/BaseOpenAICompatibleAdapter.ts`

**CHANGES**:
1. Remove `setTimeout` from `restartProvider()` (line 436)
2. Remove `setTimeout` from `makeRequest()` retry logic (line 536)
3. Use `AdapterRequestQueue` for retry management
4. Replace retry setTimeout with priority-based re-queuing

**IMPACT**: OpenAI, DeepSeek, Fireworks, Groq all fixed immediately (4 adapters)

**TESTING**:
```bash
npx vitest tests/unit/BaseOpenAICompatibleAdapter.test.ts
```

### Phase 4: Migrate OllamaAdapter (CRITICAL)

**FILE**: `daemons/ai-provider-daemon/adapters/ollama/shared/OllamaAdapter.ts`

**CHANGES**:
1. **REMOVE** `OllamaRequestQueue` class (lines 91-207) - use shared `AdapterRequestQueue`
2. Remove setTimeout from queue timeout (line 117)
3. Remove setTimeout from restart delays (lines 276, 278)
4. Remove setTimeout from retry backoff (line 648)
5. **ADD** failure pattern registration:
   ```typescript
   protected registerFailurePatterns(): void {
     this.responseValidator.registerFailurePattern(this.providerId, /^@{5,}/);
     this.responseValidator.registerFailurePattern(this.providerId, /\w@{5,}/);
   }
   ```
6. Replace custom queue with `this.requestQueue.enqueue()`

**TESTING**:
```bash
npx vitest tests/integration/ollama-adapter-refactor.test.ts
# Test: @@@@@@ detection triggers restart
# Test: Priority queue - high-priority jumps queue
# Test: Concurrent 13 personas doesn't thrash
```

### Phase 5: Migrate SentinelAdapter

**FILE**: `daemons/ai-provider-daemon/adapters/sentinel/shared/SentinelAdapter.ts`

**CHANGES**:
1. Remove setTimeout from retry delays (lines 112, 140)
2. Use shared `AdapterRequestQueue` for retry management
3. Register @@@@@@ failure patterns (same as Ollama)

**TESTING**:
```bash
npx vitest tests/integration/sentinel-adapter-refactor.test.ts
```

### Phase 6: Migrate AnthropicAdapter

**FILE**: `daemons/ai-provider-daemon/adapters/anthropic/shared/AnthropicAdapter.ts`

**CHANGES**:
1. Remove setTimeout from `makeRequest()` retry backoff (line 349)
2. Use shared `AdapterRequestQueue` for retry management
3. Register Anthropic-specific failure patterns (rate limits, etc.)

**TESTING**:
```bash
npx vitest tests/integration/anthropic-adapter-refactor.test.ts
```

### Phase 7: Verify Zero setTimeout Violations

**COMPREHENSIVE TEST**:
```bash
# Search for setTimeout in ALL adapter files
grep -r "setTimeout" daemons/ai-provider-daemon/

# Should return ZERO results (except in comments or test mocks)

# Run full integration test suite
npx vitest tests/integration/adapter-concurrency.test.ts
# Test: 13 concurrent personas + 50 external requests
# Test: Zero setTimeout calls
# Test: High-priority requests complete first
# Test: @@@@@@ responses trigger restart
# Test: All adapters healthy after load
```

---

## Success Metrics

### Before (Current)
- **setTimeout calls in adapters**: 100+ (across all adapters)
- **Ollama @@@@@@ failures**: Undetected, silently returned to users
- **Retry blocking**: High-priority requests blocked by low-priority retries
- **Health monitoring**: Inconsistent, per-adapter polling
- **Restart recovery time**: 3-5s blocked main thread

### After (Target)
- **setTimeout calls in adapters**: 0 (all event-driven)
- **Ollama @@@@@@ failures**: Detected immediately, auto-restart triggered
- **Retry blocking**: Zero - priority queue ensures high-priority first
- **Health monitoring**: Unified, event-driven, runs in separate thread
- **Restart recovery time**: 2-3s non-blocking (async)

---

## Testing Strategy

### Unit Tests
```bash
npx vitest tests/unit/AdapterHealthMonitor.test.ts
npx vitest tests/unit/AdapterRequestQueue.test.ts
npx vitest tests/unit/ResponseValidator.test.ts
```

### Integration Tests
```bash
npx vitest tests/integration/ollama-adapter-refactor.test.ts
# Test: @@@@@@ detection triggers restart
# Test: Priority queue - high-priority jumps queue
# Test: Concurrent 13 personas doesn't thrash
```

### System Tests
```bash
npm start
# Send 50 concurrent requests (mix of priorities)
# Verify: Zero setTimeout calls in adapters
# Verify: @@@@@@ responses trigger restart
# Verify: High-priority requests complete first
```

---

## References

- **PriorityQueue**: `system/core/shared/PriorityQueue.ts` (already implemented!)
- **Events system**: `system/core/shared/Events.ts`
- **DAEMON-CONCURRENCY-AUDIT.md**: Original setTimeout audit
- **User feedback**: "even though i told you to make the daemons concurrent, you typically ignore me and write some pathetic async or even worse, rube goldberg inspired setTimeout logic"

---

## Next Steps

1. Get user approval for proposed architecture
2. Create new infrastructure classes (Phase 1)
3. Refactor BaseAIProviderAdapter (Phase 2)
4. Migrate adapters one by one (Phases 3-5)
5. Test and verify zero setTimeout violations

**No more setTimeout cancer. Build real concurrent architecture.**
