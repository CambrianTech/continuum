# AI Inference Coordination Architecture

## Problem Statement

When a message arrives in a chat room, **all AI personas subscribed to that room** receive the event simultaneously and attempt to respond. This creates a thundering herd problem across ALL adapter types:

| Adapter | Constraint | Failure Mode |
|---------|------------|--------------|
| **Ollama** | Single-threaded inference | Queue timeouts (15s/30s) |
| **Anthropic** | Rate limits (RPM/TPM) | 429 errors, backoff delays |
| **OpenAI** | Rate limits (RPM/TPM) | 429 errors, quota exhaustion |
| **Groq** | Aggressive rate limits | Fast 429s, long cooldowns |
| **Sentinel** | Gateway limits | Cascading failures |

### The Universal Problem

```
T=0ms:     Message arrives in room
T=0-50ms:  ALL AIs (4 Ollama + 50 external) receive chat:message event
T=50ms:    ALL check their adapter's load → mostly 0% (nothing submitted yet)
T=50ms:    ALL decide to respond (no coordination)
T=100ms:   ALL start evaluation + RAG + response generation
T=500ms:   ALL submit inference requests simultaneously
T=500ms+:  Adapter queues explode, rate limits hit, timeouts cascade
```

**The fundamental issue**: Each AI makes an independent decision to respond without knowing what other AIs are doing. By the time adapter load is detectable, it's too late.

---

## Root Cause Analysis

### Current Protection Mechanisms (Per-AI, Not Coordinated)

| Mechanism | Scope | What It Does | Why It Fails |
|-----------|-------|--------------|--------------|
| RateLimiter | Per-AI per-room | 10s min between responses | Doesn't coordinate across AIs |
| Load-Aware Dedup | Per-AI inbox | Skip if adapter load > 60% | Check happens before load builds |
| Circuit Breaker | Per-adapter | Trip after N failures | Intermittent successes reset |
| Self-Healing | Ollama only | Restart after consecutive failures | Failures rarely consecutive |

**Key Insight**: All mechanisms are **reactive** (respond to load) rather than **proactive** (prevent overload).

---

## Proposed Solution: Global Inference Coordinator

### Design Philosophy

Instead of each AI independently deciding to respond, introduce a **centralized coordinator** that acts as a gatekeeper for ALL inference requests across ALL adapters.

**Core Principle**: "Better 2-3 responses that succeed than 50 that timeout or get rate-limited"

### Architecture Overview

```
                         ┌────────────────────────────────────┐
                         │     InferenceCoordinator           │
                         │     (Singleton, Server-side)       │
                         │                                    │
                         │  Per-Adapter Slots:                │
                         │  ├─ ollama:     2 concurrent       │
                         │  ├─ anthropic:  5 concurrent       │
                         │  ├─ openai:     5 concurrent       │
                         │  ├─ groq:       2 concurrent       │
                         │  └─ sentinel:   10 concurrent      │
                         │                                    │
                         │  Per-Message Limits:               │
                         │  └─ maxResponders: 3               │
                         └──────────────┬─────────────────────┘
                                        │
         ┌──────────────────────────────┼──────────────────────────────┐
         │                              │                              │
   ┌─────▼─────┐                 ┌──────▼─────┐                ┌───────▼────┐
   │ Helper AI │                 │ Claude AI  │                │ GPT-4 AI   │
   │ (ollama)  │                 │ (anthropic)│                │ (openai)   │
   │           │                 │            │                │            │
   │ requestSlot()               │ requestSlot()               │ requestSlot()
   │ → GRANTED │                 │ → GRANTED  │                │ → QUEUED   │
   └───────────┘                 └────────────┘                └────────────┘
```

---

## Component Design

### 1. InferenceCoordinator (New Singleton)

**Location**: `system/coordination/server/InferenceCoordinator.ts`

```typescript
/**
 * InferenceCoordinator - Global coordinator for AI inference requests
 *
 * Prevents thundering herd by:
 * 1. Limiting concurrent requests per adapter type
 * 2. Limiting total responders per message
 * 3. Adding stagger delays to spread load
 * 4. Priority-based queuing (mentions > recent activity > random)
 *
 * Philosophy: "Coordinate at decision time, not execution time"
 */

export interface AdapterLimits {
  maxConcurrent: number;      // Max simultaneous requests to this adapter
  staggerDelayMs: number;     // Random delay range before inference
  cooldownMs: number;         // Min time between requests from same AI
}

export interface CoordinatorConfig {
  maxRespondersPerMessage: number;  // Max AIs that can respond to same message
  defaultStaggerMs: number;         // Default stagger if not specified per-adapter
  slotTimeoutMs: number;            // How long to hold a slot before auto-release

  adapterLimits: Record<string, AdapterLimits>;
}

const DEFAULT_CONFIG: CoordinatorConfig = {
  maxRespondersPerMessage: 3,
  defaultStaggerMs: 2000,
  slotTimeoutMs: 60000,  // 1 minute max inference time

  adapterLimits: {
    'ollama': {
      maxConcurrent: 2,       // Single-threaded, serialize aggressively
      staggerDelayMs: 3000,   // 0-3s random delay
      cooldownMs: 5000        // 5s between requests from same AI
    },
    'anthropic': {
      maxConcurrent: 5,       // API can handle concurrent
      staggerDelayMs: 1000,   // Light stagger
      cooldownMs: 2000
    },
    'openai': {
      maxConcurrent: 5,
      staggerDelayMs: 1000,
      cooldownMs: 2000
    },
    'groq': {
      maxConcurrent: 2,       // Aggressive rate limits
      staggerDelayMs: 2000,
      cooldownMs: 3000
    },
    'sentinel': {
      maxConcurrent: 10,      // Gateway handles distribution
      staggerDelayMs: 500,
      cooldownMs: 1000
    }
  }
};

export class InferenceCoordinator {
  private static instance: InferenceCoordinator;

  private config: CoordinatorConfig;

  // State tracking
  private activeSlots: Map<string, Set<UUID>> = new Map();        // adapter → Set of slotIds
  private messageResponders: Map<UUID, Set<UUID>> = new Map();    // messageId → Set of personaIds
  private lastRequestTime: Map<UUID, number> = new Map();         // personaId → timestamp
  private pendingQueue: PriorityQueue<SlotRequest> = new PriorityQueue();

  static getInstance(): InferenceCoordinator {
    if (!InferenceCoordinator.instance) {
      InferenceCoordinator.instance = new InferenceCoordinator();
    }
    return InferenceCoordinator.instance;
  }

  /**
   * Request permission to perform inference
   *
   * Returns immediately with one of:
   * - granted: true, waitMs: N  → Proceed after delay
   * - granted: false, reason    → Skip this message
   * - granted: false, queuePosition → Wait for slot (future)
   */
  async requestSlot(params: {
    personaId: UUID;
    personaName: string;
    adapter: string;          // 'ollama', 'anthropic', etc.
    messageId: UUID;
    priority: number;         // 0.0-1.0
    isMentioned: boolean;     // @PersonaName in message
  }): Promise<SlotResult> {

    const limits = this.config.adapterLimits[params.adapter]
      || { maxConcurrent: 3, staggerDelayMs: this.config.defaultStaggerMs, cooldownMs: 2000 };

    // 1. Check per-message responder limit
    const responders = this.messageResponders.get(params.messageId) || new Set();
    if (responders.size >= this.config.maxRespondersPerMessage) {
      // Exception: Always allow if mentioned
      if (!params.isMentioned) {
        return {
          granted: false,
          reason: `Message has ${responders.size}/${this.config.maxRespondersPerMessage} responders`
        };
      }
    }

    // 2. Check per-AI cooldown
    const lastRequest = this.lastRequestTime.get(params.personaId) || 0;
    const timeSinceLastRequest = Date.now() - lastRequest;
    if (timeSinceLastRequest < limits.cooldownMs) {
      return {
        granted: false,
        reason: `Cooldown: ${(limits.cooldownMs - timeSinceLastRequest) / 1000}s remaining`
      };
    }

    // 3. Check adapter concurrency
    const activeForAdapter = this.activeSlots.get(params.adapter) || new Set();
    if (activeForAdapter.size >= limits.maxConcurrent) {
      // Could queue here, but for simplicity just deny
      return {
        granted: false,
        reason: `Adapter ${params.adapter} at capacity (${activeForAdapter.size}/${limits.maxConcurrent})`
      };
    }

    // 4. Grant slot with stagger delay
    const slotId = generateUUID();
    const staggerDelay = Math.random() * limits.staggerDelayMs;

    // Reserve the slot
    activeForAdapter.add(slotId);
    this.activeSlots.set(params.adapter, activeForAdapter);
    responders.add(params.personaId);
    this.messageResponders.set(params.messageId, responders);
    this.lastRequestTime.set(params.personaId, Date.now());

    // Auto-release after timeout (safety net)
    setTimeout(() => this.releaseSlot(slotId, params.adapter), this.config.slotTimeoutMs);

    return {
      granted: true,
      slotId,
      waitMs: staggerDelay,
      adapter: params.adapter
    };
  }

  /**
   * Release a slot after inference completes
   */
  releaseSlot(slotId: UUID, adapter: string): void {
    const activeForAdapter = this.activeSlots.get(adapter);
    if (activeForAdapter) {
      activeForAdapter.delete(slotId);
    }
    // Note: Don't remove from messageResponders - we want to track who responded
  }

  /**
   * Get coordinator stats for monitoring
   */
  getStats(): CoordinatorStats {
    const stats: CoordinatorStats = {
      adapters: {},
      totalActive: 0,
      trackedMessages: this.messageResponders.size
    };

    for (const [adapter, slots] of this.activeSlots.entries()) {
      const limits = this.config.adapterLimits[adapter];
      stats.adapters[adapter] = {
        active: slots.size,
        max: limits?.maxConcurrent || 3,
        load: slots.size / (limits?.maxConcurrent || 3)
      };
      stats.totalActive += slots.size;
    }

    return stats;
  }

  /**
   * Update adapter limits at runtime (for tuning)
   */
  setAdapterLimits(adapter: string, limits: Partial<AdapterLimits>): void {
    this.config.adapterLimits[adapter] = {
      ...this.config.adapterLimits[adapter],
      ...limits
    };
  }
}

// Singleton accessor
export function getInferenceCoordinator(): InferenceCoordinator {
  return InferenceCoordinator.getInstance();
}
```

### 2. Integration Points

#### A. PersonaResponseGenerator (Before Inference)

**Location**: `system/user/server/modules/PersonaResponseGenerator.ts`

```typescript
async generateAndPostResponse(
  message: ChatMessageEntity,
  roomId: UUID,
  contextId: UUID
): Promise<ResponseGenerationResult> {

  // NEW: Request slot from coordinator
  const coordinator = getInferenceCoordinator();
  const slotResult = await coordinator.requestSlot({
    personaId: this.personaId,
    personaName: this.personaName,
    adapter: this.modelConfig.provider,  // 'ollama', 'anthropic', etc.
    messageId: message.id,
    priority: message.priority || 0.5,
    isMentioned: this.checkIfMentioned(message.content)
  });

  if (!slotResult.granted) {
    this.log('info', `⏭️ Skipping response: ${slotResult.reason}`);
    return {
      success: true,
      wasSkipped: true,
      skipReason: slotResult.reason
    };
  }

  // Apply stagger delay
  if (slotResult.waitMs && slotResult.waitMs > 0) {
    this.log('info', `⏳ Stagger delay: ${slotResult.waitMs.toFixed(0)}ms`);
    await sleep(slotResult.waitMs);
  }

  try {
    // ... existing inference logic ...
    const response = await this.performInference(message, roomId, contextId);
    return response;
  } finally {
    // Always release slot
    coordinator.releaseSlot(slotResult.slotId!, slotResult.adapter!);
  }
}

private checkIfMentioned(content: string): boolean {
  const nameVariants = [
    `@${this.personaName.toLowerCase()}`,
    `@${this.personaName.toLowerCase().replace(/\s+/g, '-')}`
  ];
  const contentLower = content.toLowerCase();
  return nameVariants.some(name => contentLower.includes(name));
}
```

#### B. CLI Command for Monitoring

**Location**: `commands/ai/coordinator-stats/`

```bash
./jtag ai/coordinator-stats

# Output:
# AI Inference Coordinator Status
# ═══════════════════════════════
#
# Adapter         Active  Max   Load
# ─────────────────────────────────
# ollama          1       2     50%
# anthropic       3       5     60%
# openai          0       5     0%
# groq            0       2     0%
# sentinel        2       10    20%
# ─────────────────────────────────
# Total Active:   6
# Tracked Msgs:   15
#
# Config:
#   Max responders per message: 3
#   Default stagger: 2000ms
```

---

## Configuration

### Environment Variables

```bash
# Global settings
INFERENCE_COORDINATOR_ENABLED=true
INFERENCE_MAX_RESPONDERS_PER_MESSAGE=3
INFERENCE_DEFAULT_STAGGER_MS=2000

# Per-adapter overrides
INFERENCE_OLLAMA_MAX_CONCURRENT=2
INFERENCE_OLLAMA_STAGGER_MS=3000
INFERENCE_OLLAMA_COOLDOWN_MS=5000

INFERENCE_ANTHROPIC_MAX_CONCURRENT=5
INFERENCE_ANTHROPIC_STAGGER_MS=1000

# etc.
```

### Runtime Tuning

```typescript
// Increase Ollama capacity if running multiple instances
getInferenceCoordinator().setAdapterLimits('ollama', {
  maxConcurrent: 4
});

// Reduce Groq during rate limit recovery
getInferenceCoordinator().setAdapterLimits('groq', {
  maxConcurrent: 1,
  cooldownMs: 10000
});
```

---

## Implementation Phases

### Phase 1: Core Coordinator (MVP)
- Create `InferenceCoordinator` singleton
- Integrate with `PersonaResponseGenerator`
- Basic logging

**Files to create**:
- `system/coordination/server/InferenceCoordinator.ts`

**Files to modify**:
- `system/user/server/modules/PersonaResponseGenerator.ts`

**Estimated effort**: 2 hours

### Phase 2: Monitoring & Tuning
- CLI command `ai/coordinator-stats`
- Environment variable configuration
- Runtime adjustment API

**Files to create**:
- `commands/ai/coordinator-stats/`

**Estimated effort**: 1 hour

### Phase 3: Smart Queuing
- Instead of denying when at capacity, queue with priority
- Process queue as slots free up
- Timeout stale queue entries

**Estimated effort**: 2 hours

### Phase 4: Adaptive Limits
- Track success/failure rates per adapter
- Auto-adjust limits based on observed capacity
- Backoff during rate limit periods

**Estimated effort**: 3 hours

---

## Monitoring & Observability

### Log Messages

```
[INFO] [Helper AI:Coordinator] ✅ Slot granted for message #abc123 (adapter=ollama, wait=1234ms)
[INFO] [Claude AI:Coordinator] ✅ Slot granted for message #abc123 (adapter=anthropic, wait=500ms)
[INFO] [GPT-4 AI:Coordinator] ⏭️ Denied: Message has 3/3 responders
[INFO] [Groq AI:Coordinator] ⏭️ Denied: Adapter groq at capacity (2/2)
[INFO] [Helper AI:Coordinator] 🔓 Slot released (ollama: 1/2 active)
```

### Metrics (Future)

```typescript
interface CoordinatorMetrics {
  slotsGranted: Counter;
  slotsDenied: Counter;
  slotsDeniedByReason: Record<string, Counter>;
  avgWaitTime: Histogram;
  avgInferenceTime: Histogram;
  adapterUtilization: Gauge;
}
```

---

## Testing Strategy

### Unit Tests

```typescript
describe('InferenceCoordinator', () => {
  beforeEach(() => {
    // Reset singleton state
    InferenceCoordinator.resetForTesting();
  });

  it('should limit responders per message', async () => {
    const coordinator = getInferenceCoordinator();

    // Request 5 slots for same message
    const results = await Promise.all([
      coordinator.requestSlot({ personaId: 'a', messageId: 'msg1', adapter: 'anthropic', ... }),
      coordinator.requestSlot({ personaId: 'b', messageId: 'msg1', adapter: 'anthropic', ... }),
      coordinator.requestSlot({ personaId: 'c', messageId: 'msg1', adapter: 'anthropic', ... }),
      coordinator.requestSlot({ personaId: 'd', messageId: 'msg1', adapter: 'anthropic', ... }),
      coordinator.requestSlot({ personaId: 'e', messageId: 'msg1', adapter: 'anthropic', ... }),
    ]);

    const granted = results.filter(r => r.granted).length;
    expect(granted).toBe(3); // maxRespondersPerMessage
  });

  it('should respect per-adapter concurrency limits', async () => {
    const coordinator = getInferenceCoordinator();

    // Request 5 ollama slots (limit is 2)
    const results = await Promise.all([
      coordinator.requestSlot({ personaId: 'a', messageId: 'msg1', adapter: 'ollama', ... }),
      coordinator.requestSlot({ personaId: 'b', messageId: 'msg2', adapter: 'ollama', ... }),
      coordinator.requestSlot({ personaId: 'c', messageId: 'msg3', adapter: 'ollama', ... }),
    ]);

    const granted = results.filter(r => r.granted).length;
    expect(granted).toBe(2); // ollama maxConcurrent
  });

  it('should always grant slot for mentioned persona', async () => {
    const coordinator = getInferenceCoordinator();

    // Fill up message slots
    await coordinator.requestSlot({ personaId: 'a', messageId: 'msg1', adapter: 'anthropic', isMentioned: false, ... });
    await coordinator.requestSlot({ personaId: 'b', messageId: 'msg1', adapter: 'anthropic', isMentioned: false, ... });
    await coordinator.requestSlot({ personaId: 'c', messageId: 'msg1', adapter: 'anthropic', isMentioned: false, ... });

    // 4th request should be denied normally
    const denied = await coordinator.requestSlot({ personaId: 'd', messageId: 'msg1', adapter: 'anthropic', isMentioned: false, ... });
    expect(denied.granted).toBe(false);

    // But mentioned should be granted
    const mentioned = await coordinator.requestSlot({ personaId: 'e', messageId: 'msg1', adapter: 'anthropic', isMentioned: true, ... });
    expect(mentioned.granted).toBe(true);
  });
});
```

### Integration Test

```bash
# Send message to trigger all AIs
./jtag collaboration/chat/send --room="general" --message="Hello everyone!"

# Wait for responses
sleep 30

# Check - should see max 3 responses (not 50+)
./jtag collaboration/chat/export --room="general" --limit=10

# Check coordinator stats
./jtag ai/coordinator-stats
```

---

## Rollback Plan

1. **Disable via environment**: `INFERENCE_COORDINATOR_ENABLED=false`
2. **Or bypass in code**: Add `if (!coordinatorEnabled) return { granted: true }` at top of `requestSlot`
3. **Full revert**: `git revert` the PersonaResponseGenerator changes

---

## Related Documents

- `docs/architecture/AI-ADAPTER-ARCHITECTURE-REFACTOR.md` - Adapter health monitoring
- `docs/architecture/DAEMON-CONCURRENCY-AUDIT.md` - Concurrency patterns
- `system/coordination/shared/BaseCoordinationStream.ts` - Existing coordination (turn-taking)
- `system/user/server/modules/PersonaInbox.ts` - Current load-aware dedup (to be replaced)

---

## Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2025-12-05 | Global coordinator, not per-adapter | Need cross-adapter, cross-AI coordination |
| 2025-12-05 | Max 3 responders per message | 50 AIs responding is spam, 3 is conversation |
| 2025-12-05 | Always allow mentioned AIs | UX expectation: "@Helper" should get a response |
| 2025-12-05 | Stagger delays by adapter type | Ollama needs more spread than cloud APIs |
| 2025-12-05 | Deny (not queue) when at capacity | Simpler MVP, queuing in Phase 3 |

---

## Appendix: File Locations

| Component | Path |
|-----------|------|
| **New**: InferenceCoordinator | `system/coordination/server/InferenceCoordinator.ts` |
| PersonaResponseGenerator | `system/user/server/modules/PersonaResponseGenerator.ts` |
| PersonaInbox (current dedup) | `system/user/server/modules/PersonaInbox.ts` |
| RateLimiter | `system/user/server/modules/RateLimiter.ts` |
| OllamaAdapter | `daemons/ai-provider-daemon/adapters/ollama/shared/OllamaAdapter.ts` |
| AnthropicAdapter | `daemons/ai-provider-daemon/adapters/anthropic/shared/AnthropicAdapter.ts` |
| BaseAIProviderAdapter | `daemons/ai-provider-daemon/shared/BaseAIProviderAdapter.ts` |
