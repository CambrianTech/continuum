# Candle Native Rust Inference - Pitfalls & Lessons Learned

## Overview

This document captures pitfalls discovered during debugging of the Candle native Rust inference system. These issues caused Helper AI (and other local-model personas) to fail silently or time out.

---

## Pitfall 1: Provider Aliasing Only Works with `preferredProvider`

**Problem**: Code like SignalDetector uses `ai/generate` with a model name (`llama3.2:1b`) but NO `preferredProvider`. The aliasing logic only checked `preferredProvider`, so these requests went to OllamaAdapter instead of Candle.

**Symptom**: OllamaAdapter queue timeouts, error: "Request timed out in queue after 45022ms"

**Fix**: Added `isLocalModel()` check in `AIProviderDaemon.selectAdapter()`:
```typescript
if (model && this.isLocalModel(model)) {
  return candleAdapter;
}
```

**File**: `daemons/ai-provider-daemon/shared/AIProviderDaemon.ts`

---

## Pitfall 2: Candle Priority Too High (Queue Bottleneck)

**Problem**: Candle had priority 105 (highest), so ALL requests without `preferredProvider` went to Candle, including cloud provider personas (DeepSeek, Groq, etc.). This caused massive queue buildup.

**Symptom**: 25+ items in queue, 40+ second delays, timeouts

**Fix**: Exclude Candle from default adapter selection - only use for explicit local inference:
```typescript
const registrations = Array.from(this.adapters.values())
  .filter(reg => reg.enabled && reg.providerId !== 'candle')
  .sort((a, b) => b.priority - a.priority);
```

---

## Pitfall 3: PersonaUser Timeout Includes Queue Wait Time

**Problem**: PersonaUser starts a 180-second timeout when PHASE 3.3 begins (calling generateText), but the request may wait in queue for 100+ seconds before execution starts.

**Symptom**: Request times out even though Candle generation succeeds. Timeline:
- 13:25:08: PHASE 3.3 starts (timeout timer begins)
- 13:26:55: Request actually starts executing (after 107s in queue)
- 13:28:08: 180s timeout fires (only 73s of actual execution)

**Impact**: With slow inference (~140s), queued requests always time out.

**Potential Fixes**:
1. Increase timeout to 300s+ to account for queue wait
2. Start timeout when execution begins, not when enqueued
3. Limit one inflight request per persona

---

## Pitfall 4: Race Condition on Startup

**Problem**: `PersonaUser.queueStatsProvider` called `AIProviderDaemon.getAdapter('ollama')` before the daemon was initialized, causing "AIProviderDaemon not initialized" errors.

**Fix**: Wrap in try/catch with defaults:
```typescript
this.inbox.setQueueStatsProvider(() => {
  try {
    const adapter = AIProviderDaemon.getAdapter('ollama');
    if (adapter?.getQueueStats) return adapter.getQueueStats();
  } catch {
    // AIProviderDaemon not initialized yet
  }
  return { queueSize: 0, activeRequests: 0, maxConcurrent: 1, load: 0.0 };
});
```

---

## Pitfall 5: LLMAdapter Used 'deterministic' as Model Name

**Problem**: When model was `undefined`, LLMAdapter was using the string `'deterministic'` as the model name instead of the actual model.

**Fix**: Properly cascade model selection in LLMAdapter.

---

## Pitfall 6: Rust Worker Single-Threaded Blocking

**Problem**: Rust inference worker was using synchronous mutex, blocking all requests while one was processing.

**Symptom**: Requests queued up even though CPU had capacity.

**Fix**: Use async mutex or restructure to allow concurrent processing.

---

## Debugging Tips

### Log Files to Check

1. **Cognition log**: `.continuum/personas/{persona}/logs/cognition.log`
   - PHASE 1/2/3 progress
   - Timeout errors
   - RAG context building

2. **Adapter log**: `.continuum/personas/{persona}/logs/adapters.log`
   - TRACE-1 through TRACE-7 for Candle request lifecycle
   - Request IDs for correlation
   - Actual generation times

3. **AIProviderDaemon log**: `.continuum/jtag/logs/system/daemons/AIProviderDaemonServer.log`
   - Provider routing decisions
   - Queue status
   - Generation success/failure

### Key Trace Points

Add these to track request flow:
- TRACE-POINT-A: Before message polling
- TRACE-POINT-B: Before respondToMessage call
- TRACE-POINT-D: Entered respondToMessage
- TRACE-POINT-C: After respondToMessage returned

### Queue Traces

Add to InferenceWorkerClient:
```typescript
console.log(`🔧 QUEUE-TRACE: Adding ${requestId} to queue (queueLen=${queue.length})`);
console.log(`🔧 QUEUE-TRACE: Executing ${requestId}`);
console.log(`🔧 QUEUE-TRACE: Request ${requestId} completed`);
```

---

## Performance Baseline

Current Candle inference times with Qwen2-1.5B-Instruct:
- Model load: ~35 seconds (first request)
- Model from cache: <1 second
- Generation (600-1000 char prompt, 200 tokens): ~100-140 seconds
- Total first request: ~140 seconds
- Subsequent requests: ~100 seconds

These times are slow for production use. Consider:
- GPU acceleration
- Smaller models for gating (Qwen2-0.5B)
- Request batching
