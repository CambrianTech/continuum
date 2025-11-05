# Implementation Plan - Multi-Persona Chat System Fixes
**Date**: 2025-10-12
**Status**: Planning phase - reviewing assumptions before implementation
**Philosophy**: Natural AI reasoning > efficiency hacks

---

## Document Review: Correcting Assumptions

### ✅ CORRECT Assumptions
1. **3 personas exist** (Helper, Teacher, CodeReview) - confirmed by user
2. **Ollama has no request queue** - confirmed in code
3. **RAG fetches from "now"** not trigger timestamp - confirmed in code
4. **Small models need structured context** - validated by experiment
5. **Ollama restart clears erroneous patterns** - validated by recovery
6. **Personas self-recovered and diagnosed issue** - observed behavior

### ❌ INCORRECT Assumptions to Correct

#### Assumption: "6 instances created"
**Reality**: 3 instances, 2 attempts each (original + retry) = 6 log entries
**Impact**: NO DUPLICATE INSTANCE BUG - this is timeout/retry behavior
**Correction**: Focus on preventing timeouts, not preventing duplicates

#### Assumption: "Global event subscription is a bug"
**Reality**: Personas ARE room members, should receive room events
**Impact**: Event subscription is working as designed
**Correction**: This is not a bug, just need optimization for late room filtering

#### Assumption: "Meta-commentary should be rejected"
**Reality**: Meta-commentary can be natural reasoning (e.g., "spam flood" diagnosis)
**Impact**: Being too strict would suppress natural AI reasoning
**Correction**: Only reject catastrophic gibberish, allow meta-commentary

#### Assumption: "Need to prevent multiple personas responding"
**Reality**: User wants "natural first citizen reasoning" - multiple responses OK
**Impact**: Should NOT hard-limit responses
**Correction**: Fix root causes (timeouts, RAG confusion), let AIs respond naturally

#### Assumption: "Hallucinated prefixes caused by confusing system prompt"
**Reality**: Partially true, but also model limitation
**Impact**: Can improve prompt, but small models will still struggle
**Correction**: Strip prefixes before storage + improve prompt, but don't expect perfection

---

## Complete Issue Inventory

### Category 1: CRITICAL (Blocks Production)

#### Issue #1: Ollama Concurrency Bottleneck
**Severity**: CRITICAL
**Impact**: 67% timeout rate, cascade retries, wasted compute
**Root Cause**: No client-side request queue, 3 simultaneous requests to 2-capacity Ollama
**Location**: `daemons/ai-provider-daemon/shared/OllamaAdapter.ts`
**Dependencies**: None
**Complexity**: Medium (client-side queue implementation)
**Test**: Can run 10 concurrent persona evaluations without timeouts

**Architecture Change Needed**:
```typescript
// NEW: Request queue in OllamaAdapter
class OllamaRequestQueue {
  private queue: Array<QueuedRequest> = [];
  private activeRequests = 0;
  private maxConcurrent = 2;  // Ollama capacity

  async enqueue<T>(request: () => Promise<T>): Promise<T> {
    // Queue management logic
  }
}
```

**Why This First**:
- Fixes 67% of timeout issues immediately
- Enables natural multi-persona responses
- No architectural dependencies
- Low risk (queue is self-contained)

**How to Fix (Step-by-Step)**:

1. **Create queue class** in `daemons/ai-provider-daemon/shared/OllamaAdapter.ts`:
```typescript
interface QueuedRequest<T> {
  executor: () => Promise<T>;
  resolve: (value: T) => void;
  reject: (error: Error) => void;
}

class OllamaRequestQueue {
  private queue: Array<QueuedRequest<any>> = [];
  private activeRequests = 0;
  private readonly maxConcurrent = 2;  // Ollama capacity

  async enqueue<T>(executor: () => Promise<T>): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      this.queue.push({ executor, resolve, reject });
      this.processQueue();
    });
  }

  private async processQueue(): Promise<void> {
    if (this.activeRequests >= this.maxConcurrent || this.queue.length === 0) {
      return;
    }

    this.activeRequests++;
    const request = this.queue.shift()!;

    try {
      const result = await request.executor();
      request.resolve(result);
    } catch (error) {
      request.reject(error as Error);
    } finally {
      this.activeRequests--;
      this.processQueue();  // Process next in queue
    }
  }
}
```

2. **Add queue instance** to OllamaAdapter class:
```typescript
export class OllamaAdapter implements AIProvider {
  private config: OllamaConfig;
  private requestQueue: OllamaRequestQueue;  // NEW

  constructor(config?: Partial<OllamaConfig>) {
    this.config = { ...DEFAULT_CONFIG, ...config };
    this.requestQueue = new OllamaRequestQueue();  // NEW
  }
```

3. **Wrap all Ollama requests** with queue:
```typescript
// BEFORE:
async generateText(options: GenerateTextOptions): Promise<GenerateTextResult> {
  const response = await this.makeRequest<OllamaGenerateResponse>('/api/generate', {
    model: options.model,
    prompt: options.prompt,
    system: options.systemPrompt,
    stream: false
  });
  // ...
}

// AFTER:
async generateText(options: GenerateTextOptions): Promise<GenerateTextResult> {
  return this.requestQueue.enqueue(async () => {
    const response = await this.makeRequest<OllamaGenerateResponse>('/api/generate', {
      model: options.model,
      prompt: options.prompt,
      system: options.systemPrompt,
      stream: false
    });
    // ...
    return result;
  });
}
```

4. **Test**:
```bash
# Post a question in chat, verify all 3 personas respond without timeouts
./jtag debug/logs --filterPattern="timeout|Ollama" --tailLines=50
# Should show 0 timeouts
```

---

#### Issue #2: RAG Temporal Confusion
**Severity**: CRITICAL
**Impact**: LLMs see "future" responses as context, causes hallucinations
**Root Cause**: RAG fetches messages from "now" instead of trigger timestamp
**Location**: `system/rag/builders/ChatRAGBuilder.ts:175-199`
**Dependencies**: Needs trigger timestamp passed from PersonaUser
**Complexity**: Medium (query filtering + parameter threading)
**Test**: RAG context for message M only contains messages posted before M.timestamp

**Architecture Change Needed**:
```typescript
// CHANGE: Add triggerTimestamp to RAGBuildOptions
interface RAGBuildOptions {
  maxMessages?: number;
  maxMemories?: number;
  includeArtifacts?: boolean;
  includeMemories?: boolean;
  triggerTimestamp?: Date;  // NEW: Filter messages before this time
}

// PersonaUser must provide trigger timestamp
const ragContext = await ChatRAGBuilder.buildContext(roomId, this.id, {
  triggerTimestamp: messageEntity.timestamp,  // NEW
  maxMessages: 20
});
```

**Why This Second**:
- Fixes hallucination root cause
- Works with existing small models
- Depends only on PersonaUser passing timestamp
- Medium risk (query logic change)

**How to Fix (Step-by-Step)**:

1. **Add triggerTimestamp to RAGBuildOptions** in `system/rag/builders/ChatRAGBuilder.ts`:
```typescript
interface RAGBuildOptions {
  maxMessages?: number;
  maxMemories?: number;
  includeArtifacts?: boolean;
  includeMemories?: boolean;
  triggerTimestamp?: Date;  // NEW: Only fetch messages before this time
}
```

2. **Update loadConversationHistory** to use timestamp filter (line ~175):
```typescript
private async loadConversationHistory(
  roomId: UUID,
  personaId: UUID,
  maxMessages: number,
  triggerTimestamp?: Date  // NEW parameter
): Promise<LLMMessage[]> {
  try {
    const filters: any = { roomId };

    // NEW: Add temporal filtering
    if (triggerTimestamp) {
      filters.timestamp = { $lt: triggerTimestamp.toISOString() };
    }

    const result = await DataDaemon.query<ChatMessageEntity>({
      collection: ChatMessageEntity.collection,
      filters,  // Now includes timestamp filter
      sort: [{ field: 'timestamp', direction: 'desc' }],
      limit: maxMessages
    });
    // ...
  }
}
```

3. **Thread triggerTimestamp through buildContext** (line ~50):
```typescript
async buildContext(
  contextId: UUID,
  personaId: UUID,
  options?: RAGBuildOptions
): Promise<RAGContext> {
  // ...

  const messages = await this.loadConversationHistory(
    contextId,
    personaId,
    maxMessages,
    options?.triggerTimestamp  // NEW: Pass timestamp
  );

  // ...
}
```

4. **Update PersonaUser to pass trigger timestamp** in `system/user/server/PersonaUser.ts` (line ~180):
```typescript
private async handleChatMessage(messageEntity: ChatMessageEntity): Promise<void> {
  // ...

  // Build RAG context with trigger timestamp
  const ragContext = await ChatRAGBuilder.buildContext(
    messageEntity.roomId,
    this.id,
    {
      maxMessages: 20,
      triggerTimestamp: messageEntity.timestamp  // NEW: Use message timestamp
    }
  );

  // ...
}
```

5. **Test**:
```bash
# Post a question, check RAG contexts in logs
./jtag debug/logs --filterPattern="RAG-CONTEXT" --tailLines=100

# Verify: For message at timestamp T, RAG only contains messages with timestamp < T
# No "future" messages should appear
```

---

#### Issue #3: Catastrophic Gibberish Storage
**Severity**: CRITICAL
**Impact**: Gibberish pollutes database → contaminates RAG → cascade failure
**Root Cause**: No response validation before storage
**Location**: `system/user/server/PersonaUser.ts` (response storage point)
**Dependencies**: Needs BoW analysis utility
**Complexity**: Medium (validation layer + metrics computation)
**Test**: Gibberish like "@@@@@@@@@..." is rejected, retry triggered

**Architecture Change Needed**:
```typescript
// NEW: Response validation layer in PersonaUser
async sendMessage(text: string): Promise<void> {
  // Validate response quality
  const validation = await this.validateResponseQuality(text);

  if (validation.isCatastrophic) {
    console.log('🚫 Catastrophic response detected, not storing');
    // Maybe retry with stronger prompt?
    return;
  }

  if (validation.hasHallucinatedPrefix) {
    text = this.stripPrefixes(text);  // Clean before storage
  }

  // Store to database
  await this.storeMessage(text);
}
```

**Why This Third**:
- Prevents cascade failures at source
- Works with all model tiers
- No dependencies on other fixes
- Low risk (validation is additive, doesn't break existing flow)

---

### Category 2: HIGH PRIORITY (Degraded Experience)

#### Issue #4: Wrong Role Assignment in RAG
**Severity**: HIGH
**Impact**: AI personas marked as `role=user`, confuses LLM about human vs AI
**Root Cause**: ChatRAGBuilder marks all non-self messages as `role=user`
**Location**: `system/rag/builders/ChatRAGBuilder.ts:226`
**Dependencies**: Needs user type information in message context
**Complexity**: Low (simple role assignment logic fix)
**Test**: Teacher AI messages appear as `role=assistant` in Helper AI's RAG

**Current Code**:
```typescript
return {
  role: isOwnMessage ? 'assistant' as const : 'user' as const,  // ❌ BUG
  content: markedContent,
  name: msg.senderName
};
```

**Should Be**:
```typescript
return {
  role: (isOwnMessage || msg.senderType === 'ai') ? 'assistant' as const : 'user' as const,
  content: markedContent,
  name: msg.senderName
};
```

**Why This Fourth**:
- Simple fix, high impact
- Helps small models understand context
- No architectural dependencies
- Very low risk

---

#### Issue #5: Triggering Event Buried in RAG
**Severity**: HIGH
**Impact**: Small models can't identify what they're evaluating (95% noise, 5% signal)
**Root Cause**: No explicit marking of triggering message in RAG context
**Location**: `system/rag/builders/ChatRAGBuilder.ts` (formatting logic)
**Dependencies**: Needs model tier detection (Issue #7)
**Complexity**: Medium (model-dependent formatting)
**Test**: Small model RAG clearly shows "TRIGGERING EVENT" marker

**Architecture Change Needed**:
```typescript
// Model-dependent RAG formatting
if (options.modelTier === 'small') {
  return formatSmallModelRAG(messages, triggerMessage);  // Explicit structure
} else if (options.modelTier === 'medium') {
  return formatMediumModelRAG(messages, triggerMessage);  // Moderate structure
} else {
  return formatLargeModelRAG(messages);  // Natural flow
}
```

**Why This Fifth**:
- Helps small models focus on relevant context
- Depends on model tier detection (Issue #7)
- Medium complexity
- Medium risk (changes RAG structure)

---

#### Issue #6: Confusing System Prompt
**Severity**: HIGH
**Impact**: Small models generate hallucinated prefixes, meta-commentary
**Root Cause**: Negative instructions + mentions non-existent format
**Location**: `system/rag/builders/ChatRAGBuilder.ts:158-170`
**Dependencies**: Needs model tier detection (Issue #7)
**Complexity**: Low (prompt rewrite)
**Test**: Small models stop generating "Helper AI:" prefixes

**Current Prompt Issues**:
- 3x "DO NOT" (negative instructions backfire with small models)
- Mentions "Name: message" format that doesn't exist
- No human vs AI distinction
- No task clarification

**Why This Sixth**:
- Simple fix, good impact
- Works with other fixes
- Depends on model tier detection
- Low risk (prompt is easily reverted)

---

### Category 3: MEDIUM PRIORITY (Optimization)

#### Issue #7: No Model Tier Detection
**Severity**: MEDIUM
**Impact**: Can't provide model-appropriate RAG/prompts
**Root Cause**: PersonaUser doesn't know model capabilities
**Location**: `system/user/server/PersonaUser.ts`
**Dependencies**: None (standalone feature)
**Complexity**: Low (simple string matching)
**Test**: llama3.2:1b detected as 'small', llama3.1:70b as 'large'

**Implementation**:
```typescript
private getModelTier(): 'small' | 'medium' | 'large' {
  const modelName = this.aiConfig?.modelName || '';

  if (modelName.includes('1b') || modelName.includes('3b')) {
    return 'small';
  }

  if (modelName.includes('gpt-4') ||
      modelName.includes('claude-opus') ||
      modelName.includes('70b') ||
      modelName.includes('405b')) {
    return 'large';
  }

  return 'medium';
}
```

**Why This Seventh**:
- Enables Issues #5 and #6
- Low complexity
- No dependencies
- Very low risk

---

#### Issue #8: Ollama Restart Not Automated
**Severity**: MEDIUM
**Impact**: Manual intervention required for cascade failures
**Root Cause**: No degradation monitoring + auto-restart
**Location**: NEW - needs health monitoring daemon
**Dependencies**: Issue #3 (validation metrics)
**Complexity**: High (new monitoring system)
**Test**: 5+ gibberish messages triggers automatic Ollama restart

**Architecture Change Needed**:
```typescript
// NEW: Degradation monitor in AIProviderDaemon
class DegradationMonitor {
  async checkHealth(): Promise<DegradationMetrics> {
    // Analyze last N responses for catastrophic failures
  }

  async remediate(metrics: DegradationMetrics): Promise<void> {
    if (metrics.catastrophicGibberishCount >= 5) {
      await this.restartOllama();
    }
  }
}
```

**Why This Eighth**:
- Nice-to-have, not critical
- Depends on validation metrics from Issue #3
- High complexity (new system)
- Medium risk (daemon restart logic)

---

#### Issue #9: Hallucinated Prefixes in Database
**Severity**: MEDIUM
**Impact**: Existing messages pollute RAG contexts
**Root Cause**: Previous responses stored with prefixes
**Location**: Existing chat_messages records
**Dependencies**: Issue #3 (prevents new prefixes)
**Complexity**: Low (data migration script)
**Test**: Count of messages with prefixes = 0

**Implementation**:
```typescript
// One-time data cleanup script
async function cleanHallucinatedPrefixes(): Promise<void> {
  const messages = await DataDaemon.query<ChatMessageEntity>({
    collection: 'chat_messages',
    filters: {} // All messages
  });

  for (const msg of messages.data) {
    const cleaned = msg.data.text
      .replace(/^(\w+\s+AI:|Human:|\[\d{2}:\d{2}\])\s*/, '');

    if (cleaned !== msg.data.text) {
      await DataDaemon.update({
        collection: 'chat_messages',
        id: msg.id,
        data: { text: cleaned }
      });
    }
  }
}
```

**Why This Ninth**:
- Cleans up historical pollution
- Depends on Issue #3 to prevent new prefixes
- Low complexity
- Low risk (non-destructive cleanup)

---

### Category 4: LOW PRIORITY (Future Enhancement)

#### Issue #10: No Context Window Limits by Model
**Severity**: LOW
**Impact**: Small models get too much context, large models get too little
**Root Cause**: Fixed maxMessages regardless of model tier
**Location**: `system/rag/builders/ChatRAGBuilder.ts`
**Dependencies**: Issue #7 (model tier detection)
**Complexity**: Low (adjust message count by tier)
**Test**: Small model gets 10-15 messages, large model gets 50-100

**Implementation**:
```typescript
private getMaxMessagesForTier(tier: 'small' | 'medium' | 'large'): number {
  switch (tier) {
    case 'small': return 15;
    case 'medium': return 30;
    case 'large': return 100;
  }
}
```

**Why This Tenth**:
- Nice optimization
- Depends on Issue #7
- Very low complexity
- Very low risk

---

#### Issue #11: No Topic Change Detection in RAG
**Severity**: LOW
**Impact**: Off-topic messages confuse context
**Root Cause**: No semantic topic tracking
**Location**: `system/rag/builders/ChatRAGBuilder.ts`
**Dependencies**: Needs topic modeling (complex)
**Complexity**: HIGH (requires NLP/embeddings)
**Test**: RAG notes "Topic changed from websockets to async/await"

**Why This Last**:
- High complexity
- Requires external dependencies (embeddings)
- Marginal benefit vs cost
- High risk (new complex system)

---

## Implementation Order (Low-Hanging Fruit → Major Architecture)

### Phase 1: Quick Wins (Low Complexity, High Impact)
1. ✅ **Issue #7**: Model tier detection (1 hour)
   - Simple string matching in PersonaUser
   - No dependencies
   - Enables other fixes

2. ✅ **Issue #4**: Fix role assignment (30 minutes)
   - One-line fix in ChatRAGBuilder
   - Immediate improvement for small models
   - No dependencies

3. ✅ **Issue #6**: Rewrite system prompt (2 hours)
   - Prompt engineering for small/medium/large tiers
   - Depends on Issue #7
   - Low risk

### Phase 2: Core Infrastructure (Medium Complexity, Critical Impact)
4. ✅ **Issue #1**: Ollama request queue (4-6 hours)
   - Client-side queue in OllamaAdapter
   - Fixes 67% of timeout issues
   - No dependencies

5. ✅ **Issue #2**: RAG temporal filtering (3-4 hours)
   - Add triggerTimestamp to RAGBuildOptions
   - Update PersonaUser to pass timestamp
   - Update ChatRAGBuilder query logic
   - Depends on PersonaUser changes

6. ✅ **Issue #3**: Response validation (4-6 hours)
   - BoW metrics computation
   - Validation logic in PersonaUser
   - Catastrophic detection + remediation
   - No dependencies

### Phase 3: RAG Quality (Medium Complexity, High Impact)
7. ✅ **Issue #5**: Structured RAG for small models (3-4 hours)
   - Model-dependent formatting
   - Explicit triggering event marker
   - Depends on Issue #7

8. ✅ **Issue #10**: Context window by model tier (1 hour)
   - Adjust message count by tier
   - Depends on Issue #7

### Phase 4: Cleanup & Automation (Low Priority)
9. ✅ **Issue #9**: Clean hallucinated prefixes (2 hours)
   - Data migration script
   - Depends on Issue #3

10. ✅ **Issue #8**: Automated Ollama restart (6-8 hours)
    - Degradation monitoring
    - Auto-restart logic
    - Depends on Issue #3

11. ⏸️ **Issue #11**: Topic change detection (20+ hours)
    - Future enhancement
    - Requires embeddings/NLP

---

## Testing Strategy for Each Issue

### Testing Protocol
1. **Before fix**: Document current behavior + screenshot
2. **After fix**: Test same scenario, compare behavior
3. **Edge cases**: Try to break it
4. **Integration**: Test with other fixes
5. **Rollback plan**: Know how to revert if issues

### Specific Tests

**Issue #1 (Ollama Queue)**:
- Test: Post question, all 3 personas respond without timeouts
- Measure: 0% timeout rate (vs 67% before)
- Edge: 10 concurrent personas evaluating

**Issue #2 (RAG Temporal)**:
- Test: Persona A responds, Persona B RAG doesn't include A's response
- Measure: No "future" messages in RAG context
- Edge: Multiple personas responding in quick succession

**Issue #3 (Validation)**:
- Test: Inject gibberish pattern, verify rejection
- Measure: 0 gibberish messages stored
- Edge: Borderline cases (low diversity but not gibberish)

**Issue #4 (Role Assignment)**:
- Test: Check Helper AI's RAG, verify Teacher AI has `role=assistant`
- Measure: All AI messages marked as assistant
- Edge: Mixed human/AI conversation

**Issue #5 (Structured RAG)**:
- Test: Small model RAG shows clear "TRIGGERING EVENT" section
- Measure: Personas respond to correct question
- Edge: Multiple questions in quick succession

**Issue #6 (System Prompt)**:
- Test: Personas stop generating "Helper AI:" prefixes
- Measure: 0 new hallucinated prefixes
- Edge: Long conversations, multiple topics

---

## Architecture Impact Summary

### New Components Needed
1. **OllamaRequestQueue** - Client-side queue (Issue #1)
2. **ResponseValidator** - BoW validation (Issue #3)
3. **DegradationMonitor** - Health monitoring (Issue #8)
4. **ModelTierDetector** - Capability detection (Issue #7)

### Modified Components
1. **OllamaAdapter** - Add queue integration
2. **ChatRAGBuilder** - Temporal filtering, model-dependent formatting, role fix
3. **PersonaUser** - Model tier detection, validation, timestamp passing
4. **System prompts** - Rewrite for clarity

### No Changes Needed
1. **EventsDaemon** - Working as designed
2. **DataDaemon** - Query system is fine
3. **UserDaemon** - Persona management is fine
4. **Room membership** - Working correctly

---

## Risk Assessment

### Low Risk (Safe to implement)
- Issue #7 (Model tier detection) - Standalone feature
- Issue #4 (Role assignment) - One-line fix
- Issue #9 (Prefix cleanup) - Non-destructive migration
- Issue #10 (Context window) - Simple adjustment

### Medium Risk (Test thoroughly)
- Issue #1 (Ollama queue) - New queue system
- Issue #2 (RAG temporal) - Query logic change
- Issue #3 (Validation) - New validation layer
- Issue #6 (System prompt) - Prompt engineering

### High Risk (Careful implementation)
- Issue #5 (Structured RAG) - Changes RAG structure significantly
- Issue #8 (Auto-restart) - Daemon restart logic
- Issue #11 (Topic detection) - Complex NLP system

---

## Success Criteria

### Phase 1 Complete When:
- ✅ Model tier detection working (llama3.2:1b → 'small')
- ✅ AI messages have `role=assistant` in RAG
- ✅ New system prompts deployed

### Phase 2 Complete When:
- ✅ 0% timeout rate on concurrent persona evaluations
- ✅ RAG contexts only contain messages before trigger
- ✅ Gibberish responses rejected before storage

### Phase 3 Complete When:
- ✅ Small model RAG has clear structure
- ✅ Context window scaled by model tier
- ✅ Personas respond to correct questions (no hallucinations)

### Phase 4 Complete When:
- ✅ Database clean of hallucinated prefixes
- ✅ Ollama auto-restarts on degradation
- ✅ System runs stable for 24+ hours

---

## Commit Strategy

**One issue per commit**, test between each:

```bash
# Phase 1
git commit -m "feat: Add model tier detection to PersonaUser"
git commit -m "fix: Correct role assignment for AI messages in RAG"
git commit -m "feat: Add model-dependent system prompts"

# Phase 2
git commit -m "feat: Add request queue to OllamaAdapter"
git commit -m "fix: Add temporal filtering to RAG queries"
git commit -m "feat: Add response validation with BoW metrics"

# Phase 3
git commit -m "feat: Add structured RAG formatting for small models"
git commit -m "feat: Scale context window by model tier"

# Phase 4
git commit -m "chore: Clean hallucinated prefixes from database"
git commit -m "feat: Add automated Ollama restart on degradation"
```

---

## Notes for Implementation

### Keep in Mind
- **Natural AI reasoning** > efficiency (user priority)
- **Test after each change** - don't batch commits
- **Screenshot before/after** - visual verification
- **Check logs first** - don't guess
- **Be lenient with validation** - only block catastrophic failures

### Don't Do
- ❌ Hard-limit number of responses
- ❌ Implement turn-taking queue
- ❌ Disable personas
- ❌ Over-optimize prematurely
- ❌ Batch multiple fixes without testing

### Do Do
- ✅ Fix root causes, not symptoms
- ✅ Let AIs respond naturally
- ✅ Test each fix individually
- ✅ Deploy incrementally
- ✅ Check in working code frequently
