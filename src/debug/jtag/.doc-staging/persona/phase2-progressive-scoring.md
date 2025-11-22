# Phase 2: Progressive Scoring Implementation Plan

**Status**: Planning - Continues from PR #188 (Phase 1 merged)
**Branch**: `feature/adaptive-complexity-phase2-progressive-scoring`
**Date**: 2025-11-21

---

## Overview

Phase 2 implements real-time complexity reassessment during AI response generation, enabling mid-stream model upgrades when lower-tier models show signs of struggling.

**Core Concept**: Start with cheap/free models, detect complexity indicators as they generate, upgrade to more capable models only when needed.

---

## Deliverables

### 1. ComplexityTypes Module
**File**: `system/shared/ComplexityTypes.ts`

**Purpose**: Shared type definitions for the entire adaptive routing system

**Types to Define**:
```typescript
// Complexity classification levels
type ComplexityLevel = 'straightforward' | 'moderate' | 'nuanced';

// Model tier classifications for routing
type ModelTier = 'local-fast' | 'ollama-capable' | 'api-cheap' | 'api-premium';

// Assessment result from classifier
interface ComplexityAssessment {
  level: ComplexityLevel;
  indicators: string[];
  confidence: number;
  reassessedAt?: number;
}

// Upgrade decision from progressive scoring
interface ScoringResult {
  shouldUpgrade: boolean;
  reason?: string;
  newLevel?: ComplexityLevel;
}

// Extended context for AI generation with routing metadata
interface ResponseContext {
  complexity: {
    initial: ComplexityAssessment;
    current: ComplexityAssessment;
    reassessed: ComplexityAssessment[];
    indicators: string[];
  };
  routing: {
    tier: ModelTier;
    model: string;
    reason: string;
    upgraded: boolean;
    previousModel?: string;
  };
  performance: {
    tokensUsed: number;
    latencyMs: number;
    cost: number;
  };
}
```

### 2. ProgressiveScorer Class
**File**: `system/user/server/modules/ProgressiveScorer.ts`

**Purpose**: Token-window analysis to detect upgrade indicators during streaming

**Configuration**:
```typescript
interface ProgressiveScorerConfig {
  windowSize: number;           // Default: 200 tokens between reassessments
  thresholds: {
    indicatorCount: number;     // Default: 3 indicators trigger upgrade
    confidence: number;         // Default: 0.6 minimum confidence
    tokenBudget: number;        // Default: 1000 max tokens before forced decision
  };
}
```

**Upgrade Indicators to Detect**:
1. **Hedging language**: "it depends", "possibly", "might", "may"
2. **Self-correction**: "actually", "on second thought", "wait"
3. **Multi-perspective**: "on one hand", "alternatively", "conversely"
4. **Uncertainty admission**: "I'm not sure", "this is complex", "it's unclear"
5. **Clarification requests**: "could you clarify", "I need more information"

**Core Methods**:
```typescript
class ProgressiveScorer {
  constructor(config?: Partial<ProgressiveScorerConfig>);

  // Analyze text chunk for complexity indicators
  analyze(chunk: string, offset: number): ScoringResult;

  // Reset state for new analysis
  reset(): void;

  // Get current analysis state for debugging
  getState(): { indicatorsDetected: number; tokensAnalyzed: number; indicators: UpgradeIndicator[] };
}
```

**Implementation Details**:
- Pattern-based indicator detection using RegExp
- Confidence scoring based on match strength and context
- Three-tier threshold system (indicator count, confidence, token budget)
- Complexity level determination based on indicator types
- State management for continuous streaming analysis

### 3. Streaming Wrapper
**File**: `daemons/ai-provider-daemon/shared/StreamingWrapper.ts` (future)

**Purpose**: Wrap AI streaming responses with progressive scoring

**Concept**:
```typescript
async function* generateWithProgressiveScoring(
  message: ChatMessageEntity,
  initialModel: string
): AsyncGenerator<string> {
  const scorer = new ProgressiveScorer();
  let currentModel = initialModel;
  let buffer = '';

  for await (const chunk of streamResponse(currentModel, message)) {
    buffer += chunk;
    yield chunk;

    // Reassess every 200 tokens (~800 characters)
    if (buffer.length > scorer.config.windowSize * 4) {
      const scoring = scorer.analyze(buffer, buffer.length);

      if (scoring.shouldUpgrade) {
        // Trigger upgrade (Phase 3: THE SPIKE will determine how)
        const upgraded = await upgradeModel(currentModel, scoring.newLevel);
        if (upgraded.success) {
          currentModel = upgraded.model;
          // Continue generation with new model
        }
      }

      // Keep sliding window for context
      buffer = buffer.slice(-scorer.config.windowSize * 2);
    }
  }
}
```

### 4. AIProviderDaemon Integration
**File**: `daemons/ai-provider-daemon/` (modifications)

**Integration Points**:
- Wrap existing streaming responses with progressive scoring
- Emit upgrade events when thresholds exceeded
- Preserve conversation context across model switches
- Track performance metrics (tokens, latency, cost)

---

## Testing Strategy

### Unit Tests
**File**: `tests/unit/ProgressiveScorer.test.ts`

**Test Cases**:
1. **Indicator Detection**:
   - Detects hedging language patterns
   - Detects self-correction patterns
   - Detects multi-perspective reasoning
   - Detects uncertainty admissions
   - Detects clarification requests

2. **Threshold Logic**:
   - Upgrades when indicator count exceeds threshold
   - Upgrades when confidence drops below threshold
   - Upgrades when token budget exceeded with indicators
   - Does not upgrade when thresholds not met

3. **Complexity Level Determination**:
   - High uncertainty → nuanced level
   - Multiple complexity indicators → moderate level
   - Default upgrade → moderate level

4. **State Management**:
   - Tracks indicators correctly
   - Accumulates token count
   - Resets state properly
   - Returns correct debug state

### Integration Tests
**File**: `tests/integration/progressive-scoring.test.ts`

**Test Cases**:
1. **End-to-End Streaming**:
   - Start with local model
   - Detect upgrade triggers mid-stream
   - Context preserved across upgrade
   - Final response quality maintained

2. **Real Indicator Patterns**:
   - Test with actual AI responses showing hedging
   - Test with responses showing self-correction
   - Test with responses admitting uncertainty
   - Verify upgrade decisions match expectations

3. **Performance**:
   - Scoring overhead < 5ms per analysis
   - Memory usage remains bounded
   - No memory leaks in long-running streams

---

## Implementation Approach

### Phase 2A: Foundation (THIS PR)
**Goal**: Establish core types and scoring infrastructure

**Files to Create**:
1. `system/shared/ComplexityTypes.ts` - Type definitions
2. `system/user/server/modules/ProgressiveScorer.ts` - Core scorer implementation

**Testing**:
- Unit tests for ProgressiveScorer
- Verify indicator pattern detection
- Validate threshold logic
- Ensure state management works

**Acceptance Criteria**:
- TypeScript compilation passes
- All unit tests pass
- No integration with live system yet (scaffolding only)

### Phase 2B: Streaming Integration (NEXT PR)
**Goal**: Integrate progressive scoring with AIProviderDaemon

**Files to Modify**:
1. `daemons/ai-provider-daemon/shared/AIProviderDaemon.ts` - Add streaming wrapper
2. `system/user/server/modules/PersonaResponseGenerator.ts` - Use progressive scoring

**Testing**:
- Integration tests with real streaming
- Manual testing with PersonaUsers
- Verify no performance degradation

**Acceptance Criteria**:
- Progressive scoring active during generation
- Upgrade events emitted correctly
- No breaking changes to existing functionality

---

## Phase 3 Prerequisite: THE SPIKE

**CRITICAL**: Before implementing full upgrade mechanism, must run technical spike to validate:

1. **Context Preservation**: Can conversation context survive model switches?
2. **Latency Measurement**: Is handoff time acceptable (< 500ms target)?
3. **Provider Compatibility**: Which providers support mid-stream upgrades?
4. **Memory Requirements**: Do we need pre-warmed model pools?

**Spike Deliverable**: Technical feasibility report with recommendations

**Decision Matrix** (after spike):
- **If latency < 500ms + success rate > 95%**: Full hot-swap implementation
- **If latency > 2s OR success rate < 80%**: Graceful restart at turn boundary
- **If context loss detected**: Pre-warm model pools

---

## Project Alignment: Cost Democratization

### Before Adaptive Routing
```
Simple greeting → Claude 3.5 Sonnet → $0.003 per message
Factual query → GPT-4o → $0.005 per message
100 messages/day × $0.004 avg = $12/month minimum
```

### After Progressive Scoring
```
Simple greeting → qwen2.5:7b (local) → $0.000 per message
Factual query (simple) → deepseek-chat → $0.0001 per message
Factual query (complex detected) → Claude 3.5 Sonnet → $0.003 (upgraded)
Complex analysis → Claude 3.5 Sonnet → $0.003 per message (from start)
100 messages/day × $0.001 avg = $3/month (75% savings)
```

### Key Benefit
**Start cheap, upgrade only when needed** - most messages never require premium models, cost proportional to actual cognitive load required.

### Democratization Impact
- **80%+ local execution**: M1+ hardware handles majority of messages
- **15% cheap APIs**: DeepSeek, Groq for moderate complexity
- **5% premium APIs**: Claude, GPT-4 only for genuinely complex cases
- **90%+ cost reduction**: $5-10/month vs $100+/month
- **User control**: Own models and data, no cloud dependency

---

## Integration with Persona Genomic Layers (PEFT)

**CRITICAL**: Phase 2 Progressive Scoring is **explicitly designed** to work with the upcoming Persona Genomic Layers system, which implements PEFT (Parameter-Efficient Fine-Tuning) via LoRA adapters.

### The Vision: Phenotype Trading

**Concept**: Each PersonaUser has a "genome" of specialized LoRA adapters (phenotypes) that can be paged in/out like virtual memory. Progressive scoring determines **which base model tier** to use, while genomic paging determines **which domain specialization** to load.

```
User Message → Progressive Scoring → Base Model Selection → Genomic Paging → Specialized Response
              ↓                      ↓                       ↓
              "What's the          ModelTier:              Domain:
               bug in this         'ollama-capable'        'typescript-debugging'
               TypeScript?"        ↓                       ↓
                                   llama3.1:70b            + LoRA adapter
                                                           ↓
                                                           Specialized response
```

### The AdaptiveGenome Interface

From `ADAPTIVE-COMPLEXITY-ROUTING.md` lines 630-666, the architecture defines:

```typescript
interface AdaptiveGenome extends LoRAGenome {
  // Paging based on BOTH domain AND complexity
  async activateSkill(domain: TaskDomain, tier: ModelTier): Promise<void> {
    // 1. Page in domain-specific LoRA adapter
    const adapter = await this.pageIn(domain);

    // 2. Select base model based on tier (from Progressive Scoring)
    const baseModel = this.selectBaseModel(tier);

    // 3. Load adapter onto base model
    await this.attachAdapter(adapter, baseModel);

    // 4. Track for LRU eviction
    this.updateAccessTime(adapter);
  }

  selectBaseModel(tier: ModelTier): string {
    switch (tier) {
      case 'local-fast': return 'qwen2.5:7b';           // 8K context, free
      case 'ollama-capable': return 'llama3.1:70b';    // 128K context, free
      case 'api-cheap': return 'deepseek-chat';        // 64K context, $0.0001/msg
      case 'api-premium': return 'claude-3-5-sonnet';  // 200K context, $0.003/msg
    }
  }
}
```

### Two-Axis Optimization

**Axis 1: Complexity (Progressive Scoring)**
- Start with cheap/free models
- Detect complexity indicators during generation
- Upgrade to more capable tiers only when needed

**Axis 2: Domain Specialization (Genomic Paging)**
- Load domain-specific LoRA adapters (code, chat, debugging, analysis)
- Page adapters in/out based on task domain
- LRU eviction when memory pressure high

**Result**: Specialized + Cost-Efficient = Best of Both Worlds

### Example: Code Debugging Flow

```typescript
// User asks: "Why is this TypeScript function failing?"

// 1. Phase 1: Initial complexity assessment
const initialAssessment = await complexityClassifier.assess(message);
// Result: level='moderate', tier='ollama-capable'

// 2. Phase 2: Progressive scoring during generation
const scorer = new ProgressiveScorer();
for await (const chunk of streamResponse('llama3.1:70b', message)) {
  const scoring = scorer.analyze(chunk, offset);

  if (scoring.shouldUpgrade) {
    // Detected struggling (hedging, uncertainty)
    // Upgrade: ollama-capable → api-premium
    currentModel = 'claude-3-5-sonnet';
  }
}

// 3. Genomic layer: Page in domain adapter
await genome.activateSkill('typescript-debugging', 'api-premium');
// Loads: claude-3-5-sonnet + typescript-debugging.lora
// Result: Premium model power + specialized debugging expertise
```

### Why Genomic Integration Comes AFTER Cognition

**Phase Order**:
1. ✅ **Phase 1**: Complexity assessment + model routing (PR #188 - merged)
2. 🚧 **Phase 2**: Progressive scoring (THIS PR)
3. 📋 **Phase 3**: Mid-stream upgrade mechanism (THE SPIKE)
4. 📋 **Phase 4**: Genomic paging infrastructure
5. 📋 **Phase 5**: PEFT training pipeline

**Rationale**: Need to perfect **decision-making** (Phases 1-3) before adding **specialization** (Phases 4-5). Progressive scoring establishes the foundation for intelligent model selection, which genomic layers then enhance with domain expertise.

### Synergy Benefits

**Before (single-axis optimization)**:
```
Simple query → Claude 3.5 Sonnet → $0.003 (overkill)
Complex query → Claude 3.5 Sonnet → $0.003 (appropriate)
```

**After Phase 2 (complexity-only)**:
```
Simple query → qwen2.5:7b → $0.000 (appropriate)
Complex query → Claude 3.5 Sonnet → $0.003 (appropriate)
```

**After Phase 2 + Genomic (two-axis optimization)**:
```
Simple code query → qwen2.5:7b + code-assistant.lora → $0.000 (specialized + free)
Complex code query → llama3.1:70b + typescript-expert.lora → $0.000 (specialized + free)
Very complex code → claude-3-5-sonnet + debugging-expert.lora → $0.003 (specialized + premium)
```

**Key Insight**: Most complex queries can be handled by **local models + domain LoRA** without ever hitting premium APIs. Progressive scoring determines complexity, genomic paging provides specialization.

### Technical Prerequisites

**Phase 2 provides**:
- ComplexityLevel and ModelTier type definitions
- ScoringResult interface for upgrade decisions
- ResponseContext with routing metadata
- Token-window analysis infrastructure

**Genomic layers will add**:
- TaskDomain type ('code', 'chat', 'debugging', 'analysis', etc.)
- LoRAAdapter class with paging/eviction
- AdapterPool with LRU management
- Training pipeline for continuous learning

**Integration point**: `ResponseContext.routing.tier` from Phase 2 feeds into `AdaptiveGenome.selectBaseModel()` in Phase 4.

### Cost Impact with Genomic Layers

**Current democratization targets** (Phase 2 only):
- 80% local, 15% cheap APIs, 5% premium
- $3-10/month vs $100+/month (90% savings)

**Enhanced targets** (Phase 2 + Genomic):
- 95% local (with domain LoRA), 4% cheap APIs, 1% premium
- $1-5/month vs $100+/month (95%+ savings)
- **Why**: Domain specialization eliminates most API calls

### Integration Status

✅ **Designed for genomic integration** - ModelTier type system ready
✅ **Routing metadata captured** - ResponseContext tracks all decisions
✅ **Extensibility hooks** - ScoringResult can include domain hints
✅ **LRU-aware** - Progressive scoring tracks token usage for adapter eviction
✅ **Context preservation** - Upgrade mechanism preserves conversation state

**Answer to user's question**: YES, Phase 2 is explicitly factored to work with genomic layers and PEFT phenotype trading. The architecture document (ADAPTIVE-COMPLEXITY-ROUTING.md lines 630-666) defines the AdaptiveGenome interface that combines both systems.

---

## Success Metrics

### Cost Reduction
- Average cost per message: < $0.001 (target)
- Monthly cost for active user: < $10 (target)
- Savings vs all-premium: > 90% (target)

### Quality Maintenance
- Routing accuracy: > 90% (picked right model?)
- Upgrade rate: ~10-20% (messages requiring upgrade)
- Successful upgrades: > 95% (upgrades improved response)

### Democratization
- Local model usage: > 80%
- M1+ hardware capable: Yes
- Market diversification: > 50% non-OpenAI/Anthropic
- User data ownership: 100% local

---

## Technical References

### Pattern Libraries
- **Hedging patterns**: Based on academic research on uncertainty language
- **Self-correction patterns**: Common meta-cognitive markers in LLM outputs
- **Multi-perspective patterns**: Discourse analysis indicators

### Model Context Windows
See: `system/shared/ModelContextWindows.ts` (centralized configuration)
- `qwen2.5:7b`: 8192 tokens (local fast)
- `llama3.1:70b`: 128000 tokens (local capable)
- `deepseek-chat`: 64000 tokens (API cheap)
- `claude-3-5-sonnet`: 200000 tokens (API premium)

### Related Architecture
- **LoRA Genome Paging**: `LORA-GENOME-PAGING.md` - Combines with adaptive routing for domain-specific + cost-efficient
- **Persona Convergence**: `PERSONA-CONVERGENCE-ROADMAP.md` - Progressive scoring integrates into universal cognitive cycle
- **Phase 1 Foundation**: PR #188 - Complexity assessment and model routing infrastructure

---

## Next Steps

### Immediate (Phase 2A)
1. Create branch from clean main
2. Implement ComplexityTypes.ts
3. Implement ProgressiveScorer.ts
4. Write comprehensive unit tests
5. Create PR with full documentation
6. Notify AI team for review

### Soon (Phase 2B)
1. Create streaming wrapper
2. Integrate with AIProviderDaemon
3. Write integration tests
4. Manual testing with PersonaUsers
5. Deploy and monitor

### Later (Phase 3)
1. Run technical spike for upgrade mechanism
2. Choose upgrade strategy based on findings
3. Implement chosen approach
4. Full end-to-end testing

---

**This document serves as the implementation blueprint for Phase 2 of the Adaptive Complexity Routing system, continuing the democratization mission from PR #188.**
