# Adaptive Thresholds Roadmap

**Philosophy**: "Hard-coded heuristics need to be properly abstracted, with the plan of phasing them out"

The current system uses fixed thresholds that work but prevent organic adaptation. This document outlines the strategy for replacing hard-coded heuristics with learned, adaptive behavior.

---

## Current Hard-Coded Heuristics (To Be Phased Out)

### PersonaState Thresholds
```typescript
// system/user/server/modules/PersonaState.ts

// ENERGY THRESHOLDS (hard-coded)
if (this.state.energy < 0.3) return 'tired';      // Line 124
if (this.state.energy >= 0.5) return 'active';    // Line 129

// INBOX OVERLOAD (hard-coded)
if (this.state.inboxLoad > 50) return 'overwhelmed';  // Line 119

// ENGAGEMENT THRESHOLDS (hard-coded)
priority > 0.8  // Always engage (line 149)
priority > 0.9  // Overwhelmed (line 156)
priority > 0.5 && energy > 0.2  // Tired (line 163)
priority > 0.3  // Active (line 170)
priority > 0.1  // Idle (line 176)

// CADENCE TIMING (hard-coded)
idle: 3000ms, active: 5000ms, tired: 7000ms, overwhelmed: 10000ms  // Lines 196-207
```

### PersonaInbox Priority Weights
```typescript
// system/user/server/modules/PersonaInbox.ts (calculateMessagePriority)

// PRIORITY WEIGHTS (hard-coded)
base: 0.2           // Line 199
mention: +0.4       // Line 203
recent (<1min): +0.2   // Line 209
recent (<5min): +0.1   // Line 211
active room: +0.1      // Line 216
expertise: +0.1        // Line 227
```

---

## Phase 1: Abstract Into Configuration (Current Work)

**Goal**: Extract hard-coded values into configurable parameters WITHOUT changing behavior.

### PersonaState Configuration
```typescript
// system/user/server/modules/PersonaState.ts

export interface StateConfig {
  // Energy thresholds (currently hard-coded)
  tiredEnergyThreshold: number;      // 0.3
  activeEnergyThreshold: number;     // 0.5

  // Inbox thresholds (currently hard-coded)
  overwhelmedInboxThreshold: number;  // 50

  // Engagement thresholds (currently hard-coded)
  engagementThresholds: {
    alwaysEngage: number;    // 0.8
    overwhelmed: number;     // 0.9
    tiredPriority: number;   // 0.5
    tiredEnergy: number;     // 0.2
    active: number;          // 0.3
    idle: number;            // 0.1
  };

  // Cadence timing (currently hard-coded)
  cadenceTiming: {
    idle: number;        // 3000
    active: number;      // 5000
    tired: number;       // 7000
    overwhelmed: number; // 10000
  };

  // Existing fields
  energyDepletionRate: number;
  energyRecoveryRate: number;
  attentionFatigueRate: number;
  enableLogging: boolean;
}
```

### PersonaInbox Priority Configuration
```typescript
// system/user/server/modules/PersonaInbox.ts

export interface PriorityWeights {
  base: number;           // 0.2
  mention: number;        // 0.4
  recentImmediate: {      // <1 minute
    threshold: number;    // 60000
    weight: number;       // 0.2
  };
  recentModerate: {       // <5 minutes
    threshold: number;    // 300000
    weight: number;       // 0.1
  };
  activeRoom: number;     // 0.1
  expertise: number;      // 0.1
}

export function calculateMessagePriority(
  message: { content: string; timestamp: number; roomId: UUID },
  persona: { displayName: string; id: UUID; recentRooms?: UUID[]; expertise?: string[] },
  weights: PriorityWeights = DEFAULT_PRIORITY_WEIGHTS  // NEW parameter
): number {
  // Use weights instead of hard-coded values
}
```

**Status**: ❌ Not implemented yet

---

## Phase 2: Metrics Collection (Foundation for Learning)

**Goal**: Track performance metrics WITHOUT changing behavior yet.

### Metrics to Collect
```typescript
// system/user/server/modules/PersonaMetrics.ts (NEW FILE)

export interface PerformanceMetrics {
  // Engagement metrics
  messagesEvaluated: number;
  messagesEngaged: number;
  messagesSkipped: number;
  engagementRate: number;  // engaged / evaluated

  // Priority distribution
  highPriorityMissed: number;   // priority > 0.8 but skipped
  lowPriorityEngaged: number;   // priority < 0.3 but engaged

  // Energy metrics
  averageEnergyLevel: number;
  timeInTiredState: number;     // ms spent tired
  timeInOverwhelmedState: number;

  // Cadence metrics
  averageResponseTime: number;   // Time from message to response
  missedDeadlines: number;       // High-priority messages delayed

  // Inbox metrics
  averageInboxLoad: number;
  peakInboxLoad: number;
  messagesDropped: number;       // Lost due to overflow
}

export class PersonaMetricsCollector {
  private metrics: PerformanceMetrics;
  private readonly windowSize: number = 100;  // Track last 100 messages

  recordEngagement(message: InboxMessage, engaged: boolean, state: PersonaState): void {
    // Track decision
    this.metrics.messagesEvaluated++;
    if (engaged) {
      this.metrics.messagesEngaged++;
      if (message.priority < 0.3) {
        this.metrics.lowPriorityEngaged++;
      }
    } else {
      this.metrics.messagesSkipped++;
      if (message.priority > 0.8) {
        this.metrics.highPriorityMissed++;  // CRITICAL: We missed high priority!
      }
    }

    // Track state
    if (state.mood === 'tired') {
      this.metrics.timeInTiredState += state.getCadence();
    }

    // Calculate derived metrics
    this.metrics.engagementRate = this.metrics.messagesEngaged / this.metrics.messagesEvaluated;
  }

  getMetrics(): PerformanceMetrics {
    return { ...this.metrics };
  }

  reset(): void {
    // Reset for new window
  }
}
```

**Integration Point**:
```typescript
// PersonaUser.ts - Add metrics collector
private metricsCollector: PersonaMetricsCollector;

async evaluateShouldRespond(context: RAGContext): Promise<{ shouldRespond: boolean; confidence: number }> {
  const message = /* extract from context */;
  const state = this.personaState.getState();

  // Make decision (using current hard-coded thresholds)
  const shouldEngage = this.personaState.shouldEngage(message.priority);

  // Record decision for metrics (NEW)
  this.metricsCollector.recordEngagement(message, shouldEngage, this.personaState);

  return { shouldRespond: shouldEngage, confidence: /* ... */ };
}
```

**Status**: ❌ Not implemented yet

---

## Phase 3: Adaptive Learning (Replace Hard-Coded with Learned)

**Goal**: Use metrics to ADJUST thresholds, replacing hard-coded values with learned ones.

### Adaptation Strategy

#### 1. Threshold Adaptation Based on Miss Rate
```typescript
// system/user/server/modules/AdaptiveThresholds.ts (NEW FILE)

export class AdaptiveThresholdManager {
  private thresholds: StateConfig['engagementThresholds'];
  private metrics: PersonaMetricsCollector;

  constructor(initialThresholds: StateConfig['engagementThresholds']) {
    this.thresholds = { ...initialThresholds };
  }

  adapt(): void {
    const metrics = this.metrics.getMetrics();

    // RULE 1: If missing high-priority messages, lower thresholds (be more eager)
    if (metrics.highPriorityMissed > 0) {
      this.thresholds.tired.priority *= 0.95;  // Lower threshold by 5%
      this.thresholds.active *= 0.95;
      console.log(`⚠️  Missed ${metrics.highPriorityMissed} high-priority messages - lowering thresholds`);
    }

    // RULE 2: If engaging with too many low-priority (getting exhausted), raise thresholds
    const lowPriorityRate = metrics.lowPriorityEngaged / metrics.messagesEngaged;
    if (lowPriorityRate > 0.5 && metrics.timeInTiredState > 60000) {  // More than 1 minute tired
      this.thresholds.idle *= 1.05;  // Raise threshold by 5%
      this.thresholds.active *= 1.05;
      console.log(`⚠️  Too much low-priority engagement (${(lowPriorityRate*100).toFixed(0)}%) - raising thresholds`);
    }

    // RULE 3: If inbox overflowing, raise overwhelmed threshold (shed more load)
    if (metrics.messagesDropped > 0) {
      this.thresholds.overwhelmed *= 0.95;  // LOWER threshold = shed load sooner
      console.log(`⚠️  Dropped ${metrics.messagesDropped} messages - raising overwhelmed sensitivity`);
    }

    // Clamp thresholds to reasonable ranges
    this.thresholds.idle = Math.max(0.05, Math.min(0.3, this.thresholds.idle));
    this.thresholds.active = Math.max(0.2, Math.min(0.5, this.thresholds.active));
    this.thresholds.tired.priority = Math.max(0.4, Math.min(0.7, this.thresholds.tired.priority));

    // Reset metrics for next window
    this.metrics.reset();
  }

  getThresholds(): StateConfig['engagementThresholds'] {
    return { ...this.thresholds };
  }
}
```

#### 2. Cadence Adaptation Based on Response Time
```typescript
export class AdaptiveCadenceManager {
  private cadence: StateConfig['cadenceTiming'];
  private metrics: PersonaMetricsCollector;

  adapt(): void {
    const metrics = this.metrics.getMetrics();

    // RULE 1: If missing deadlines, speed up cadence
    if (metrics.missedDeadlines > 0) {
      this.cadence.idle *= 0.9;  // Check 10% faster
      this.cadence.active *= 0.9;
      console.log(`⚠️  Missed ${metrics.missedDeadlines} deadlines - speeding up cadence`);
    }

    // RULE 2: If responding too quickly (low inbox), slow down cadence (save energy)
    if (metrics.averageInboxLoad < 3 && metrics.averageEnergyLevel > 0.8) {
      this.cadence.idle *= 1.1;  // Check 10% slower
      console.log(`✅ Low load, high energy - slowing cadence to conserve`);
    }

    // Clamp cadence to reasonable ranges
    this.cadence.idle = Math.max(1000, Math.min(10000, this.cadence.idle));
    this.cadence.active = Math.max(2000, Math.min(15000, this.cadence.active));
    this.cadence.tired = Math.max(5000, Math.min(20000, this.cadence.tired));
  }
}
```

**Integration Point**:
```typescript
// PersonaUser.ts - Add adaptive managers
private adaptiveThresholds: AdaptiveThresholdManager;
private adaptiveCadence: AdaptiveCadenceManager;

// Run adaptation every 100 messages
private messageCount = 0;
async evaluateShouldRespond(context: RAGContext): Promise<...> {
  this.messageCount++;

  if (this.messageCount % 100 === 0) {
    // Adapt thresholds based on last 100 messages
    this.adaptiveThresholds.adapt();
    this.adaptiveCadence.adapt();

    // Update PersonaState with new thresholds
    this.personaState.updateThresholds(this.adaptiveThresholds.getThresholds());
    this.personaState.updateCadence(this.adaptiveCadence.getCadence());
  }

  // ... rest of evaluation
}
```

**Status**: ❌ Not implemented yet

---

## Phase 4: Genome-Based Adaptation (Long-Term Learning)

**Goal**: Persist learned thresholds in PersonaUser genome (LoRA weights or config).

### Genome Storage
```typescript
// PersonaUser genome stores learned thresholds
{
  "thresholds": {
    "idle": 0.12,      // Learned: slightly more selective than default 0.1
    "active": 0.28,    // Learned: slightly more eager than default 0.3
    "tired": 0.52,     // Learned: slightly more selective than default 0.5
    "overwhelmed": 0.88  // Learned: shed load earlier than default 0.9
  },
  "cadence": {
    "idle": 3200,      // Learned: slightly slower than default 3000
    "active": 4800,    // Learned: slightly faster than default 5000
    "tired": 7200,     // Learned: slightly faster than default 7000
    "overwhelmed": 9500  // Learned: slightly faster than default 10000
  }
}
```

### Initialization
```typescript
// PersonaUser.ts - Load learned thresholds from genome
constructor(...) {
  // Load genome config
  const genome = await this.loadGenome();

  // Initialize with learned thresholds (if available)
  const thresholds = genome.thresholds || DEFAULT_ENGAGEMENT_THRESHOLDS;
  const cadence = genome.cadence || DEFAULT_CADENCE_TIMING;

  this.personaState = new PersonaStateManager(this.displayName, {
    engagementThresholds: thresholds,
    cadenceTiming: cadence,
    // ... other config
  });

  this.adaptiveThresholds = new AdaptiveThresholdManager(thresholds);
  this.adaptiveCadence = new AdaptiveCadenceManager(cadence);
}
```

### Periodic Save
```typescript
// Save learned thresholds back to genome every N adaptations
private adaptationCount = 0;
async evaluateShouldRespond(context: RAGContext): Promise<...> {
  if (this.messageCount % 100 === 0) {
    this.adaptiveThresholds.adapt();
    this.adaptiveCadence.adapt();

    this.adaptationCount++;
    if (this.adaptationCount % 10 === 0) {
      // Save to genome every 1000 messages (10 adaptation windows)
      await this.saveGenome({
        thresholds: this.adaptiveThresholds.getThresholds(),
        cadence: this.adaptiveCadence.getCadence()
      });
    }
  }
}
```

**Status**: ❌ Not implemented yet

---

## Phase 5: Multi-Persona Learning (Future)

**Goal**: Personas learn from EACH OTHER via shared metrics.

### Shared Learning Architecture
```typescript
// system/user/server/modules/SharedLearning.ts (FUTURE)

export class PersonaCommunity {
  private personas: Map<UUID, PersonaUser>;
  private sharedMetrics: PerformanceMetrics;

  async shareMetrics(): Promise<void> {
    // Aggregate metrics from all personas
    for (const persona of this.personas.values()) {
      const metrics = persona.getMetrics();
      this.sharedMetrics.merge(metrics);
    }

    // Find best-performing threshold configurations
    const bestThresholds = this.findOptimalThresholds();

    // Broadcast to all personas (they can choose to adopt)
    for (const persona of this.personas.values()) {
      await persona.suggestThresholds(bestThresholds);
    }
  }

  private findOptimalThresholds(): StateConfig['engagementThresholds'] {
    // Which personas have:
    // - Lowest highPriorityMissed rate
    // - Highest engagementRate
    // - Lowest timeInTiredState
    // Return their threshold configuration
  }
}
```

**Status**: ❌ Future work

---

## Implementation Order

1. **Phase 1**: Extract hard-coded values into configuration (Week 1)
   - Modify PersonaState.ts to accept threshold config
   - Modify PersonaInbox.ts to accept priority weights
   - NO behavior change, just abstraction

2. **Phase 2**: Add metrics collection (Week 2)
   - Create PersonaMetricsCollector
   - Integrate into PersonaUser evaluation loop
   - Collect data, NO adaptation yet

3. **Phase 3**: Implement adaptive learning (Week 3)
   - Create AdaptiveThresholdManager
   - Create AdaptiveCadenceManager
   - Run adaptation every 100 messages
   - Verify improvement via metrics

4. **Phase 4**: Genome persistence (Week 4)
   - Save learned thresholds to genome
   - Load on initialization
   - Personas remember their learned behavior

5. **Phase 5**: Multi-persona learning (Future)
   - Community-wide metric sharing
   - Best-practice propagation
   - Collective intelligence

---

## Testing Strategy

### Phase 1 Tests
- Verify configuration abstraction doesn't change behavior
- Unit tests pass with custom thresholds
- Integration tests unchanged

### Phase 2 Tests
- Metrics correctly track engagement decisions
- High-priority miss detection works
- Low-priority overload detection works

### Phase 3 Tests
- Thresholds adapt in response to metrics
- Adaptation improves performance (fewer misses, less exhaustion)
- Thresholds stabilize after learning period

### Phase 4 Tests
- Genome save/load preserves learned thresholds
- Persona resumes with learned behavior after restart

### Phase 5 Tests
- Community learning improves all personas
- Best practices propagate correctly
- Personas maintain individual specialization

---

## Success Criteria

**Phase 1**: ✅ Configuration abstraction complete, all tests pass
**Phase 2**: ✅ Metrics collection running, data shows decision patterns
**Phase 3**: ✅ Adaptive learning reduces high-priority misses by >50%
**Phase 4**: ✅ Genome persistence allows learned behavior to survive restarts
**Phase 5**: ✅ Community learning improves average performance across all personas

---

## Philosophy Alignment

> "Hard-coded heuristics need to be properly abstracted, with the plan of phasing them out"

This roadmap follows the philosophy:
1. **Abstract first**: Extract values into configuration (no behavior change)
2. **Measure second**: Collect metrics to understand current behavior
3. **Adapt third**: Use metrics to learn better thresholds
4. **Persist fourth**: Save learned behavior in genome
5. **Share fifth**: Community-wide learning and best practices

The goal is **organic adaptation** - personas that learn from experience, not rigid rules.

---

**Created**: 2025-10-29 00:18
**Status**: Roadmap defined, Phase 1 ready to begin
**Next Step**: Extract hard-coded thresholds into PersonaState configuration
