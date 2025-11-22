# Lean Core Loop Pattern

**Principle**: Core persona loop must be **free of bottlenecks** so it can react quickly

Like cbar's core animation loop - FAST coordination, heavy processing off-thread

---

## The Problem with Current Approach

### Memory Consolidation Subprocess (Current)
```typescript
protected async tick(): Promise<void> {
  // ❌ HEAVY: Peek inbox
  const inboxItems = await this.persona.inbox.peek(10);

  // ❌ HEAVY: Recall working memory
  const thoughts = await this.persona.workingMemory.recall({ limit: 20 });

  // ❌ HEAVY: Detect patterns
  const patterns = await this.detectPatterns(inboxItems, thoughts);

  // ❌ HEAVY: Process consolidation
  if (patterns.shouldConsolidate) {
    await this.consolidate();
  }
}
```

**Every cycle does heavy work** - even when nothing has changed

---

## The Solution: Signal-Based Activation

### 1. **Lean State Checks** (No Heavy Processing)

Instead of doing heavy work every cycle, just check lightweight signals:

```typescript
interface MemorySignals {
  memoryPressure: number;      // Just read counter (FAST)
  inboxDepthChanged: boolean;   // Just compare numbers (FAST)
  patternsDetected: boolean;    // Set by external trigger (FAST)
}

// Every cycle: Just read counters (FAST)
private checkSignals(): MemorySignals {
  return {
    memoryPressure: this.persona.workingMemory.used / this.persona.workingMemory.max,
    inboxDepthChanged: this.lastInboxDepth !== this.persona.inbox.depth,
    patternsDetected: this.patternFlag  // Set externally
  };
}
```

### 2. **Trigger-Based Processing** (Like cbar motion detection)

Only do heavy work when triggered:

```typescript
protected async tick(): Promise<void> {
  // Check signals (FAST - just read counters)
  const signals = this.checkSignals();

  // Only process when triggered
  if (signals.memoryPressure > 0.8) {
    // NOW do the heavy work
    await this.consolidateHighPressure();
  }

  if (signals.inboxDepthChanged) {
    // Check if patterns emerged
    await this.checkForPatterns();
  }
}
```

**Like cbar:**
- Motion detected → trigger semantic segmentation
- New area detected → trigger feature extraction
- Plane found → trigger geometry analysis

**For us:**
- Memory pressure → trigger consolidation
- Inbox spike → trigger pattern detection
- Idle detected → trigger self-task generation

---

## Context-Adaptive Priority (Like Hippocampus)

### Dynamic Priority Based on Persona State

```typescript
interface PersonaState {
  isFocused: boolean;       // Currently processing a task
  cognitiveLoad: number;    // 0.0 = idle, 1.0 = max load
}

// Adjust subprocess priority based on state
private getEffectivePriority(): number {
  const basePriority = this.basePriority;  // e.g., 0.5

  if (this.persona.state.isFocused) {
    // Like hippocampus during focus - reduce background processing
    return basePriority * 0.3;  // 70% reduction
  }

  if (this.persona.state.cognitiveLoad < 0.3) {
    // Low load - increase background processing
    return basePriority * 1.5;  // 50% increase
  }

  return basePriority;
}
```

**Like hippocampus:**
- During focus: Slow down memory consolidation
- During idle: Speed up memory consolidation
- High load: Defer non-critical work
- Low load: Opportunistically process

---

## Subprocess Sleep Timing

### Current: Fixed Timing
```typescript
// ❌ Always wait 500ms, regardless of context
await this.sleep(500);
```

### Better: Adaptive Timing
```typescript
// ✅ Adapt based on priority and context
private getSleepTime(): number {
  const effectivePriority = this.getEffectivePriority();

  // Higher priority = shorter sleep
  // Lower priority = longer sleep
  const baseTime = 1000;  // 1 second base
  return baseTime * (1 - effectivePriority);
}

// In tick():
await this.sleep(this.getSleepTime());
```

**Result:**
- High priority + focused: ~300ms cycles
- Low priority + idle: ~700ms cycles
- Dynamic adaptation to context

---

## Pattern: Dependency-Based Activation

### Like cbar: Feature Extraction → Semantic Segmentation → Geometry Analysis

```typescript
// Feature extraction detects new area
if (newAreaDetected) {
  // Wake semantic segmentation
  this.persona.semanticAnalyzer.wakeup();
}

// Semantic segmentation finds plane
if (planeDetected) {
  // Wake geometry analyzer
  this.persona.geometryAnalyzer.wakeup();
}
```

### For us: Pattern Detection → Consolidation → Activation

```typescript
// Pattern detector finds cluster
if (patternsEmerging) {
  // Wake memory consolidation
  this.persona.memoryWorker.wakeup();
}

// Consolidation stores to long-term
if (consolidationComplete) {
  // Wake activation checker
  this.persona.activationWorker.wakeup();
}
```

**Dependencies chain together, not fixed schedules**

---

## Implementation: Refactor Memory Subprocess

### Before (Heavy Every Cycle)
```typescript
export class MemoryConsolidationSubprocess extends PersonaContinuousSubprocess {
  protected async tick(): Promise<void> {
    // Heavy work every cycle
    const inboxItems = await this.persona.inbox.peek(10);  // Heavy
    const thoughts = await this.persona.workingMemory.recall({ limit: 20 });  // Heavy
    const patterns = await this.detectPatterns(inboxItems, thoughts);  // Heavy

    if (patterns.shouldConsolidate) {
      await this.consolidate();  // Heavy
    }
  }
}
```

### After (Signal-Based, Lean Checks)
```typescript
export class MemoryConsolidationSubprocess extends PersonaContinuousSubprocess {
  private lastInboxDepth: number = 0;
  private lastMemoryCheck: number = 0;

  protected async tick(): Promise<void> {
    // LEAN checks only
    const now = Date.now();
    const signals = this.checkSignals();

    // Only do heavy work when triggered
    if (signals.memoryPressure > 0.8) {
      await this.handleMemoryPressure();
    } else if (signals.inboxDepthChanged && now - this.lastMemoryCheck > 5000) {
      // Check for patterns only if inbox changed AND 5 seconds elapsed
      await this.checkForPatterns();
      this.lastMemoryCheck = now;
    }

    // Adaptive sleep based on priority
    await this.sleep(this.getSleepTime());
  }

  // LEAN: Just read counters
  private checkSignals(): MemorySignals {
    const currentInboxDepth = this.persona.inbox.getDepth();
    const memoryCapacity = this.persona.workingMemory.getCapacity('global');

    const signals = {
      memoryPressure: memoryCapacity.used / memoryCapacity.max,
      inboxDepthChanged: currentInboxDepth !== this.lastInboxDepth
    };

    this.lastInboxDepth = currentInboxDepth;
    return signals;
  }

  // HEAVY: Only called when triggered
  private async handleMemoryPressure(): Promise<void> {
    const candidates = await this.persona.workingMemory.recall({
      minImportance: 0.6,
      limit: 50
    });

    await this.consolidate(candidates);
  }

  // HEAVY: Only called when inbox changed
  private async checkForPatterns(): Promise<void> {
    const inboxItems = await this.persona.inbox.peek(10);
    const thoughts = await this.persona.workingMemory.recall({ limit: 20 });

    const patterns = await this.detectPatterns(inboxItems, thoughts);

    if (patterns.shouldConsolidate) {
      await this.consolidate(patterns.candidates);
    }
  }
}
```

**Key changes:**
- ✅ Tick() is now LEAN (just check signals)
- ✅ Heavy work only when triggered
- ✅ Adaptive sleep timing
- ✅ Rate limiting (5 second minimum between pattern checks)

---

## Benefits

### 1. **Faster Reaction Time**
- Lean checks every cycle (10-100ms)
- Heavy work only when needed
- No wasted processing

### 2. **Context-Adaptive**
- Slow down during focus (like hippocampus)
- Speed up during idle
- Dynamic priority adjustment

### 3. **Dependency-Based**
- Pattern detected → consolidate
- Consolidation complete → activate
- Like cbar's motion → semantic → geometry chain

### 4. **Efficient Resource Usage**
- No continuous heavy processing
- Opportunistic work during idle
- Rate limiting prevents thrashing

---

## Core Principle

**The core loop must be free of bottlenecks so it can react quickly**

Like cbar:
- Core animation loop: LEAN coordination
- Heavy processing: Off-thread, triggered by events
- Responsiveness: Paramount

For us:
- Core persona loop: LEAN signal checks
- Heavy processing: In subprocesses, triggered by signals
- Responsiveness: Fast reaction to events

**Don't do heavy work in every cycle - only when triggered**
