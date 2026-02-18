# Event-State Decoupling Architecture

## Problem Statement

The current system has **synchronous bidirectional coupling** between state and events:

```
State Change → Immediate Event → Handler → State Change → Event → OOM
```

This causes:
- Infinite recursion
- Memory exhaustion (3.4GB heap)
- UI freezes
- Cascade failures across WebSocket bridge

## Design Principles

1. **Eventual Consistency** - State changes propagate asynchronously, not immediately
2. **Transaction Boundaries** - Changes are atomic; events fire AFTER transaction completes
3. **Backpressure** - System can slow down when overwhelmed
4. **Cycle Detection** - Cascades are detected and broken
5. **Off-Main-Thread** - Heavy processing in Workers, main thread stays responsive

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        MAIN THREAD                               │
│                                                                  │
│  ┌──────────────┐    postMessage    ┌──────────────────────┐   │
│  │   Widgets    │ ─────────────────→│                      │   │
│  │              │                    │   StateEventBridge   │   │
│  │ state.x = y  │←─────────────────  │   (thin proxy)       │   │
│  └──────────────┘    onmessage      └──────────┬───────────┘   │
│                                                 │               │
└─────────────────────────────────────────────────│───────────────┘
                                                  │ postMessage
                                                  ▼
┌─────────────────────────────────────────────────────────────────┐
│                     EVENT COORDINATOR WORKER                     │
│                                                                  │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │                     Event Queue                             │ │
│  │  ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐                          │ │
│  │  │ E1  │ │ E2  │ │ E3  │ │ E4  │  ...                     │ │
│  │  └─────┘ └─────┘ └─────┘ └─────┘                          │ │
│  └────────────────────────────────────────────────────────────┘ │
│                          │                                       │
│                          ▼                                       │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │               Transaction Processor                         │ │
│  │                                                             │ │
│  │  - Deduplication (same event+payload within window)        │ │
│  │  - Cycle Detection (A→B→A pattern)                         │ │
│  │  - Rate Limiting (max N per event type per window)         │ │
│  │  - Backpressure (queue depth monitoring)                   │ │
│  │  - Priority Ordering (critical events first)               │ │
│  └────────────────────────────────────────────────────────────┘ │
│                          │                                       │
│                          ▼                                       │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │                  Batch Dispatcher                           │ │
│  │                                                             │ │
│  │  - Groups events by target                                  │ │
│  │  - Sends batches at controlled intervals                    │ │
│  │  - Uses requestAnimationFrame timing for UI events          │ │
│  │  - Uses requestIdleCallback for background events           │ │
│  └────────────────────────────────────────────────────────────┘ │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Core Components

### 1. StateEventBridge (Main Thread)

Thin proxy that sits between application code and the Worker.

```typescript
interface StateEventBridge {
  // State mutations go through here
  setState(domain: string, key: string, value: unknown): void;

  // Event emissions go through here
  emit(eventName: string, payload: unknown): void;

  // Subscriptions registered here
  subscribe(pattern: string, handler: Handler): Unsubscribe;

  // Transaction API
  beginTransaction(): TransactionId;
  commitTransaction(id: TransactionId): void;
  rollbackTransaction(id: TransactionId): void;
}
```

**Key Behavior:**
- Does NOT process anything itself
- Posts messages to Worker
- Receives processed batches from Worker
- Dispatches to registered handlers

### 2. EventCoordinatorWorker (Web Worker)

Heavy lifting happens here, OFF main thread.

```typescript
// Inside Worker
interface EventCoordinator {
  // Receives from main thread
  onMessage(msg: BridgeMessage): void;

  // Internal queue
  queue: PriorityQueue<QueuedEvent>;

  // Deduplication cache
  recentEvents: LRUCache<string, number>; // hash → timestamp

  // Cycle detection
  callGraph: Map<string, Set<string>>; // event → triggered events

  // Rate limiting
  rateLimits: Map<string, RateBucket>;

  // Batch and dispatch
  processBatch(): ProcessedBatch;
}
```

### 3. Transaction Model

Events emitted during a transaction are held until commit:

```typescript
interface Transaction {
  id: TransactionId;
  startTime: number;
  events: QueuedEvent[];
  stateChanges: StateChange[];
  status: 'pending' | 'committed' | 'rolled-back';
}

// Usage in application code:
const txn = bridge.beginTransaction();
try {
  state.currentRoom = newRoom;      // Queued, not fired
  state.currentTab = newTab;        // Queued, not fired
  Events.emit('room-changed', {});  // Queued, not fired
  bridge.commitTransaction(txn);    // NOW all fire, as batch
} catch (e) {
  bridge.rollbackTransaction(txn); // Discard all
}
```

### 4. Cycle Detection Algorithm

Track causal chains to detect loops:

```typescript
interface CausalChain {
  originEvent: string;
  chain: string[];  // [event1, event2, event3, ...]
  depth: number;
}

function detectCycle(newEvent: string, chain: CausalChain): boolean {
  // If newEvent already in chain, we have a cycle
  if (chain.chain.includes(newEvent)) {
    console.error(`Cycle detected: ${chain.chain.join(' → ')} → ${newEvent}`);
    return true;
  }

  // If chain too deep, assume problematic
  if (chain.depth > MAX_CHAIN_DEPTH) {
    console.error(`Chain too deep: ${chain.depth}`);
    return true;
  }

  return false;
}
```

### 5. Deduplication Strategy

Hash event+payload, skip if seen recently:

```typescript
function shouldDedupe(event: QueuedEvent): boolean {
  const hash = computeHash(event.name, event.payload);
  const lastSeen = recentEvents.get(hash);

  if (lastSeen && (Date.now() - lastSeen) < DEDUPE_WINDOW_MS) {
    return true; // Skip duplicate
  }

  recentEvents.set(hash, Date.now());
  return false;
}
```

### 6. Backpressure Mechanism

When queue grows too large, apply backpressure:

```typescript
interface BackpressureState {
  queueDepth: number;
  dropRate: number;      // 0.0 - 1.0
  throttleMs: number;    // Delay between batches
}

function computeBackpressure(depth: number): BackpressureState {
  if (depth < LOW_WATERMARK) {
    return { queueDepth: depth, dropRate: 0, throttleMs: 0 };
  }

  if (depth > HIGH_WATERMARK) {
    // Critical: drop low-priority events
    return {
      queueDepth: depth,
      dropRate: 0.5,  // Drop 50% of low-priority
      throttleMs: 100
    };
  }

  // Linear interpolation between watermarks
  const ratio = (depth - LOW_WATERMARK) / (HIGH_WATERMARK - LOW_WATERMARK);
  return {
    queueDepth: depth,
    dropRate: ratio * 0.3,
    throttleMs: ratio * 50
  };
}
```

## Event Priority Levels

```typescript
enum EventPriority {
  CRITICAL = 0,    // User input, errors - never dropped
  HIGH = 1,        // UI state changes - rarely dropped
  NORMAL = 2,      // Data updates - may be coalesced
  LOW = 3,         // Analytics, logging - may be dropped
  BACKGROUND = 4   // Housekeeping - dropped under pressure
}
```

## Wire Protocol (Main Thread ↔ Worker)

```typescript
// Main → Worker
type OutboundMessage =
  | { type: 'emit'; event: string; payload: unknown; priority: EventPriority; txnId?: string }
  | { type: 'state-change'; domain: string; key: string; value: unknown; txnId?: string }
  | { type: 'begin-txn'; txnId: string }
  | { type: 'commit-txn'; txnId: string }
  | { type: 'rollback-txn'; txnId: string }
  | { type: 'subscribe'; pattern: string; handlerId: string }
  | { type: 'unsubscribe'; handlerId: string };

// Worker → Main
type InboundMessage =
  | { type: 'dispatch'; batch: ProcessedEvent[] }
  | { type: 'state-sync'; domain: string; state: Record<string, unknown> }
  | { type: 'backpressure'; state: BackpressureState }
  | { type: 'cycle-detected'; chain: string[] }
  | { type: 'metrics'; stats: CoordinatorMetrics };
```

## Migration Strategy

### Phase 1: Instrument Current System
- Add metrics to current Events.emit()
- Track frequency, cascades, memory impact
- Identify worst offenders

### Phase 2: Build Worker Infrastructure
- Create EventCoordinatorWorker
- Create StateEventBridge proxy
- Run in parallel with current system (shadow mode)

### Phase 3: Migrate High-Volume Events
- Route data:* events through new system
- Route UI_EVENTS through new system
- Keep low-volume events on old path

### Phase 4: Full Migration
- All events through Worker
- Remove old synchronous paths
- Add transaction API to critical flows

### Phase 5: Optimize
- Tune watermarks based on real usage
- Add SharedArrayBuffer for high-frequency state (if needed)
- Profile and optimize hot paths

## Metrics & Observability

The Worker should expose:

```typescript
interface CoordinatorMetrics {
  queueDepth: number;
  eventsProcessed: number;
  eventsDropped: number;
  cyclesDetected: number;
  dedupeHits: number;
  avgBatchSize: number;
  avgLatencyMs: number;
  backpressureState: BackpressureState;
  topEventTypes: Array<{ name: string; count: number }>;
}
```

## Failure Modes & Recovery

1. **Worker Crash**
   - Main thread detects via onerror
   - Respawn Worker
   - Replay uncommitted transactions from journal

2. **Queue Overflow**
   - Drop lowest priority events
   - Log warning
   - Notify UI of degraded mode

3. **Cycle Storm**
   - Break cycle at detection point
   - Log full chain for debugging
   - Temporarily blacklist problematic event patterns

## Not Using setTimeout/setInterval

The architecture avoids these because:
- They're imprecise (minimum ~4ms, often more)
- They don't respect main thread workload
- They can stack up if processing takes too long

Instead, we use:
- **Worker message passing** - True parallelism
- **requestAnimationFrame** - Synced to display refresh for UI
- **requestIdleCallback** - Background work when idle
- **MessageChannel** - High-priority async without timer overhead

## Open Questions

1. Should state live in Worker (SharedArrayBuffer) or stay on main thread?
2. How to handle subscriptions that need DOM access?
3. Cross-tab coordination via SharedWorker or BroadcastChannel?
4. Persistence of transaction journal for crash recovery?
