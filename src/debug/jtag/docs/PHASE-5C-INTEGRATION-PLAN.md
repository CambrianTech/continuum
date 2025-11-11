# Phase 5C: CoordinationDecision Logging Integration Plan

**Status**: Ready to implement
**Date**: 2025-11-11
**Goal**: Capture every AI decision (RESPOND/SILENT) with complete context for time-travel debugging and training

---

## Overview

Integrate `CoordinationDecisionLogger` into PersonaUser to log every decision made by personas. This enables:

1. **Time-travel debugging**: "Why did Helper AI respond to that message?" → inspect exact RAG context
2. **Training data collection**: Log 1000s of decisions → train autopilot models
3. **Meta-learning**: Companion AI suggestions → learn from human overrides
4. **Coherence debugging**: User's explicit goal - "see why our general chat is kind of getting incoherent"

---

## Decision Points in PersonaUser

### 1. **evaluateShouldRespond()** (Line 1722-1950)

**Three decision paths:**

#### Path A: System Test Filter (Line 1746)
- **Action**: SILENT
- **Reason**: "System test message - skipped to avoid noise"
- **Fast-path**: No LLM call, no RAG context
- **Log?**: NO - system tests don't need training data

#### Path B: Fast-Path Mentioned (Line 1785)
- **Action**: RESPOND
- **Confidence**: 0.95-0.99
- **Reason**: "Directly mentioned by name"
- **Fast-path**: No LLM call, minimal RAG context
- **Log?**: YES - capture mention-based decisions

#### Path C: LLM Gating Decision (Line 1822-1950)
- **Action**: RESPOND or SILENT
- **Confidence**: From AI model
- **Reason**: From AI model
- **Full RAG context**: Built at line 1825-1840
- **Filtered context**: Line 1888-1891
- **Log?**: YES - this is the primary decision point

### 2. **evaluateAndPossiblyRespond()** (Line 468-729)

**After gating decision is made:**

#### Decision: SILENT (Line 539)
- Already has `gatingResult` with full context
- Logs to `AIDecisionLogger` (old system)
- **Integration point**: Add `CoordinationDecisionLogger.logDecision()`

#### Decision: RESPOND (Line 577)
- Already has `gatingResult` with full context
- Logs to `AIDecisionLogger` (old system)
- **Integration point**: Add `CoordinationDecisionLogger.logDecision()`

#### After Response Posted (Line 720)
- Response has been generated and posted
- **Integration point**: Update the RESPOND decision log with `responseContent`

---

## Implementation Strategy

### Step 1: Add Import to PersonaUser.ts

```typescript
import { CoordinationDecisionLogger } from '../../coordination/server/CoordinationDecisionLogger';
import type { RAGContext } from '../../data/entities/CoordinationDecisionEntity';
```

### Step 2: Helper Method - Build RAGContext

Add to PersonaUser class (around line 750):

```typescript
/**
 * Build complete RAGContext from filteredRagContext
 * Converts ChatRAGBuilder output to CoordinationDecisionEntity format
 */
private buildCoordinationRAGContext(
  filteredRagContext: any,  // From ChatRAGBuilder
  systemPrompt: string
): RAGContext {
  return {
    identity: {
      systemPrompt,
      bio: this.entity?.bio ?? '',
      role: this.displayName
    },
    conversationHistory: filteredRagContext.conversationHistory.map((msg: any) => ({
      role: msg.role,
      content: msg.content,
      timestamp: msg.timestamp ?? Date.now()
    })),
    artifacts: filteredRagContext.artifacts ?? [],
    privateMemories: filteredRagContext.privateMemories ?? [],
    metadata: {
      timestamp: Date.now(),
      tokenCount: filteredRagContext.metadata?.messageCount ?? 0,
      contextWindow: 4096  // From model config
    }
  };
}
```

### Step 3: Log SILENT Decisions

**Location**: PersonaUser.ts line 539 (after `if (!gatingResult.shouldRespond)`)

**Before existing `this.logAIDecision()` call, add:**

```typescript
// PHASE 5C: Log coordination decision to database
try {
  const ragContext = this.buildCoordinationRAGContext(
    filteredRagContext,  // PROBLEM: Not available in this scope
    gatingResult.conversationHistory?.[0]?.content ?? 'No system prompt'
  );

  await CoordinationDecisionLogger.logDecision({
    // Identity
    actorId: this.id,
    actorName: this.displayName,
    actorType: 'ai-persona',
    triggerEventId: messageEntity.id,

    // Complete RAG context
    ragContext,

    // Visual context (optional - could add chat UI state)
    visualContext: undefined,

    // Decision
    action: 'SILENT',
    confidence: gatingResult.confidence,
    reasoning: gatingResult.reason,
    responseContent: undefined,
    modelUsed: gatingResult.model,
    modelProvider: 'ollama',  // From this.modelConfig.provider
    tokensUsed: undefined,
    responseTime: Date.now() - startTime,  // PROBLEM: startTime not in scope

    // Metadata
    sessionId: DataDaemon.jtagContext!.sessionId,
    contextId: messageEntity.roomId,
    tags: [
      senderIsHuman ? 'human-sender' : 'ai-sender',
      'gating-decision'
    ]
  });
} catch (error) {
  console.error(`❌ ${this.displayName}: Failed to log SILENT decision:`, error);
}
```

### Step 4: Log RESPOND Decisions

**Location**: PersonaUser.ts line 577 (after `// === RESPOND: LLM gating decided to respond`)

**Before existing `this.logAIDecision()` call, add:**

```typescript
// PHASE 5C: Log coordination decision to database
try {
  const ragContext = this.buildCoordinationRAGContext(
    filteredRagContext,  // PROBLEM: Not available in this scope
    gatingResult.conversationHistory?.[0]?.content ?? 'No system prompt'
  );

  await CoordinationDecisionLogger.logDecision({
    // Identity
    actorId: this.id,
    actorName: this.displayName,
    actorType: 'ai-persona',
    triggerEventId: messageEntity.id,

    // Complete RAG context
    ragContext,

    // Visual context (optional)
    visualContext: undefined,

    // Decision
    action: 'POSTED',  // Will respond
    confidence: gatingResult.confidence,
    reasoning: gatingResult.reason,
    responseContent: undefined,  // Will be filled after response generation
    modelUsed: gatingResult.model,
    modelProvider: 'ollama',
    tokensUsed: undefined,
    responseTime: Date.now() - startTime,  // PROBLEM: startTime not in scope

    // Metadata
    sessionId: DataDaemon.jtagContext!.sessionId,
    contextId: messageEntity.roomId,
    tags: [
      senderIsHuman ? 'human-sender' : 'ai-sender',
      isMentioned ? 'mentioned' : 'not-mentioned',
      'gating-decision',
      'will-respond'
    ]
  });
} catch (error) {
  console.error(`❌ ${this.displayName}: Failed to log RESPOND decision:`, error);
}
```

---

## Problems to Solve

### Problem 1: `filteredRagContext` not in scope

The `filteredRagContext` is built inside `evaluateShouldRespond()` but the logging happens in `evaluateAndPossiblyRespond()`.

**Solution**: Modify `evaluateShouldRespond()` to return the filtered RAG context:

```typescript
private async evaluateShouldRespond(
  message: ChatMessageEntity,
  senderIsHuman: boolean,
  isMentioned: boolean
): Promise<{
  shouldRespond: boolean;
  confidence: number;
  reason: string;
  model?: string;
  ragContextSummary?: { ... };
  conversationHistory?: Array<{ ... }>;
  filteredRagContext?: any;  // ADD THIS
}> {
  // ... existing code ...

  return {
    shouldRespond: result.shouldRespond,
    confidence: result.confidence,
    reason: result.reason,
    model: gatingModel,
    ragContextSummary: { ... },
    conversationHistory: result.conversationHistory,
    filteredRagContext  // ADD THIS
  };
}
```

### Problem 2: `startTime` not in scope

The `startTime` is defined in `evaluateShouldRespond()` but logging happens in `evaluateAndPossiblyRespond()`.

**Solution**: Add decision start time to return value OR track start time in `evaluateAndPossiblyRespond()`:

```typescript
// At start of evaluateAndPossiblyRespond()
const decisionStartTime = Date.now();

// When logging
responseTime: Date.now() - decisionStartTime
```

### Problem 3: System prompt not readily available

The system prompt is built during response generation, not during gating.

**Solution**: Extract systemPrompt earlier OR build a minimal one:

```typescript
const systemPrompt = `You are ${this.displayName}. ${this.entity?.bio ?? ''}`;
```

---

## Testing Strategy

### Step 1: Verify Table Exists

```bash
./jtag data/schema --collection=coordination_decisions
```

Should show `CoordinationDecisionEntity` with all fields.

### Step 2: Send Test Message

```bash
./jtag debug/chat-send --roomId="general-room-id" --message="@helper-ai test"
```

### Step 3: Check Decisions Were Logged

```bash
./jtag data/list --collection=coordination_decisions --orderBy='[{"field":"createdAt","direction":"desc"}]' --limit=5
```

Should show decision logged with:
- `actorId`: Helper AI's ID
- `action`: 'POSTED' or 'SILENT'
- `confidence`: 0.0-1.0
- `ambientState.temperature`: Current room temperature
- `coordinationSnapshot`: ThoughtStream state
- `ragContext`: Full conversation history

### Step 4: Inspect Full Decision

```bash
./jtag data/read --collection=coordination_decisions --id="<decision-id>"
```

Should show complete context including:
- Full conversation history (15+ messages)
- System prompt with persona identity
- Ambient state (temperature, userPresent)
- Coordination snapshot (other AIs considering)
- Decision reasoning

---

## Timeline

- **Step 1-2**: Add import + helper method (15 min)
- **Step 3**: Solve scope problems (30 min)
- **Step 4**: Add logging to SILENT path (30 min)
- **Step 5**: Add logging to RESPOND path (30 min)
- **Step 6**: Test with real messages (30 min)
- **Step 7**: Debug any issues (1-2 hours)

**Total**: 3-4 hours

---

## Success Criteria

1. ✅ Every AI decision logged to `coordination_decisions` table
2. ✅ RAG context includes full conversation history
3. ✅ Ambient state captured (temperature, userPresent)
4. ✅ Coordination snapshot captured (ThoughtStream state)
5. ✅ Decision confidence and reasoning captured
6. ✅ User can query: "What did Helper AI see when they responded to Joel's message?"
7. ✅ Database query shows 10+ decisions logged after 5 minutes of chat

---

## Next Phase: Phase 5D

After Phase 5C complete:

1. Create `./jtag decision/list` command
2. Create `./jtag decision/inspect --id=<UUID>` command
3. Create `./jtag decision/replay --id=<UUID> --persona=<new-persona-id>` command

This enables the user's debugging use case: **"see why our general chat is kind of getting incoherent"**

---

**Ready to implement**: All architecture in place, just need to wire PersonaUser to logger.
