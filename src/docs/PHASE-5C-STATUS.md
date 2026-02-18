# Phase 5C Implementation Status

**Date**: 2025-11-11
**Status**: 90% Complete - Core infrastructure ready, needs final wiring

---

## ✅ Completed

### 1. Core Infrastructure (100%)

- **CoordinationDecisionEntity** (630 lines) - Complete decision context structure
  - Location: `system/data/entities/CoordinationDecisionEntity.ts`
  - All interfaces defined: RAGContext, AmbientState, CoordinationSnapshot, DecisionData, OutcomeData
  - Full validation logic
  - Pagination config

- **EntityRegistry Integration** - Entity registered and database table created
  - Location: `daemons/data-daemon/server/EntityRegistry.ts`
  - Entity imported (line 24)
  - Initialized (line 50)
  - Registered (line 68)
  - ✅ Verified: `./jtag data/schema --collection=coordination_decisions` works

- **CoordinationDecisionLogger** (200+ lines) - Static logging utility
  - Location: `system/coordination/server/CoordinationDecisionLogger.ts`
  - `logDecision()` method - logs complete decision with all context
  - `buildRAGContext()` helper - converts ChatRAGBuilder output
  - `buildChatVisualContext()` helper - captures UI state
  - Fetches ambient state from ChatCoordinationStream
  - Builds coordination snapshot from ThoughtStream
  - Automatic sequence numbering per actor

### 2. PersonaUser Integration (50%)

- ✅ Import added (line 48-49):
  ```typescript
  import { CoordinationDecisionLogger } from '../../coordination/server/CoordinationDecisionLogger';
  import type { RAGContext } from '../../data/entities/CoordinationDecisionEntity';
  ```

### 3. Documentation (100%)

- **PHASE-5C-INTEGRATION-PLAN.md** - Comprehensive 200+ line implementation guide
- **COORDINATION-DECISION-ARCHITECTURE.md** - Vision and use cases
- **NOVEL-CONCEPTS-TO-ADD.md** - Updated with Phase 3bis completion

---

## 🚧 Remaining Work (10%)

### Final Integration Steps

The architecture is sound, logger is ready, entity/schema are in place. Just need to wire PersonaUser to call the logger at decision points.

#### Step 1: Modify `evaluateShouldRespond()` Return Type

**Location**: PersonaUser.ts line 1728

**Current**:
```typescript
): Promise<{
  shouldRespond: boolean;
  confidence: number;
  reason: string;
  model?: string;
  ragContextSummary?: { ... };
  conversationHistory?: Array<{ ... }>;
}> {
```

**Add**:
```typescript
): Promise<{
  shouldRespond: boolean;
  confidence: number;
  reason: string;
  model?: string;
  ragContextSummary?: { ... };
  conversationHistory?: Array<{ ... }>;
  filteredRagContext?: any;  // ADD THIS LINE
}> {
```

#### Step 2: Return `filteredRagContext` from `evaluateShouldRespond()`

**Location**: PersonaUser.ts line 1949

**Current**:
```typescript
      return {
        shouldRespond: result.shouldRespond,
        confidence: result.confidence,
        reason: result.reason,
        model: result.model,
        ragContextSummary: { ... },
        conversationHistory: recentHistory.map(msg => ({ ... }))
      };
```

**Add**:
```typescript
      return {
        shouldRespond: result.shouldRespond,
        confidence: result.confidence,
        reason: result.reason,
        model: result.model,
        ragContextSummary: { ... },
        conversationHistory: recentHistory.map(msg => ({ ... })),
        filteredRagContext  // ADD THIS LINE
      };
```

#### Step 3: Add helper method to PersonaUser (around line 750)

```typescript
/**
 * Build CoordinationDecision RAGContext from ChatRAGBuilder output
 * Converts domain-specific RAG format to universal decision logging format
 */
private buildCoordinationRAGContext(filteredRagContext: any): RAGContext {
  const systemPrompt = filteredRagContext.identity?.systemPrompt ??
                       `You are ${this.displayName}. ${this.entity?.bio ?? ''}`;

  return {
    identity: {
      systemPrompt,
      bio: this.entity?.bio ?? '',
      role: this.displayName
    },
    conversationHistory: (filteredRagContext.conversationHistory ?? []).map((msg: any) => ({
      role: msg.role,
      content: msg.content,
      timestamp: msg.timestamp ?? Date.now()
    })),
    artifacts: filteredRagContext.artifacts ?? [],
    privateMemories: filteredRagContext.privateMemories ?? [],
    metadata: {
      timestamp: Date.now(),
      tokenCount: filteredRagContext.metadata?.messageCount ??
                  filteredRagContext.conversationHistory?.length ?? 0,
      contextWindow: 4096
    }
  };
}
```

#### Step 4: Log SILENT Decision

**Location**: PersonaUser.ts line 539 (after `if (!gatingResult.shouldRespond)`)

**Add BEFORE existing `this.logAIDecision()` call**:

```typescript
    // PHASE 5C: Log coordination decision to database
    if (gatingResult.filteredRagContext) {
      try {
        const decisionStartTime = Date.now();
        const ragContext = this.buildCoordinationRAGContext(gatingResult.filteredRagContext);

        await CoordinationDecisionLogger.logDecision({
          actorId: this.id,
          actorName: this.displayName,
          actorType: 'ai-persona',
          triggerEventId: messageEntity.id,
          ragContext,
          visualContext: undefined,
          action: 'SILENT',
          confidence: gatingResult.confidence,
          reasoning: gatingResult.reason,
          responseContent: undefined,
          modelUsed: gatingResult.model,
          modelProvider: this.modelConfig.provider ?? 'ollama',
          tokensUsed: undefined,
          responseTime: Date.now() - decisionStartTime,
          sessionId: DataDaemon.jtagContext!.sessionId,
          contextId: messageEntity.roomId,
          tags: [senderIsHuman ? 'human-sender' : 'ai-sender', 'gating-silent']
        });
      } catch (error) {
        console.error(`❌ ${this.displayName}: Failed to log SILENT decision:`, error);
      }
    }
```

#### Step 5: Log RESPOND Decision

**Location**: PersonaUser.ts line 577 (after `// === RESPOND: LLM gating decided to respond`)

**Add BEFORE existing `this.logAIDecision()` call**:

```typescript
    // PHASE 5C: Log coordination decision to database
    if (gatingResult.filteredRagContext) {
      try {
        const decisionStartTime = Date.now();
        const ragContext = this.buildCoordinationRAGContext(gatingResult.filteredRagContext);

        await CoordinationDecisionLogger.logDecision({
          actorId: this.id,
          actorName: this.displayName,
          actorType: 'ai-persona',
          triggerEventId: messageEntity.id,
          ragContext,
          visualContext: undefined,
          action: 'POSTED',
          confidence: gatingResult.confidence,
          reasoning: gatingResult.reason,
          responseContent: undefined,  // Will be filled after generation
          modelUsed: gatingResult.model,
          modelProvider: this.modelConfig.provider ?? 'ollama',
          tokensUsed: undefined,
          responseTime: Date.now() - decisionStartTime,
          sessionId: DataDaemon.jtagContext!.sessionId,
          contextId: messageEntity.roomId,
          tags: [
            senderIsHuman ? 'human-sender' : 'ai-sender',
            isMentioned ? 'mentioned' : 'not-mentioned',
            'gating-respond'
          ]
        });
      } catch (error) {
        console.error(`❌ ${this.displayName}: Failed to log RESPOND decision:`, error);
      }
    }
```

---

## Testing Plan

### 1. Compile Check
```bash
npm run lint:file system/user/server/PersonaUser.ts
```

### 2. Deploy
```bash
npm start  # Wait 90+ seconds
```

### 3. Send Test Message
```bash
# Get room ID
./jtag data/list --collection=rooms --limit=1

# Send message mentioning Helper AI
./jtag debug/chat-send --roomId="<room-id>" --message="@helper-ai can you help me test?"
```

### 4. Verify Decision Logged
```bash
# Check decisions were logged
./jtag data/list --collection=coordination_decisions \
  --orderBy='[{"field":"createdAt","direction":"desc"}]' \
  --limit=5

# Inspect full decision
./jtag data/read --collection=coordination_decisions --id="<decision-id>"
```

### 5. Verify Complete Context

Expected fields in decision:
- ✅ `actorId`: Helper AI's UUID
- ✅ `action`: 'POSTED' or 'SILENT'
- ✅ `confidence`: 0.0-1.0
- ✅ `ragContext.conversationHistory`: Array of messages (15+)
- ✅ `ragContext.identity.systemPrompt`: Persona's system prompt
- ✅ `ambientState.temperature`: Current room temperature (0.0-1.0)
- ✅ `ambientState.userPresent`: Boolean
- ✅ `coordinationSnapshot.othersConsideringCount`: Number
- ✅ `decision.reasoning`: AI's explanation
- ✅ `metadata.sessionId`: Current session UUID
- ✅ `metadata.contextId`: Room UUID

---

## Success Criteria

1. ✅ TypeScript compiles without errors
2. ✅ System deploys successfully (`npm start`)
3. ✅ AI responds to test message
4. ✅ Decision logged to `coordination_decisions` table
5. ✅ RAG context includes full conversation history (15+ messages)
6. ✅ Ambient state captured (temperature, userPresent)
7. ✅ Coordination snapshot captured
8. ✅ Can query: "What did Helper AI see when they responded?"

---

## User's Goal: "See why general chat is incoherent"

Once complete, user can:

```bash
# List all decisions by persona
./jtag data/list --collection=coordination_decisions \
  --filter='{"actorName":"Helper AI"}' \
  --orderBy='[{"field":"createdAt","direction":"desc"}]'

# Inspect specific decision
./jtag data/read --collection=coordination_decisions --id="<id>"

# See exactly:
# - What RAG context they saw (all messages)
# - What system prompt they had
# - What ambient state was (temperature, presence)
# - What coordination state was (other AIs active)
# - Why they decided to respond/stay silent
# - What confidence level
```

This enables **time-travel debugging** - inspect any decision point with complete reproducible context.

---

## Estimated Time to Complete

- **Steps 1-5**: 1-2 hours (code edits + testing)
- **Debugging**: 30 min - 1 hour (handle any TypeScript errors)

**Total**: 1.5-3 hours

---

## Files Modified

1. ✅ `system/coordination/server/CoordinationDecisionLogger.ts` (created)
2. ✅ `system/data/entities/CoordinationDecisionEntity.ts` (created)
3. ✅ `daemons/data-daemon/server/EntityRegistry.ts` (modified - lines 24, 50, 68)
4. 🚧 `system/user/server/PersonaUser.ts` (needs steps 1-5 above)

---

**Status**: Core infrastructure complete. Final wiring is straightforward - just connecting the dots that are already in place.
