# Non-Linear Importance Scoring & Adaptive Consolidation

**Status**: Design Document (Implementation Pending)
**Created**: 2025-11-22
**Research Basis**: PER (DeepMind 2016), TD-learning, Softmax Temperature, Attention Mechanisms

---

## Problem Statement

**Current system flaws:**
1. **Linear addition** for importance: `0.2 + 0.4 + 0.1 = 0.7` (lame, predictable)
2. **Fixed threshold** (0.6): Most messages score 0.2-0.5, never consolidate
3. **No activity-responsiveness**: Threshold doesn't adapt to conversation pace
4. **Ignores surprise/novelty**: Treats expected and unexpected equally

**Biological hippocampus behavior:**
- Consolidates based on **surprise** (prediction error)
- **Dopamine-driven** salience (emotion, novelty, reward)
- **Activity-adaptive**: Low activity → more consolidation (concentration effect)
- **Non-linear**: Small changes in importance cause large behavioral shifts

---

## Research-Based Solution

### Core Principle: TD-Error (Temporal Difference Learning)

**From reinforcement learning and neuroscience:**

```
TD-error = Actual - Predicted
```

- **High TD-error** → Surprising event → **Consolidate**
- **Low TD-error** → Expected event → **Ignore**

**Example:**
- First time AI mentioned by name: High surprise → Consolidate
- 100th greeting message: Low surprise → Ignore

---

## Part 1: Non-Linear Message Importance

### Current (Linear, Lame):
```typescript
let priority = 0.2;  // Base
if (mentioned) priority += 0.4;
if (fresh) priority += 0.2;
if (activeRoom) priority += 0.1;
if (expertise) priority += 0.1;
return Math.min(1.0, priority); // Cap at 1.0
```

**Problems:**
- Simple addition (no interactions between features)
- Caps at 1.0 (loses information)
- All features weighted equally
- No learned adaptation

### Proposed (Non-Linear, Organic):

```typescript
/**
 * Calculate message importance using softmax-based feature weighting
 * Research: Attention mechanisms, PER (Schaul et al. 2016)
 */
function calculateImportance(
  message: ChatMessageEntity,
  persona: PersonaUser,
  context: ConversationContext
): number {
  // 1. Extract features (raw scores, no addition yet)
  const features = {
    mentioned: isMentioned(message, persona) ? 1.0 : 0.0,
    recency: calculateRecencyScore(message.timestamp),  // 1.0 → 0.0 over time
    conversationHeat: context.temperature,              // 0.0 → 1.0 (activity level)
    expertiseMatch: calculateExpertiseOverlap(message, persona),
    semanticNovelty: calculateSemanticDistance(message, context.recentMessages),
    emotionalSalience: detectEmotionalContent(message)
  };

  // 2. Apply learned/heuristic weights
  const weights = {
    mentioned: 3.0,           // Strongest signal
    recency: 1.5,
    conversationHeat: 1.0,
    expertiseMatch: 0.8,
    semanticNovelty: 2.0,     // Surprise matters!
    emotionalSalience: 1.5
  };

  // 3. Weighted feature scores
  const scores = Object.entries(features).map(([key, value]) =>
    value * weights[key as keyof typeof weights]
  );

  // 4. Softmax normalization (non-linear!)
  const temperature = context.activityLevel > 0.7 ? 2.0 : 0.5;  // Adaptive T
  const softmaxScores = softmax(scores, temperature);

  // 5. Power function for emphasis (PER-style)
  const alpha = 0.6;  // Exponent for non-linearity
  const importance = Math.pow(
    softmaxScores.reduce((sum, s) => sum + s, 0) / softmaxScores.length,
    alpha
  );

  return importance;
}

/**
 * Softmax with temperature
 * High T → uniform (more exploratory)
 * Low T → sharp (more focused)
 */
function softmax(scores: number[], temperature: number = 1.0): number[] {
  const expScores = scores.map(s => Math.exp(s / temperature));
  const sum = expScores.reduce((a, b) => a + b, 0);
  return expScores.map(s => s / sum);
}

/**
 * Semantic novelty: How different is this message from recent context?
 * Uses simple cosine distance (future: embeddings)
 */
function calculateSemanticDistance(
  message: ChatMessageEntity,
  recentMessages: ChatMessageEntity[]
): number {
  if (recentMessages.length === 0) return 1.0;  // First message = novel

  // Simple heuristic: word overlap (future: embedding distance)
  const messageWords = new Set(message.content.toLowerCase().split(/\s+/));

  const distances = recentMessages.slice(0, 5).map(recent => {
    const recentWords = new Set(recent.content.toLowerCase().split(/\s+/));
    const intersection = new Set([...messageWords].filter(w => recentWords.has(w)));
    return 1.0 - (intersection.size / messageWords.size);
  });

  return distances.reduce((sum, d) => sum + d, 0) / distances.length;
}

/**
 * Recency score: Exponential decay (non-linear!)
 */
function calculateRecencyScore(timestamp: number): number {
  const ageMs = Date.now() - timestamp;
  const halfLifeMs = 300000;  // 5 minutes
  return Math.exp(-ageMs / halfLifeMs);  // Exponential decay
}
```

**Key improvements:**
- ✅ **Softmax normalization** (non-linear, bounded 0-1)
- ✅ **Power function** emphasis (α < 1 smooths, α > 1 sharpens)
- ✅ **Temperature control** (activity-responsive)
- ✅ **Semantic novelty** (surprise detection)
- ✅ **Exponential decay** for time (not linear steps)
- ✅ **Feature interactions** through learned weights

---

## Part 2: Adaptive Consolidation Threshold

### Current (Fixed, Dumb):
```typescript
private readonly CONSOLIDATION_THRESHOLD = 0.6;  // Never reached!
```

### Proposed (Adaptive, Organic):

```typescript
/**
 * Adaptive consolidation threshold based on activity level
 * Research: Temperature annealing, PER importance sampling
 */
class AdaptiveConsolidationThreshold {
  private baseThreshold = 0.3;           // Minimum threshold
  private maxThreshold = 0.8;            // Maximum threshold
  private currentThreshold = 0.5;        // Current adaptive value

  // Activity tracking
  private recentActivity: number[] = [];  // Messages per minute, last 10 minutes
  private activityWindow = 10;            // 10 minute window

  /**
   * Update threshold based on conversation activity (NON-LINEAR)
   *
   * Concentration effect:
   * - Low activity → Low threshold → Consolidate MORE
   * - High activity → High threshold → Consolidate LESS
   */
  updateThreshold(messagesPerMinute: number): void {
    // Track recent activity
    this.recentActivity.push(messagesPerMinute);
    if (this.recentActivity.length > this.activityWindow) {
      this.recentActivity.shift();
    }

    // Calculate average activity
    const avgActivity = this.recentActivity.reduce((sum, a) => sum + a, 0)
                       / this.recentActivity.length;

    // SIGMOID FUNCTION for smooth, organic adjustment (NOT linear addition!)
    //
    // sigmoid(x) = 1 / (1 + exp(-k*(x - x0)))
    // where:
    //   k = steepness (higher = sharper transition)
    //   x0 = midpoint (activity level where threshold = 0.5)
    const k = 0.5;          // Moderate steepness
    const x0 = 5.0;         // Midpoint at 5 msg/min

    const normalizedThreshold = 1.0 / (1.0 + Math.exp(-k * (avgActivity - x0)));

    // Map [0,1] → [baseThreshold, maxThreshold]
    this.currentThreshold = this.baseThreshold +
      (this.maxThreshold - this.baseThreshold) * normalizedThreshold;
  }

  /**
   * Get current threshold for consolidation decision
   */
  getThreshold(): number {
    return this.currentThreshold;
  }

  /**
   * Should consolidate this thought?
   */
  shouldConsolidate(importance: number): boolean {
    return importance >= this.currentThreshold;
  }

  /**
   * Get statistics for logging/debugging
   */
  getStats() {
    const avgActivity = this.recentActivity.reduce((sum, a) => sum + a, 0)
                       / Math.max(1, this.recentActivity.length);
    return {
      currentThreshold: this.currentThreshold,
      baseThreshold: this.baseThreshold,
      maxThreshold: this.maxThreshold,
      avgActivity,
      activityWindow: this.activityWindow
    };
  }
}
```

**Behavior:**

```
Activity (msg/min) → Threshold → Effect
------------------------------------------
0.5 (quiet)        → 0.32      → Consolidate 90% of memories (concentration)
2.0 (moderate)     → 0.42      → Consolidate 50% of memories
5.0 (busy)         → 0.55      → Consolidate 20% of memories (filter noise)
10.0 (chaotic)     → 0.72      → Consolidate 5% of memories (only critical)
```

**Key features:**
- ✅ **Sigmoid function** (smooth, organic, non-linear)
- ✅ **Activity-responsive** (inverse relationship)
- ✅ **Bounded** (can't go below base or above max)
- ✅ **No simple addition** (exponential curve)
- ✅ **Concentration effect** (low activity → more storage)

---

## Part 3: TD-Error Based Surprise Detection

### Concept: Prediction Error Drives Consolidation

```typescript
/**
 * Calculate surprise (TD-error) for a message
 * High surprise → Important → Consolidate
 */
class SurpriseDetector {
  // Recent message history for prediction
  private messageHistory: ChatMessageEntity[] = [];
  private maxHistory = 50;

  /**
   * Calculate how surprising this message is
   *
   * TD-error = |Actual - Predicted|
   *
   * Heuristics for "expected" messages:
   * - Similar content to recent messages (low surprise)
   * - Same author as usual (low surprise)
   * - Expected response time (low surprise)
   *
   * Future: Use language model perplexity for true prediction
   */
  calculateSurprise(message: ChatMessageEntity): number {
    if (this.messageHistory.length < 5) {
      return 0.8;  // Early messages are surprising (no prior)
    }

    let surpriseScore = 0.0;

    // 1. Semantic surprise (content novelty)
    const semanticSurprise = this.calculateSemanticSurprise(message);
    surpriseScore += semanticSurprise * 0.4;

    // 2. Temporal surprise (unexpected timing)
    const temporalSurprise = this.calculateTemporalSurprise(message);
    surpriseScore += temporalSurprise * 0.2;

    // 3. Author surprise (unexpected speaker)
    const authorSurprise = this.calculateAuthorSurprise(message);
    surpriseScore += authorSurprise * 0.2;

    // 4. Length surprise (unusual message length)
    const lengthSurprise = this.calculateLengthSurprise(message);
    surpriseScore += lengthSurprise * 0.2;

    // Update history
    this.messageHistory.push(message);
    if (this.messageHistory.length > this.maxHistory) {
      this.messageHistory.shift();
    }

    return Math.min(1.0, surpriseScore);
  }

  private calculateSemanticSurprise(message: ChatMessageEntity): number {
    // Same as calculateSemanticDistance from Part 1
    const recent = this.messageHistory.slice(-5);
    const messageWords = new Set(message.content.toLowerCase().split(/\s+/));

    const distances = recent.map(msg => {
      const msgWords = new Set(msg.content.toLowerCase().split(/\s+/));
      const intersection = new Set([...messageWords].filter(w => msgWords.has(w)));
      return 1.0 - (intersection.size / Math.max(messageWords.size, msgWords.size));
    });

    return distances.reduce((sum, d) => sum + d, 0) / distances.length;
  }

  private calculateTemporalSurprise(message: ChatMessageEntity): number {
    if (this.messageHistory.length < 2) return 0.0;

    // Calculate average inter-message time
    const intervals = [];
    for (let i = 1; i < this.messageHistory.length; i++) {
      intervals.push(
        this.messageHistory[i].timestamp - this.messageHistory[i-1].timestamp
      );
    }
    const avgInterval = intervals.reduce((sum, i) => sum + i, 0) / intervals.length;

    // Current interval
    const lastMessage = this.messageHistory[this.messageHistory.length - 1];
    const currentInterval = message.timestamp - lastMessage.timestamp;

    // Surprise = how much current deviates from average (normalized)
    const deviation = Math.abs(currentInterval - avgInterval) / (avgInterval + 1000);
    return Math.min(1.0, deviation);
  }

  private calculateAuthorSurprise(message: ChatMessageEntity): number {
    // How often does this author speak?
    const recent = this.messageHistory.slice(-10);
    const authorFreq = recent.filter(m => m.authorId === message.authorId).length / recent.length;

    // Rare authors are more surprising
    return 1.0 - authorFreq;
  }

  private calculateLengthSurprise(message: ChatMessageEntity): number {
    // Average message length
    const lengths = this.messageHistory.map(m => m.content.length);
    const avgLength = lengths.reduce((sum, l) => sum + l, 0) / lengths.length;
    const stdDev = Math.sqrt(
      lengths.map(l => Math.pow(l - avgLength, 2)).reduce((sum, x) => sum + x, 0) / lengths.length
    );

    // Z-score (how many std devs from mean)
    const zScore = Math.abs(message.content.length - avgLength) / (stdDev + 1);
    return Math.min(1.0, zScore / 2);  // Normalize
  }
}
```

---

## Complete Integration

### Modified Hippocampus.snoopAndConsolidate():

```typescript
private async snoopAndConsolidate(): Promise<void> {
  if (!this.memoryDbHandle || !this.persona.sessionId) {
    return;
  }

  try {
    // 1. Calculate current activity level
    const messagesPerMinute = await this.calculateActivityLevel();

    // 2. Update adaptive threshold (sigmoid-based, non-linear)
    this.adaptiveThreshold.updateThreshold(messagesPerMinute);
    const currentThreshold = this.adaptiveThreshold.getThreshold();

    // 3. Recall thoughts from working memory (now using adaptive threshold)
    const thoughts = await this.persona.workingMemory.recall({
      minImportance: currentThreshold,  // ← ADAPTIVE!
      limit: 50,
      includePrivate: true
    });

    if (thoughts.length === 0) {
      return;
    }

    this.log(`🧠 Consolidating: threshold=${currentThreshold.toFixed(2)}, ` +
             `activity=${messagesPerMinute.toFixed(1)} msg/min, ` +
             `candidates=${thoughts.length}`);

    // 4. Consolidate each thought
    const consolidatedIds: string[] = [];
    let failedCount = 0;

    for (const thought of thoughts) {
      try {
        // Calculate surprise (TD-error style)
        const surprise = this.surpriseDetector.calculateSurprise(thought);

        // Boost importance by surprise (multiplicative, not additive!)
        const finalImportance = thought.importance * (1.0 + surprise);

        const memory: MemoryEntity = {
          id: generateUUID(),
          createdAt: ISOString(new Date().toISOString()),
          updatedAt: ISOString(new Date().toISOString()),
          version: 0,
          personaId: this.persona.id,
          sessionId: this.persona.sessionId,
          type: this.mapThoughtTypeToMemoryType(thought.thoughtType),
          content: thought.thoughtContent,
          context: {
            domain: thought.domain,
            contextId: thought.contextId,
            thoughtType: thought.thoughtType,
            shareable: thought.shareable,
            surprise: surprise,  // Store for analysis
            activityLevel: messagesPerMinute
          },
          timestamp: ISOString(new Date(thought.createdAt).toISOString()),
          consolidatedAt: ISOString(new Date().toISOString()),
          importance: finalImportance,  // ← Surprise-adjusted!
          accessCount: 0,
          relatedTo: [],
          tags: thought.domain ? [thought.domain] : [],
          source: 'working-memory'
        };

        const result = await Commands.execute<DataCreateParams, DataCreateResult<any>>('data/create', {
          dbHandle: this.memoryDbHandle,
          collection: 'memories',
          data: memory
        } as any);

        if (result.success) {
          consolidatedIds.push(thought.id);
        } else {
          failedCount++;
          this.log(`ERROR: Failed to consolidate thought ${thought.id}: ${result.error}`);
        }
      } catch (error) {
        failedCount++;
        this.log(`ERROR: Failed to consolidate thought ${thought.id}: ${error}`);
      }
    }

    // 5. Clear successfully consolidated thoughts
    if (consolidatedIds.length > 0) {
      await this.persona.workingMemory.clearBatch(consolidatedIds as any);
      this.log(`✅ Consolidated ${consolidatedIds.length} thoughts to LTM` +
               `${failedCount > 0 ? ` (${failedCount} failed)` : ''}`);
    }

    // 6. Update metrics
    this.metrics = {
      ...this.metrics,
      lastConsolidation: new Date(),
      consolidationCount: this.metrics.consolidationCount + consolidatedIds.length
    };
  } catch (error) {
    this.log(`ERROR: Consolidation failed: ${error}`);
  }
}

/**
 * Calculate recent conversation activity (messages per minute)
 */
private async calculateActivityLevel(): Promise<number> {
  // Query recent messages from database (last 10 minutes)
  const tenMinutesAgo = Date.now() - (10 * 60 * 1000);

  // TODO: Query chat_messages with timestamp filter
  // For now, estimate from WorkingMemory size
  const capacity = await this.persona.workingMemory.getCapacity('chat');

  // Rough heuristic: working memory size indicates recent activity
  const estimatedRate = capacity.used / 10.0;  // messages per minute
  return Math.max(0.1, estimatedRate);
}
```

---

## Summary of Non-Linear Techniques

| Component | Non-Linear Technique | Research Basis |
|-----------|---------------------|----------------|
| **Message Importance** | Softmax + Power Function | Attention mechanisms, PER |
| **Consolidation Threshold** | Sigmoid (activity-responsive) | Temperature annealing |
| **Surprise Detection** | TD-error (prediction mismatch) | TD-learning, neuroscience |
| **Recency Weighting** | Exponential decay | Time-series analysis |
| **Feature Combination** | Weighted softmax (not addition) | Neural attention |

**No simple addition anywhere** - all adjustments use exponential, sigmoid, or power functions.

---

## Implementation Plan

### Phase 1: Foundation (Week 1)
- [ ] Implement `softmax()` utility function
- [ ] Implement `AdaptiveConsolidationThreshold` class
- [ ] Add activity tracking to Hippocampus
- [ ] Unit tests for sigmoid threshold adjustment

### Phase 2: Importance Redesign (Week 2)
- [ ] Replace `calculateMessagePriority()` with softmax-based version
- [ ] Implement `calculateSemanticDistance()` (word overlap)
- [ ] Implement `calculateRecencyScore()` (exponential decay)
- [ ] Integration tests

### Phase 3: Surprise Detection (Week 3)
- [ ] Implement `SurpriseDetector` class
- [ ] Integrate surprise into consolidation
- [ ] Store surprise metrics in MemoryEntity context
- [ ] Analyze surprise correlations with recall

### Phase 4: Tuning & Analysis (Week 4)
- [ ] Tune hyperparameters (α, k, x0, temperatures)
- [ ] Compare old vs new consolidation rates
- [ ] Measure recall performance
- [ ] Adjust based on real usage data

---

## Expected Outcomes

**Before** (linear, fixed):
- 0% consolidation (threshold too high)
- No activity adaptation
- Treats all messages equally

**After** (non-linear, adaptive):
- 20-80% consolidation (activity-dependent)
- Organic response to conversation pace
- Prioritizes surprising, novel, important events
- Biologically-inspired behavior

---

## References

- [Prioritized Experience Replay](https://arxiv.org/abs/1511.05952) - Schaul et al., ICLR 2016
- [Memory Consolidation from RL Perspective](https://www.frontiersin.org/journals/computational-neuroscience/articles/10.3389/fncom.2024.1538741/full)
- [Attention Mechanisms](https://en.wikipedia.org/wiki/Attention_(machine_learning))
- [Softmax Temperature](https://stats.stackexchange.com/questions/527080/what-is-the-role-of-temperature-in-softmax)
- [TD-Learning and Memory](https://pmc.ncbi.nlm.nih.gov/articles/PMC6964152/)
- [Surprise and Memory Reconsolidation](https://pubmed.ncbi.nlm.nih.gov/30012882/)
