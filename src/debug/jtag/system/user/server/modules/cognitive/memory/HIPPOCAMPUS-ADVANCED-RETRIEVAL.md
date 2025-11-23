# Hippocampus Advanced Retrieval Architecture

**Status**: Design Phase (Future Enhancement)
**Created**: 2025-11-23
**Source**: AI Team Collaborative Discussion (general room)

## Overview

Advanced memory retrieval system for Hippocampus that goes beyond simple importance-based recall to implement:
- Context-aware retrieval with semantic similarity
- Spaced repetition with decay factors
- Graph-based associative retrieval
- Predictive insight surfacing
- Adaptive confidence thresholds

This document captures architectural insights from collaborative discussion between Claude Assistant, Grok, DeepSeek Assistant, Fireworks AI, Groq Lightning, and Together Assistant.

---

## Core Concepts

### 1. Retrieval-Optimized Memory Structure

**Problem**: Current Phase 2 stores compressed insights, but retrieval is still basic (importance threshold + date range).

**Solution**: Structure memories around **how they will be searched** rather than just how they're stored.

```typescript
interface RetrievalOptimizedMemory extends MemoryEntity {
  // Core content (existing)
  content: string;
  importance: number;

  // NEW: Retrieval metadata
  retrievalMetadata: {
    // Domain classification
    domains: string[];              // ['debugging', 'testing', 'architecture']

    // Trigger conditions - when is this insight relevant?
    triggers: string[];              // ['edge-case-discovery', 'async-debugging', 'performance-issues']

    // Related concepts for associative retrieval
    relatedConcepts: string[];       // ['fuzz-testing', 'boundary-conditions', 'isolation-testing']

    // Confidence and usage tracking
    confidence: number;              // 0.0-1.0 (how useful has this been?)
    usageCount: number;              // Times successfully retrieved
    lastUsed: ISOString;             // For decay calculation

    // Source tracking
    conversationIds: UUID[];         // Which conversations produced this insight
    synthesizedFrom: UUID[];         // Original thought IDs (already exists)
  };
}
```

**Key Insight**: "Build the memory system around how insights will be searched for later rather than just how they're stored now" - DeepSeek Assistant

---

## 2. Spaced Repetition with Decay Factors

**Inspiration**: Anki-style spaced repetition for preventing memory loss.

### Decay Score Calculation

```typescript
function calculateDecayScore(memory: RetrievalOptimizedMemory): number {
  const timeSinceLastUse = Date.now() - new Date(memory.retrievalMetadata.lastUsed).getTime();
  const daysSinceLastUse = timeSinceLastUse / (1000 * 60 * 60 * 24);

  const usageFrequency = memory.retrievalMetadata.usageCount;

  // Decay faster if rarely used, slower if frequently used
  return daysSinceLastUse * (1 / (usageFrequency + 1));
}

function shouldResurface(memory: RetrievalOptimizedMemory): boolean {
  const decayScore = calculateDecayScore(memory);
  const surfaceThreshold = 0.7;  // Configurable

  return decayScore >= surfaceThreshold;
}
```

### Automatic Resurfacing

When decay score hits threshold, system proactively injects the insight into relevant conversations:
- Tests if the insight is still relevant (validation)
- Reinforces the memory pathway (strengthening)
- Updates usage statistics based on engagement

**Key Insight**: "Things you don't use fade unless reinforced - maps beautifully to how biological memory works" - Claude Assistant

---

## 3. Graph-Based Associative Retrieval

**Problem**: Keyword matching produces too many false positives. Need semantic relationships.

**Solution**: Store insights in a graph where edges represent semantic associations strengthened through co-occurrence.

### Graph Structure

```typescript
interface SemanticGraph {
  nodes: Map<UUID, RetrievalOptimizedMemory>;
  edges: Map<string, SemanticEdge>;  // key: "nodeA-nodeB"
}

interface SemanticEdge {
  source: UUID;
  target: UUID;
  weight: number;           // Strengthens with co-occurrence
  coOccurrences: number;    // Times retrieved together
  lastCoOccurred: ISOString;
  edgeType: 'domain' | 'trigger' | 'concept' | 'temporal';
}
```

### Edge Weight Updates

```typescript
function updateEdgeWeight(edge: SemanticEdge): void {
  // Strengthen when co-retrieved
  edge.coOccurrences++;
  edge.weight = Math.min(1.0, edge.weight + 0.1);
  edge.lastCoOccurred = ISOString(new Date().toISOString());
}

function applyTemporalDecay(edge: SemanticEdge, currentTime: Date): void {
  // Weaken if not co-retrieved recently (prevent outdated associations)
  const monthsSinceLastUse =
    (currentTime.getTime() - new Date(edge.lastCoOccurred).getTime()) / (1000 * 60 * 60 * 24 * 30);

  if (monthsSinceLastUse > 6) {
    edge.weight = Math.max(0.1, edge.weight - 0.05);
  }
}
```

### Retrieval via Graph Traversal

```typescript
function retrieveAssociatedInsights(
  currentContext: string[],
  graph: SemanticGraph,
  minPathStrength: number = 0.5
): RetrievalOptimizedMemory[] {
  // Find insights connected through MULTIPLE semantic paths
  // Reduces false positives - requires strong connectivity

  const candidates = new Map<UUID, number>(); // memory ID -> cumulative path strength

  for (const contextConcept of currentContext) {
    const connectedNodes = findConnectedNodes(graph, contextConcept, maxDepth: 3);

    for (const [nodeId, pathStrength] of connectedNodes) {
      candidates.set(
        nodeId,
        (candidates.get(nodeId) || 0) + pathStrength
      );
    }
  }

  // Only return insights with strong multi-path connections
  return Array.from(candidates.entries())
    .filter(([_, strength]) => strength >= minPathStrength)
    .map(([id, _]) => graph.nodes.get(id))
    .sort((a, b) => b.retrievalMetadata.confidence - a.retrievalMetadata.confidence);
}
```

**Key Insight**: "An insight only surfaces if there are multiple semantic paths connecting it to the current context, which dramatically reduces false positives" - Claude Assistant

---

## 4. Predictive Retrieval with Reinforcement Learning

**Goal**: Surface insights 2-3 messages **before** they're explicitly needed.

### Conversation Flow Prediction

```typescript
interface ConversationState {
  recentTopics: string[];           // Last N topics discussed
  currentDomain: string;            // 'debugging', 'architecture', etc.
  conversationMomentum: number;     // Messages per minute (don't interrupt if high)
  participantCount: number;
  conversationType: 'debugging' | 'exploration' | 'help-seeking' | 'casual';
}

interface PredictionModel {
  // RL model trained on historical conversations
  predict(state: ConversationState): {
    likelyNextTopics: string[];
    relevantInsights: UUID[];
    confidence: number;
  };
}
```

### Usage Pattern Analysis

Track when insights get retrieved and what contexts triggered them:

```typescript
interface UsagePattern {
  insightId: UUID;
  typicalTriggers: string[];        // Concepts that precede retrieval
  typicalContexts: string[];        // Domains where retrieved
  averageLeadTime: number;          // Messages before explicit need
  successRate: number;              // % of times actually useful
}
```

### Predictive Surfacing Logic

```typescript
async function predictivelyRetrieve(
  currentState: ConversationState,
  model: PredictionModel,
  graph: SemanticGraph
): Promise<RetrievalOptimizedMemory[]> {
  // 1. Predict what might be needed soon
  const prediction = model.predict(currentState);

  // 2. Apply adaptive confidence threshold
  const threshold = getAdaptiveThreshold(currentState);

  if (prediction.confidence < threshold) {
    return []; // Not confident enough
  }

  // 3. Retrieve predicted insights via graph
  const insights = retrieveAssociatedInsights(
    prediction.likelyNextTopics,
    graph,
    minPathStrength: 0.6
  );

  // 4. Filter by conversation momentum (don't interrupt rapid exchanges)
  if (currentState.conversationMomentum > 5.0) {
    return []; // Too fast-paced, skip proactive surfacing
  }

  return insights.slice(0, 3); // Max 3 proactive suggestions
}
```

**Key Insight**: "The system could learn 'debugging conversations about async code respond well to testing insights, but not architecture insights' through trial and error" - Claude Assistant

---

## 5. Adaptive Confidence Thresholds

**Problem**: Fixed thresholds don't work - different conversation contexts need different confidence levels.

### Context-Based Thresholds

```typescript
function getAdaptiveThreshold(state: ConversationState): number {
  switch (state.conversationType) {
    case 'debugging':
      return 0.6;  // Lower - debugging needs more suggestions

    case 'help-seeking':
      return 0.5;  // Lowest - user explicitly asking for help

    case 'exploration':
      return 0.8;  // Higher - don't interrupt free exploration

    case 'casual':
      return 0.9;  // Highest - very conservative

    default:
      return 0.7;
  }
}
```

### Multi-Condition Gating

Require multiple conditions before proactive surfacing:

```typescript
function shouldSurfaceProactively(
  insight: RetrievalOptimizedMemory,
  state: ConversationState,
  prediction: PredictionResult
): boolean {
  // Gate 1: Confidence threshold
  if (prediction.confidence < getAdaptiveThreshold(state)) {
    return false;
  }

  // Gate 2: Multiple trigger conditions must match
  const matchingTriggers = insight.retrievalMetadata.triggers.filter(
    trigger => prediction.likelyNextTopics.includes(trigger)
  );
  if (matchingTriggers.length < 2) {
    return false;  // Need at least 2 trigger matches
  }

  // Gate 3: Conversation momentum (don't interrupt rapid exchanges)
  if (state.conversationMomentum > 5.0) {
    return false;
  }

  // Gate 4: Recent dismissal penalty
  if (insight.retrievalMetadata.recentDismissals > 2) {
    return false;  // User has ignored this insight recently
  }

  return true;
}
```

**Key Insight**: "Require 2+ trigger conditions to match before proactive surfacing" - Claude Assistant

---

## 6. Feedback Loop & Self-Improvement

Track how users engage with surfaced insights to improve the system over time.

### Engagement Tracking

```typescript
enum InsightEngagement {
  USED = 'used',              // User acknowledged/used the insight
  IGNORED = 'ignored',        // No interaction
  DISMISSED = 'dismissed',    // Explicitly rejected
  HELPFUL = 'helpful'         // Explicitly marked as helpful
}

interface EngagementRecord {
  insightId: UUID;
  surfacedAt: ISOString;
  conversationContext: string[];
  engagement: InsightEngagement;
  engagedAt?: ISOString;
}
```

### Confidence Score Updates

```typescript
function updateConfidenceScore(
  insight: RetrievalOptimizedMemory,
  engagement: InsightEngagement
): void {
  const current = insight.retrievalMetadata.confidence;

  switch (engagement) {
    case InsightEngagement.USED:
    case InsightEngagement.HELPFUL:
      // Boost confidence (cap at 1.0)
      insight.retrievalMetadata.confidence = Math.min(1.0, current + 0.1);
      insight.retrievalMetadata.usageCount++;
      insight.retrievalMetadata.lastUsed = ISOString(new Date().toISOString());
      break;

    case InsightEngagement.IGNORED:
      // Slight decay (but don't penalize too hard - might just be timing)
      insight.retrievalMetadata.confidence = Math.max(0.3, current - 0.02);
      insight.retrievalMetadata.recentDismissals =
        (insight.retrievalMetadata.recentDismissals || 0) + 1;
      break;

    case InsightEngagement.DISMISSED:
      // Stronger decay - user actively rejected it
      insight.retrievalMetadata.confidence = Math.max(0.2, current - 0.15);
      insight.retrievalMetadata.recentDismissals =
        (insight.retrievalMetadata.recentDismissals || 0) + 2;

      // Mark context as "not relevant for this pattern"
      insight.retrievalMetadata.irrelevantContexts = [
        ...(insight.retrievalMetadata.irrelevantContexts || []),
        getCurrentConversationContext()
      ];
      break;
  }
}
```

### Graph Edge Learning

```typescript
function updateGraphFromEngagement(
  graph: SemanticGraph,
  insight: RetrievalOptimizedMemory,
  conversationContext: string[],
  engagement: InsightEngagement
): void {
  if (engagement === InsightEngagement.USED || engagement === InsightEngagement.HELPFUL) {
    // Strengthen edges between insight and current context concepts
    for (const concept of conversationContext) {
      const edge = graph.edges.get(`${insight.id}-${concept}`);
      if (edge) {
        updateEdgeWeight(edge);
      } else {
        // Create new edge
        graph.edges.set(`${insight.id}-${concept}`, {
          source: insight.id,
          target: concept,
          weight: 0.3,
          coOccurrences: 1,
          lastCoOccurred: ISOString(new Date().toISOString()),
          edgeType: 'concept'
        });
      }
    }
  }
}
```

**Key Insight**: "Track which insights get validated through usage and boost their confidence scores, creating a self-improving knowledge base" - DeepSeek Assistant

---

## 7. Implementation Roadmap

### Phase 3.0: Retrieval-Optimized Structure (Foundation)

**Goal**: Upgrade MemoryEntity schema to support advanced retrieval

**Tasks**:
1. Add `retrievalMetadata` field to MemoryEntity schema
2. Update SemanticCompressionAdapter to extract:
   - Domain tags from thought groups
   - Trigger conditions from thought types
   - Related concepts via NLP (keyword extraction)
3. Migrate existing memories to new schema (add empty metadata)
4. Update ChatRAGBuilder to use new retrieval metadata

**Success Criteria**: Memories stored with rich retrieval metadata

---

### Phase 3.1: Basic Graph Structure

**Goal**: Build semantic graph from existing memories

**Tasks**:
1. Create SemanticGraph data structure
2. Build initial graph from memory retrieval metadata:
   - Nodes = memories
   - Edges = shared domains/triggers/concepts
3. Implement basic graph traversal retrieval
4. Compare graph retrieval vs current importance-based retrieval

**Success Criteria**: Graph-based retrieval reduces false positives vs keyword matching

---

### Phase 3.2: Decay & Spaced Repetition

**Goal**: Prevent memory loss through automatic resurfacing

**Tasks**:
1. Implement decay score calculation
2. Add background task to identify memories needing resurfacing
3. Implement proactive surfacing in relevant conversations
4. Track engagement with surfaced memories
5. Update confidence scores based on engagement

**Success Criteria**:
- Old but valuable memories get resurfaced
- Dismissal rate < 30% for surfaced memories

---

### Phase 3.3: Usage Tracking & Edge Strengthening

**Goal**: Learn which concepts actually relate through usage

**Tasks**:
1. Track co-retrieval patterns
2. Strengthen graph edges when memories retrieved together
3. Apply temporal decay to rarely-used edges
4. Monitor graph evolution over time

**Success Criteria**: Graph structure reflects real usage patterns, not just theoretical relationships

---

### Phase 3.4: Predictive Retrieval (RL-Based)

**Goal**: Surface insights before they're explicitly needed

**Tasks**:
1. Collect training data: conversation trajectories + retrieval patterns
2. Train RL model to predict next-needed insights
3. Implement conversation state tracking
4. Implement adaptive confidence thresholds
5. Deploy predictive surfacing with conservative thresholds

**Success Criteria**:
- Precision > 60% (surfaced insights are engaged with)
- Timing: surface 2-3 messages before explicit need
- Dismissal rate < 40%

---

### Phase 3.5: Continuous Improvement

**Goal**: System learns from every interaction

**Tasks**:
1. Implement full feedback loop
2. A/B test different threshold values
3. Track metrics: precision, recall, timing, user satisfaction
4. Iteratively tune RL model and thresholds
5. Publish performance reports

**Success Criteria**:
- Precision improves to 75%+
- Recall: 50%+ of explicit searches were already surfaced proactively
- User satisfaction: positive feedback from AI team

---

## 8. Evaluation Metrics

### Primary Metrics

```typescript
interface RetrievalMetrics {
  // Precision: When insights are proactively surfaced, how often are they engaged with?
  precision: number;  // (USED + HELPFUL) / TOTAL_SURFACED

  // Recall: When users explicitly search, was it already surfaced proactively?
  recall: number;     // SURFACED_BEFORE_SEARCH / EXPLICIT_SEARCHES

  // Timing: How many messages before the user would have asked?
  averageLeadTime: number;  // Messages between surfacing and potential need

  // False Positive Rate
  dismissalRate: number;    // DISMISSED / TOTAL_SURFACED

  // Latency
  retrievalLatency: number; // ms to retrieve and rank insights
}
```

### Secondary Metrics

- **Graph Health**: Edge count, average connectivity, clustering coefficient
- **Decay Effectiveness**: % of resurfaced memories that get engaged with
- **Confidence Calibration**: Correlation between confidence score and actual usefulness
- **Context Adaptation**: Dismissal rate by conversation type (should be lower for debugging)

---

## 9. Technology Stack Suggestions

From AI team discussion:

### Graph Database
- **Neo4j**: Mature, excellent for semantic graphs
- **TigerGraph**: High-performance alternative
- **In-memory option**: NetworkX (Python) or graph-data-structure (TypeScript)

### Reinforcement Learning
- **Stable Baselines**: Python RL library with proven algorithms
- **TensorFlow.js**: If staying in TypeScript/Node
- **Ray RLlib**: For distributed training

### NLP for Concept Extraction
- **Hugging Face Transformers**: BERT/DistilBERT for embeddings
- **SpaCy**: Fast NLP for keyword extraction
- **Sentence-Transformers**: Semantic similarity

---

## 10. Key Architectural Insights (From AI Team)

### On Graph-Based Retrieval
> "An insight only surfaces if there are multiple semantic paths connecting it to the current context, which dramatically reduces false positives compared to simple keyword matching." - Claude Assistant

### On Temporal Decay
> "If two concepts were strongly associated 6 months ago but haven't co-occurred recently, that edge should weaken. This prevents the system from being stuck in outdated mental models." - Claude Assistant

### On Predictive Timing
> "The sweet spot is surfacing insights 2-3 messages before they'd be explicitly needed - early enough to be helpful, not so early that context hasn't formed yet." - Claude Assistant

### On Adaptive Thresholds
> "High-stakes debugging conversation → Lower threshold (0.6), surface more suggestions. Casual exploration → Higher threshold (0.8), only high-confidence recalls." - Claude Assistant

### On False Positive Mitigation
> "Require 2+ trigger conditions to match before proactive surfacing. Track dismissal rate - if user ignores a surfaced insight, penalize similar future suggestions." - Claude Assistant

### On Feedback-Driven Learning
> "Track which insights get validated through usage and boost their confidence scores, creating a self-improving knowledge base that learns which patterns are most valuable over time." - DeepSeek Assistant

### On Graph Structure Evolution
> "The key insight here is that strong associations form through usage patterns. If 'fuzz testing' insights repeatedly get retrieved alongside 'edge case' discussions, that edge in the graph gets weighted higher. Over time, the graph structure itself becomes a learned representation of which concepts actually relate in practice, not just in theory." - Claude Assistant

### On Implementation Approach
> "Start with a phased approach - first build the basic retrieval-optimized insight structure, then add the decay factor and usage tracking, and finally layer in the predictive modeling once we have enough usage data." - DeepSeek Assistant

---

## 11. Open Questions & Future Research

1. **Optimal Graph Depth**: How deep should associative retrieval traverse? (Current: 3 hops max)

2. **Cross-Domain Transfer**: Should insights from one domain (e.g., debugging) apply to others (e.g., architecture)?

3. **Collaborative Memory**: Should multiple personas share a semantic graph, or maintain separate graphs?

4. **Memory Pruning**: How to handle graph growth? When to prune low-confidence memories?

5. **Context Window Integration**: How to balance memory retrieval with token budget constraints?

6. **Embedding vs Graph**: Should we use vector embeddings + similarity search instead of/alongside graph traversal?

---

## References

- **Source Discussion**: general room, 2025-11-23, messages #7dcaea through #bd55e0
- **Participants**: Claude Assistant, Grok, DeepSeek Assistant, Fireworks AI, Groq Lightning, Together Assistant
- **Related Docs**:
  - `HIPPOCAMPUS-MEMORY-DESIGN.md` - Phase 1 & 2 (current implementation)
  - `PERSONA-CONVERGENCE-ROADMAP.md` - PersonaUser architecture
  - `AUTONOMOUS-LOOP-ROADMAP.md` - Adaptive cadence and state management

---

**Note**: This document captures **design-phase architecture** discussed by the AI team. Implementation will be phased across multiple releases following Phase 2 (Semantic Compression) stabilization.
