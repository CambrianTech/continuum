# Decision Adapter Implementation Plan

**Status**: SUPERSEDED by two-layer cognition architecture
**Date**: 2025-11-16
**Last Updated**: 2025-11-16

---

## ⚠️ Architecture Pivot

**Original plan**: Extract decision logic into adapters (FastPathDecisionAdapter, LLMDecisionAdapter, etc.).

**New understanding**: Decision adapters are a good pattern, but they're solving a symptom. The real problem is AIs have no persistent self-awareness or working memory.

**NEW PRIORITY**: Build two-layer cognition architecture FIRST:
1. **Universal Self-State** - Persistent awareness (current focus, cognitive load, preoccupations)
2. **Domain Working Memory** - Database-backed thought storage per activity

**When to revisit this**: After working memory exists, decision adapters become the "perception" layer in domain cognitive adapters. The `shouldEngageWith()` universal gate will use adapter pattern but with access to self-state and working memory.

**See `COGNITION-ARCHITECTURE.md` for complete design.**

---

## Context (Original Plan - Still Valid Pattern)

**Problem**: PersonaUser.evaluateShouldRespond() has hardcoded decision logic mixing fast-path (mention check) and LLM evaluation. This breaks cognitive architecture abstraction.

**Solution**: Extract decision strategies into adapters using Chain of Responsibility pattern.

**Status**:
- ✅ Bug fix deployed (fast-path now captures RAG context for decision logging)
- ✅ IDecisionAdapter interface created
- 🚧 Adapters to implement (DEFERRED until after working memory)

---

## Architecture

### Domain-Agnostic Design

**CRITICAL**: Cognition module must work across ALL domains (chat, game, code, web). Using `ChatMessageEntity` locks PersonaUser into chat forever.

**Solution**: Use `BaseEntity` or generic `<TEvent extends BaseEntity>` so cognition works with:
- `ChatMessageEntity` (chat messages)
- `GameEventEntity` (player actions, AI turns)
- `CodeReviewEntity` (PR comments, file changes)
- `WebInteractionEntity` (clicks, form submissions)

### Chain of Responsibility Pattern

```
PersonaUser.evaluateShouldRespond()
  ↓
  Try FastPathDecisionAdapter
    ├─ isMentioned? → Return decision
    └─ Not mentioned → Return null (try next)
  ↓
  Try LLMDecisionAdapter
    ├─ Run LLM gating → Return decision
    └─ Error → Return null (try next)
  ↓
  Default: SILENT decision
```

### Adapter Flow (Domain-Agnostic)

```typescript
// In PersonaUser.evaluateShouldRespond()
const context: DecisionContext<TEvent> = {
  triggerEvent,              // BaseEntity (works for any domain!)
  eventContent: string,      // Extracted text/description
  personaId: this.id,
  personaDisplayName: this.displayName,
  senderIsHuman,
  isMentioned,
  gatingModel: this.entity?.personaConfig?.gatingModel,
  contextWindowMinutes: this.entity?.personaConfig?.contextWindowMinutes,
  minContextMessages: this.entity?.personaConfig?.minContextMessages
};

// Try each adapter in order
for (const adapter of this.decisionAdapters) {
  const decision = await adapter.evaluate(context);
  if (decision !== null) {
    return decision; // Adapter handled it
  }
}

// Default: SILENT
return {
  shouldRespond: false,
  confidence: 1.0,
  reason: 'No adapter could evaluate',
  model: 'default'
};
```

---

## Implementation Steps

### Step 1: Create FastPathDecisionAdapter (~100 lines)

**File**: `system/user/server/modules/cognition/adapters/FastPathDecisionAdapter.ts`

**Responsibilities**:
- Check if persona is mentioned by name
- If mentioned → build RAG context, return RESPOND decision
- If not mentioned → return null (try next adapter)

**Key Code**:
```typescript
export class FastPathDecisionAdapter implements IDecisionAdapter {
  readonly name = 'fast-path';

  constructor(
    private personaNames: string[],  // Display name + aliases
    private buildRAGContext: (context: DecisionContext) => Promise<any>
  ) {}

  async evaluate(context: DecisionContext): Promise<CognitiveDecision | null> {
    if (!context.isMentioned) {
      return null; // Not mentioned, try next adapter
    }

    // Build RAG context for decision logging
    const ragContext = await this.buildRAGContext(context);

    return {
      shouldRespond: true,
      confidence: 0.95 + Math.random() * 0.04,
      reason: 'Directly mentioned by name',
      model: 'fast-path',
      filteredRagContext: ragContext
    };
  }
}
```

**Extract from**: PersonaUser.ts lines 1889-1944

---

### Step 2: Create LLMDecisionAdapter (~150 lines)

**File**: `system/user/server/modules/cognition/adapters/LLMDecisionAdapter.ts`

**Responsibilities**:
- Build RAG context with time window filtering
- Call AIDecisionService.evaluateGating()
- Return RESPOND or SILENT decision based on LLM evaluation

**Key Code**:
```typescript
export class LLMDecisionAdapter implements IDecisionAdapter {
  readonly name = 'llm-gating';

  constructor(
    private personaId: UUID,
    private personaDisplayName: string
  ) {}

  async evaluate(context: DecisionContext): Promise<CognitiveDecision | null> {
    try {
      // Build RAG context
      const ragBuilder = new ChatRAGBuilder();
      const rawRagContext = await ragBuilder.buildContext(
        context.message.roomId,
        context.personaId,
        {
          maxMessages: 20,
          maxMemories: 0,
          includeArtifacts: false,
          includeMemories: false,
          currentMessage: {
            role: 'user',
            content: context.messageText,
            name: context.message.senderName,
            timestamp: context.message.timestamp
          }
        }
      );

      // Filter by time window and minimum message count
      const filteredRagContext = this.filterContextByTimeWindow(
        rawRagContext,
        context.contextWindowMinutes ?? 30,
        context.minContextMessages ?? 15
      );

      // Use AIDecisionService for LLM evaluation
      const decisionContext: AIDecisionContext = {
        personaId: context.personaId,
        personaName: context.personaDisplayName,
        roomId: context.message.roomId,
        triggerMessage: context.message,
        ragContext: filteredRagContext
      };

      const result = await AIDecisionService.evaluateGating(decisionContext, {
        model: context.gatingModel ?? 'llama3.2:3b',
        temperature: 0.3
      });

      return {
        shouldRespond: result.shouldRespond,
        confidence: result.confidence,
        reason: result.reason,
        model: result.model,
        filteredRagContext
      };
    } catch (error) {
      console.error(`❌ LLMDecisionAdapter: Evaluation failed:`, error);
      return null; // Error, try next adapter
    }
  }
}
```

**Extract from**: PersonaUser.ts lines 1947-2068

---

### Step 3: Wire Adapters into PersonaUser

**Changes to PersonaUser.ts**:

```typescript
// Add property
private decisionAdapters: IDecisionAdapter[];

// In initialize()
this.decisionAdapters = [
  new FastPathDecisionAdapter(
    [this.displayName, ...this.getAliases()],
    (context) => this.buildRAGContextForDecision(context)
  ),
  new LLMDecisionAdapter(
    this.id,
    this.displayName
  )
];

// Refactor evaluateShouldRespond() to use adapters
private async evaluateShouldRespond(
  message: ChatMessageEntity,
  senderIsHuman: boolean,
  isMentioned: boolean
): Promise<CognitiveDecision> {
  const startTime = Date.now();

  try {
    // Build decision context
    const context: DecisionContext = {
      message,
      messageText: message.content?.text || '',
      personaId: this.id,
      personaDisplayName: this.displayName,
      senderIsHuman,
      isMentioned,
      gatingModel: this.entity?.personaConfig?.gatingModel,
      contextWindowMinutes: this.entity?.personaConfig?.contextWindowMinutes,
      minContextMessages: this.entity?.personaConfig?.minContextMessages
    };

    // Try each adapter in order
    for (const adapter of this.decisionAdapters) {
      const decision = await adapter.evaluate(context);
      if (decision !== null) {
        console.log(`✅ ${this.displayName}: Decision made by ${adapter.name}`);
        return decision;
      }
    }

    // Default: SILENT (no adapter could decide)
    return {
      shouldRespond: false,
      confidence: 1.0,
      reason: 'No adapter could evaluate',
      model: 'default'
    };
  } catch (error) {
    console.error(`❌ ${this.displayName}: Decision evaluation failed:`, error);
    return {
      shouldRespond: false,
      confidence: 0,
      reason: `Error: ${error}`,
      model: 'error'
    };
  }
}
```

---

## Benefits

1. **Extensibility**: Add new adapters without touching PersonaUser
   - HeuristicDecisionAdapter (bag-of-words scoring)
   - MultiModelDecisionAdapter (vote across models)
   - RuleBasedDecisionAdapter (domain-specific rules)

2. **Configuration**: Personas can use different adapter chains
   ```typescript
   // In persona config
   decisionStrategy: 'fast-only' | 'llm-only' | 'fast-then-llm' | 'custom'
   ```

3. **Testing**: Test adapters in isolation
   ```typescript
   // Unit test FastPathDecisionAdapter
   const adapter = new FastPathDecisionAdapter(['Helper AI'], mockRAGBuilder);
   const decision = await adapter.evaluate(mockContext);
   expect(decision.shouldRespond).toBe(true);
   ```

4. **Domain Agnostic**: When chat → game/code/web, just swap RAG builder
   ```typescript
   new FastPathDecisionAdapter(
     ['Helper AI'],
     (context) => new GameRAGBuilder().buildContext(...)  // Game domain
   )
   ```

---

## Testing Strategy

### Unit Tests (Fast)

```typescript
// tests/unit/FastPathDecisionAdapter.test.ts
describe('FastPathDecisionAdapter', () => {
  it('returns RESPOND when mentioned', async () => {
    const adapter = new FastPathDecisionAdapter(['Helper AI'], mockRAGBuilder);
    const context = { ...mockContext, isMentioned: true };
    const decision = await adapter.evaluate(context);

    expect(decision).not.toBeNull();
    expect(decision.shouldRespond).toBe(true);
    expect(decision.model).toBe('fast-path');
  });

  it('returns null when not mentioned', async () => {
    const adapter = new FastPathDecisionAdapter(['Helper AI'], mockRAGBuilder);
    const context = { ...mockContext, isMentioned: false };
    const decision = await adapter.evaluate(context);

    expect(decision).toBeNull();
  });
});
```

### Integration Tests (Slow)

```bash
# Test with real system
npm start
./jtag debug/chat-send --roomId="UUID" --message="helper ai what do you think?"

# Check decision was made by fast-path
./jtag debug/logs --filterPattern="Decision made by fast-path"

# Check decision logged with responseContent
./jtag ai/report/decisions --limit=1 --verbose=true
```

---

## Migration Path

1. ✅ **Phase 1: Bug fix** (DONE)
   - Add `filteredRagContext` to fast-path return
   - Verify responseContent captured in decision logging

2. **Phase 2: Extract adapters** (NEXT)
   - Create FastPathDecisionAdapter
   - Create LLMDecisionAdapter
   - No behavior changes, just refactoring

3. **Phase 3: Wire adapters**
   - Add adapter chain to PersonaUser
   - Refactor evaluateShouldRespond() to use adapters
   - Run tests, verify identical behavior

4. **Phase 4: Add thermal activation**
   - Implement PersonaThermalState class
   - Create ThermalDecisionAdapter
   - Hardcode thermal profiles in Constants.ts (temporary)
   - Wire into adapter chain
   - Test sleep/wake behavior

5. **Phase 5: Move thermal config to entity definitions**
   - Add `thermalProfile` field to UserEntity (or AIUserConfig)
   - Migrate hardcoded profiles from Constants.ts to database
   - Allow per-persona thermal configuration via UI
   - Support custom profiles for different personas

6. **Phase 6: Add new adapters**
   - HeuristicDecisionAdapter (bag-of-words)
   - MultiModelDecisionAdapter (voting)
   - Per-persona adapter chain configuration

---

## Thermal Activation Model

### Overview

PersonaUsers use a **thermal activation model** for dynamic response behavior. Temperature (0.0-1.0) rises with conversation activity and triggers AI activation when crossing a threshold. This creates natural engagement patterns - AIs become more responsive in active conversations and naturally cool down during inactivity.

**Key Insight**: Use floats (0-1) not enums. System states (cold/warming/hot/chaotic) **emerge naturally** from temperature dynamics.

### Core Thermal Dynamics

```typescript
interface ThermalProfile {
  // Rate at which temperature rises per activity event
  heatRate: number;           // 0.05 - 0.30 typical

  // Rate at which temperature naturally decays (per second)
  decayRate: number;          // 0.0001 - 0.001 typical

  // Rate at which ambient conversations raise temperature
  ambientAbsorption: number;  // 0.01 - 0.05 typical

  // Temperature boost when AI is mentioned by name
  mentionBoost: number;       // 0.3 - 0.6 typical

  // Temperature threshold to trigger activation
  activationThreshold: number; // 0.5 - 0.8 typical
}
```

### Temperature Evolution Over Time

```typescript
class PersonaThermalState {
  private temperature: number = 0.0;  // Current temperature (0-1)
  private profile: ThermalProfile;
  private lastUpdateTime: number = Date.now();

  // Update temperature based on elapsed time
  private applyDecay(): void {
    const now = Date.now();
    const elapsedSeconds = (now - this.lastUpdateTime) / 1000;
    this.lastUpdateTime = now;

    // Natural temperature decay over time
    this.temperature = Math.max(0.0,
      this.temperature - (this.profile.decayRate * elapsedSeconds)
    );
  }

  // Heat from direct activity (message sent, task completed)
  onActivity(): void {
    this.applyDecay();
    this.temperature = Math.min(1.0,
      this.temperature + this.profile.heatRate
    );
  }

  // Heat from ambient conversation (others talking)
  onAmbientActivity(): void {
    this.applyDecay();
    this.temperature = Math.min(1.0,
      this.temperature + this.profile.ambientAbsorption
    );
  }

  // Heat from being mentioned by name (wake-up trigger)
  onMentioned(): void {
    this.applyDecay();
    this.temperature = Math.min(1.0,
      this.temperature + this.profile.mentionBoost
    );
  }

  // Check if temperature is above activation threshold
  shouldActivate(): boolean {
    this.applyDecay();
    return this.temperature >= this.profile.activationThreshold;
  }

  // Get current temperature for introspection
  getCurrentTemperature(): number {
    this.applyDecay();
    return this.temperature;
  }
}
```

### Domain-Specific Thermal Profiles

Different activity domains (chat, game, code, academy) require different thermal dynamics:

```typescript
const thermalProfiles: Record<string, ThermalProfile> = {
  // CHAT: Moderate, balanced dynamics
  // - Conversations ebb and flow naturally
  // - Moderate activation threshold (not too eager, not too passive)
  chat: {
    heatRate: 0.15,              // Moderate heat per message
    decayRate: 0.001,            // Medium decay (5 min to cool fully)
    ambientAbsorption: 0.03,     // Some ambient awareness
    mentionBoost: 0.4,           // Strong mention response
    activationThreshold: 0.6     // Moderate threshold
  },

  // GAME: Fast, urgent dynamics
  // - Games require rapid response (turn-based, real-time events)
  // - Low activation threshold (eager to participate)
  // - Fast decay (short attention span between events)
  game: {
    heatRate: 0.25,              // High heat per event (urgent)
    decayRate: 0.0005,           // Slow decay (stay warm longer)
    ambientAbsorption: 0.05,     // High ambient awareness
    mentionBoost: 0.6,           // Very strong mention response
    activationThreshold: 0.5     // Low threshold (eager)
  },

  // CODE: Slow, thoughtful dynamics
  // - Code review requires sustained attention
  // - High activation threshold (only when truly needed)
  // - Very slow decay (maintain focus during work)
  code: {
    heatRate: 0.08,              // Low heat per event (deliberate)
    decayRate: 0.0002,           // Very slow decay (long focus)
    ambientAbsorption: 0.01,     // Low ambient awareness (focused work)
    mentionBoost: 0.3,           // Moderate mention response
    activationThreshold: 0.7     // High threshold (selective)
  },

  // ACADEMY: Balanced, educational dynamics
  // - Teaching requires sustained engagement
  // - Moderate threshold (helpful but not intrusive)
  // - Balanced decay (maintain context during learning)
  academy: {
    heatRate: 0.12,              // Moderate heat per interaction
    decayRate: 0.0008,           // Medium-slow decay (maintain context)
    ambientAbsorption: 0.02,     // Low-moderate ambient awareness
    mentionBoost: 0.35,          // Moderate mention response
    activationThreshold: 0.65    // Moderate-high threshold
  }
};
```

### Emergent System States

States **emerge naturally** from temperature values, not explicit enums:

```typescript
// Temperature ranges create natural behavioral zones:
// 0.0 - 0.2: COLD (hibernating, only mentions wake)
// 0.2 - 0.5: WARMING (passively observing, building context)
// 0.5 - 0.8: HOT (actively engaged, high responsiveness)
// 0.8 - 1.0: CHAOTIC (maximum engagement, might need cooldown)

function getEmergentState(temperature: number): string {
  if (temperature < 0.2) return 'COLD';
  if (temperature < 0.5) return 'WARMING';
  if (temperature < 0.8) return 'HOT';
  return 'CHAOTIC';
}

// But adapters just check shouldActivate() - no explicit state handling needed!
```

### Sleep/Wake Cycle Integration

```typescript
class ThermalDecisionAdapter implements IDecisionAdapter {
  readonly name = 'thermal-gating';
  private thermalState: PersonaThermalState;

  async evaluate(context: DecisionContext): Promise<CognitiveDecision | null> {
    // Apply thermal dynamics based on context
    if (context.isMentioned) {
      this.thermalState.onMentioned();  // Wake-up trigger
    } else if (context.senderIsHuman) {
      this.thermalState.onAmbientActivity();  // Ambient heat
    }

    // Check activation threshold
    if (!this.thermalState.shouldActivate()) {
      return null;  // Too cold, try next adapter
    }

    // Temperature above threshold - proceed with LLM evaluation
    // (Thermal adapter gates access to more expensive LLMDecisionAdapter)
    return null;  // Let next adapter handle actual decision
  }
}

// Adapter chain becomes:
// 1. FastPathDecisionAdapter (mentions always respond)
// 2. ThermalDecisionAdapter (check temperature threshold)
// 3. LLMDecisionAdapter (expensive LLM evaluation)
// 4. Default: SILENT
```

### Domain Switching

PersonaUser switches thermal profiles based on current activity context:

```typescript
class PersonaUser extends AIUser {
  private thermalState: PersonaThermalState;
  private currentDomain: string = 'chat';  // Default domain

  // Switch thermal profile when domain changes
  private switchDomain(newDomain: string): void {
    if (newDomain !== this.currentDomain) {
      console.log(`🌡️ ${this.displayName}: Switching thermal profile: ${this.currentDomain} → ${newDomain}`);

      this.currentDomain = newDomain;
      this.thermalState = new PersonaThermalState(
        thermalProfiles[newDomain]
      );
    }
  }

  // Detect domain from context (room type, message content, etc.)
  private detectDomain(context: DecisionContext): string {
    // Simple heuristics (can be expanded):
    if (context.message.roomId.includes('game')) return 'game';
    if (context.message.roomId.includes('code')) return 'code';
    if (context.message.roomId.includes('academy')) return 'academy';
    return 'chat';  // Default
  }

  async evaluateShouldRespond(/* ... */): Promise<CognitiveDecision> {
    // Detect and switch domain if needed
    const domain = this.detectDomain(context);
    this.switchDomain(domain);

    // Continue with adapter chain...
  }
}
```

### Thermal Ecosystem Dynamics

Multiple PersonaUsers create a thermal ecosystem with emergent behaviors:

**Entropy Forces** (future enhancement):
- System seeks thermal equilibrium across all AIs
- If all AIs cold → entropy adds warmth (ensure someone responds)
- If all AIs hot → entropy adds cooldown (prevent chaos)
- Natural load balancing without explicit coordination

```typescript
// Future: CoordinationDecisionLogger tracks ecosystem temperature
class ThermalEcosystem {
  private aiTemperatures: Map<UUID, number> = new Map();

  // Calculate system-wide thermal entropy
  getEntropyForce(): number {
    const temps = Array.from(this.aiTemperatures.values());
    const avgTemp = temps.reduce((a, b) => a + b, 0) / temps.length;

    // If avg temp too low → positive entropy (add heat)
    // If avg temp too high → negative entropy (add cooldown)
    if (avgTemp < 0.3) return 0.1;   // Warm up system
    if (avgTemp > 0.8) return -0.1;  // Cool down system
    return 0.0;  // Equilibrium
  }
}
```

### Integration with Adapter Pattern

Thermal model integrates cleanly into adapter chain:

```typescript
// In PersonaUser.initialize()
this.decisionAdapters = [
  // 1. Fast-path: Mentions always bypass thermal gating
  new FastPathDecisionAdapter(
    [this.displayName, ...this.getAliases()],
    (context) => this.buildRAGContextForDecision(context)
  ),

  // 2. Thermal gate: Check temperature threshold
  new ThermalDecisionAdapter(
    this.thermalState,
    thermalProfiles[this.currentDomain]
  ),

  // 3. LLM evaluation: Only if thermal threshold passed
  new LLMDecisionAdapter(
    this.id,
    this.displayName
  )
];
```

**Benefits**:
1. **Separation of concerns**: Thermal state isolated in adapter
2. **Domain-agnostic**: Thermal logic works for chat/game/code/web
3. **Testable**: Can unit test thermal dynamics independently
4. **Configurable**: Easy to tune thermal profiles per domain
5. **Emergent behavior**: Natural engagement patterns without explicit rules

---

## Decision: When to Implement?

**Option A: Implement now**
- Pros: Fresh context, clean abstraction, extensibility
- Cons: More deployment time, needs testing

**Option B: Commit bug fix, implement adapters later**
- Pros: Bug fix deployed quickly, can test responseContent capture first
- Cons: Leaves technical debt (hardcoded logic in PersonaUser)

**Recommendation**: Option A - implement now while we have full context. The refactor is clean (just extraction), and we've already verified the bug fix logic works.
