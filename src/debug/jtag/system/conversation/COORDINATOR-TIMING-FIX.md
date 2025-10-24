# ThoughtStream Coordinator Timing Architecture

**Status**: 🔴 **BUG DISCOVERED** - Fix pending

**Date**: 2025-10-23

---

## Problem Statement

The ThoughtStream coordinator makes decisions too early, missing late-arriving AI evaluations.

### Evidence

From live testing (2025-10-23):
```
Thought #1: +0ms      (first AI evaluates)
Decision:   ~+2000ms  (coordinator decides - only saw 1-2 thoughts!)
Thought #4: +6475ms   (MISSED - arrived after decision)
Thought #5: +12017ms  (MISSED - way too late)
```

**Result**: Only the fastest 1-2 AIs get included in each decision, leading to:
- Together Assistant responding multiple times (separate rounds)
- High-confidence AIs being denied despite wanting to respond
- Poor conversation quality (missing diverse perspectives)

### Root Cause

```typescript
// BaseModerator.ts:268
if (elapsedMs < config.intentionWindowMs) return false;
```

**Current Logic**:
- Fixed 2-second window from FIRST thought
- Decision made regardless of who has evaluated
- Late thoughts arrive after `stream.phase = 'decided'`

**Why This Fails**:
- AI evaluation speeds vary wildly: 0-12+ seconds
- Network latency varies
- Model inference times differ (local vs API)
- System load fluctuates

---

## Solution: Adaptive Heartbeat Coordination

### Core Principle: Learn the System's Rhythm

Like a biological system adapting to its environment, the coordinator should:
1. **Measure** actual evaluation speeds
2. **Adapt** decision window to system capacity
3. **Stabilize** around optimal timing
4. **Recover** from transient slowdowns

### Architecture Components

#### 1. Heartbeat Tracker (NEW)

```typescript
interface SystemHeartbeat {
  contextId: UUID;

  // Evaluation speed statistics
  recentEvaluationTimes: number[];  // Last 20 evaluations
  avgEvaluationTime: number;        // Moving average
  p95EvaluationTime: number;        // 95th percentile
  p99EvaluationTime: number;        // 99th percentile

  // Adaptive window
  currentWindowMs: number;          // Current decision timeout
  minWindowMs: number;              // Floor (1000ms)
  maxWindowMs: number;              // Ceiling (15000ms)

  // Health metrics
  missedThoughts: number;           // Late arrivals count
  completedRounds: number;          // Total decision rounds
  avgParticipation: number;         // % of room that evaluates
}
```

#### 2. Adaptive Window Algorithm

**Initial State**:
- Start with conservative window (5 seconds)
- Measure actual evaluation times
- Adapt toward optimal

**Adaptation Logic**:
```typescript
calculateAdaptiveWindow(heartbeat: SystemHeartbeat): number {
  // Target: Capture 95% of evaluations
  const targetTime = heartbeat.p95EvaluationTime;

  // Smooth adaptation (20% weight to new data)
  const newWindow = 0.8 * heartbeat.currentWindowMs + 0.2 * targetTime;

  // Clamp to safe bounds
  return Math.max(
    heartbeat.minWindowMs,
    Math.min(heartbeat.maxWindowMs, newWindow)
  );
}
```

**Network Adaptation** (TCP Slow-Start style):
- Fast network → shorter window (1-2 seconds)
- Slow network → longer window (8-10 seconds)
- Packet loss → increase window temporarily
- Stable period → gradually decrease window

#### 3. Late-Arrival Queue (NEW)

**Problem**: Late thoughts shouldn't be dropped

**Solution**: Queue them for next decision round

```typescript
interface LateArrivalQueue {
  messageId: UUID;
  queuedThoughts: Thought[];
  maxDepth: number;  // Prevent over-accumulation (default: 10)
}

async broadcastThought(messageId: string, thought: Thought): Promise<void> {
  const stream = this.getOrCreateStream(messageId, thought.personaId);

  // Check if decision already made
  if (stream.phase === 'decided') {
    // Queue for next round instead of dropping
    const queue = this.getOrCreateQueue(messageId);

    if (queue.queuedThoughts.length < queue.maxDepth) {
      queue.queuedThoughts.push(thought);
      console.log(`⏰ Late arrival queued: ${thought.personaId.slice(0, 8)} (+${Date.now() - stream.startTime}ms)`);

      // Schedule Round 2 if not already scheduled
      this.scheduleNextRound(messageId, queue);
    } else {
      console.warn(`⚠️ Queue full - dropping late thought from ${thought.personaId.slice(0, 8)}`);
    }
    return;
  }

  // Normal flow: Add to current decision round
  stream.thoughts.push(thought);
  stream.considerations.set(thought.personaId, thought);
}
```

#### 4. Latency Penalty (NEW)

**Problem**: Slow evaluators shouldn't delay everyone

**Solution**: Score penalty for late arrivals

```typescript
calculateConfidenceWithPenalty(thought: Thought, stream: ThoughtStream): number {
  const elapsedMs = Date.now() - stream.startTime;
  const targetWindow = stream.heartbeat.currentWindowMs;

  // No penalty if within window
  if (elapsedMs <= targetWindow) {
    return thought.confidence;
  }

  // Penalty for late arrivals: -0.1 per second late
  const lateMs = elapsedMs - targetWindow;
  const penalty = Math.min(0.5, (lateMs / 1000) * 0.1);

  return Math.max(0, thought.confidence - penalty);
}
```

**Effect**:
- Fast evaluator (2s): confidence = 0.9 → 0.9 (no penalty)
- Slow evaluator (8s): confidence = 0.9 → 0.8 (0.1 penalty)
- Very slow (15s): confidence = 0.9 → 0.4 (0.5 penalty max)

#### 5. Round-Based Coordination (NEW)

**Current**: One decision per message

**New**: Multiple rounds if late arrivals queued

```typescript
async scheduleNextRound(messageId: string, queue: LateArrivalQueue): Promise<void> {
  // Wait a bit for more late arrivals to accumulate
  setTimeout(async () => {
    if (queue.queuedThoughts.length === 0) return;

    console.log(`🔄 Starting Round 2 for ${messageId.slice(0, 8)} with ${queue.queuedThoughts.length} queued thoughts`);

    // Create new decision round with queued thoughts
    const stream = this.getOrCreateStream(messageId, queue.queuedThoughts[0].personaId);
    stream.phase = 'gathering';  // Reset to gathering
    stream.roundNumber = (stream.roundNumber || 1) + 1;

    // Add queued thoughts to new round
    for (const thought of queue.queuedThoughts) {
      stream.thoughts.push(thought);
      stream.considerations.set(thought.personaId, thought);
    }

    queue.queuedThoughts = [];  // Clear queue

    // Make decision with Round 2 thoughts
    await this.makeDecision(stream);
  }, 1000);  // 1 second grace period
}
```

---

## Implementation Plan

### Phase 1: Measurement (Non-Breaking)
- ✅ Add timing logs (DONE - committed)
- ✅ Expose timing in ai/thoughtstream command (DONE)
- ✅ Document bug with evidence (DONE - this file)
- ⏳ Track evaluation speeds per persona
- ⏳ Calculate p95/p99 latencies

### Phase 2: Adaptive Window
- ⏳ Implement SystemHeartbeat tracker
- ⏳ Calculate adaptive window based on p95
- ⏳ Smooth adaptation algorithm (TCP-style)
- ⏳ Clamp to safe bounds (1-15 seconds)

### Phase 3: Late-Arrival Queue
- ⏳ Implement queue structure
- ⏳ Queue late thoughts instead of dropping
- ⏳ Schedule Round 2 decisions
- ⏳ Add max depth protection

### Phase 4: Latency Penalty
- ⏳ Calculate penalty for late arrivals
- ⏳ Apply to confidence scoring
- ⏳ Balance incentive vs fairness

### Phase 5: Testing & Tuning
- ⏳ Test with varying network speeds
- ⏳ Test with mixed AI speeds (local + API)
- ⏳ Verify no over-accumulation
- ⏳ Tune adaptation parameters

---

## Expected Outcomes

### Before (Current Broken State)
```
Message: "What is 5 + 3?"
Round 1: +2000ms → Decision with 1 AI (Grok)
        → Together Assistant MISSED
        → Helper AI MISSED
        → GPT MISSED
Result: Only Grok responds (poor!)
```

### After (Fixed with Adaptive Heartbeat)
```
Message: "What is 5 + 3?"
Heartbeat: p95 = 8 seconds, adaptive window = 8.5s
Round 1: +8500ms → Decision with 5 AIs
        → All evaluated within window
        → Moderator grants 2-3 based on diversity
Result: Rich, diverse conversation (good!)
```

### With Late Arrivals
```
Message: "What is 5 + 3?"
Round 1: +8500ms → Decision with 4 AIs
Round 2: +12000ms → Very slow AI arrives late
        → Queued for Round 2
        → Decision with 1 AI (penalized confidence)
        → Granted if score still high enough
Result: No thoughts dropped, fair penalty for slowness
```

---

## Monitoring Commands

```bash
# Check recent evaluation timing
./jtag ai/thoughtstream --limit=5 | jq '.streams[] | {
  message: .messageContent[0:40],
  thoughtCount: (.thoughts | length),
  evaluationDuration: .evaluationDuration,
  decisionTime: .decision.waitDuration
}'

# Check heartbeat health (after implementation)
./jtag ai/coordinator/heartbeat --contextId=<ROOM_ID>
# Expected output:
{
  "avgWindow": "6.2s",
  "p95Time": "8.1s",
  "missedRate": "5%",
  "avgParticipation": "92%"
}
```

---

## Related Files

- `system/conversation/server/ThoughtStreamCoordinator.ts` - Main coordinator
- `system/conversation/shared/BaseModerator.ts` - Decision timing logic
- `commands/ai/thoughtstream/` - Diagnostic command
- `commands/ai/rag/inspect/` - RAG context inspection

---

## References

- TCP Slow-Start Algorithm (network congestion control)
- Biological heartbeat adaptation (cardiovascular system)
- Distributed consensus protocols (Raft, Paxos)
- Real-time systems theory (deadline scheduling)
