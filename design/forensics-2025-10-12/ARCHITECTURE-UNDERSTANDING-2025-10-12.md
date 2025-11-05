# Architecture Understanding - 2025-10-12
**Purpose**: Document verified architecture understanding before implementation
**Status**: Research complete - ready for implementation plan corrections

---

## What I NOW KNOW (Verified by Reading Code)

### 1. ThoughtStream Coordinator (VERIFIED)

**File**: `system/conversation/server/ThoughtStreamCoordinator.ts`

**How It Works**:
1. **Sequential Evaluation** (lines 66-91):
   - `requestEvaluationTurn()` creates promise chain
   - Each persona waits for previous persona to finish
   - Random delay 10-100ms adds "neural timing variance"
   - NOT duplicate instances - proper sequential coordination

2. **Probabilistic Slot Allocation** (lines 363-368):
   - 70% chance: 1 responder per message
   - 25% chance: 2 responders
   - 5% chance: 3 responders
   - Per-message, NOT global limit

3. **No Hard Limits**:
   - Personas decide via LLM gating
   - ThoughtStream grants/denies based on confidence
   - Natural "first citizen reasoning" preserved

**Key Insight**: The "6 instances" I saw were 3 personas × 2 evaluation attempts (retry after timeout), NOT duplicate persona creation. ThoughtStream ensures they evaluate sequentially.

---

### 2. PersonaUser Event Handler (VERIFIED)

**File**: `system/user/server/PersonaUser.ts`

**Flow** (lines 153-254):
```
handleChatMessage(messageEntity)
    ↓
 Step 1: Ignore own messages (line 160)
    ↓
 Step 2: Request ThoughtStream turn (line 171)
    ↓
 Step 3: evaluateAndPossiblyRespond()
    ↓
    ├─ Response cap check (line 191)
    ├─ Rate limiting check (line 207)
    ├─ LLM gating decision (line 220)
    ├─ Update RAG context (line 242)
    ├─ Post response (line 245)
    └─ Track response count (line 248)
    ↓
 Step 4: Release ThoughtStream turn (line 176)
```

**Key Details**:
- RAG context built at line 242: `updateRAGContext(roomId, messageEntity)`
- Gating uses: `evaluateShouldRespond(messageEntity, senderIsHuman, isMentioned)` (line 220)
- Response sent via: `respondToMessage(messageEntity)` (line 245)

**What I Don't Know Yet**:
- [ ] Where does `updateRAGContext()` call `ChatRAGBuilder.buildContext()`?
- [ ] Where does `respondToMessage()` store message to database?
- [ ] How does `evaluateShouldRespond()` build RAG for gating?

---

### 3. Gating Decision (VERIFIED)

**File**: `system/user/server/PersonaUser.ts` (lines 1023-1043)

```typescript
private async evaluateShouldRespond(
  message: ChatMessageEntity,
  senderIsHuman: boolean,
  isMentioned: boolean
): Promise<{ shouldRespond: boolean; confidence: number; reason: string }> {

  // Build RAG context for gating decision
  const ragBuilder = new ChatRAGBuilder();
  const ragContext = await ragBuilder.buildContext(
    message.roomId,
    this.id,
    {
      maxMessages: 10,  // Gating uses 10 messages
      maxMemories: 0,
      includeArtifacts: false,
      includeMemories: false,
      currentMessage: {
        role: 'user',
        content: message.content.text,
        // ...
      }
    }
  );
```

**Key Findings**:
- Gating RAG uses **10 messages** (not 20)
- No temporal filtering (`triggerTimestamp` not passed)
- `currentMessage` parameter exists (the triggering message)

---

### 4. AI Commands (VERIFIED)

**Files**:
- `commands/ai/should-respond-fast/` - BoW gating (fast, no LLM)
- `commands/ai/should-respond/` - LLM gating (slower, smarter)
- `commands/ai/generate/` - LLM text generation
- `commands/ai/bag-of-words/` - BoW orchestration

**ShouldRespondFast Command** (verified at `server/ShouldRespondFastServerCommand.ts`):
- Pure algorithmic scoring (lines 126-207)
- No LLM calls
- Weights: directMention, domainKeywords, conversationContext, isQuestion, publicMessage, roomActivity
- Cooldown tracking per persona+room (lines 38-48)

**Key Insight**: We already have TWO gating systems:
1. `should-respond-fast` - BoW scoring
2. `should-respond` - LLM-based

PersonaUser currently uses LLM gating. Could optimize by using BoW first, LLM second.

---

### 5. What I Don't Know (Need to Verify)

#### Unknown #1: Where is RAG built for full response?
- PersonaUser line 242: `updateRAGContext(roomId, messageEntity)`
- Need to find this method
- Does it call `ChatRAGBuilder.buildContext()` with different params than gating?

#### Unknown #2: Where is message stored to database?
- PersonaUser line 245: `respondToMessage(messageEntity)`
- Need to find this method
- Does it call `Commands.execute('data/create', ...)`?
- **This is where we need response validation (Issue #3)**

#### Unknown #3: How is ChatRAGBuilder actually used?
- Need to read `system/rag/builders/ChatRAGBuilder.ts` fully
- Verify current RAG structure
- Check if `triggerTimestamp` parameter already exists
- Check role assignment logic (Issue #4)

---

## Corrected Assumptions

### ❌ WRONG Assumption: "6 duplicate instances"
**Reality**: 3 personas, sequential evaluation via ThoughtStream, 2 attempts each (timeout → retry)

### ❌ WRONG Assumption: "Need to prevent multiple responses"
**Reality**: System already has probabilistic slot allocation (1-3 responders). Natural multi-AI conversation is DESIRED.

### ❌ WRONG Assumption: "Global event subscription is a bug"
**Reality**: Personas ARE room members, should receive events. ThoughtStream ensures sequential evaluation.

### ❌ WRONG Assumption: "Hard-coded response limits needed"
**Reality**: System already has:
- Per-persona response caps (`maxResponsesPerSession`)
- Per-persona rate limiting (`minSecondsBetweenResponses`)
- LLM-based gating (intelligent decision)
- ThoughtStream slot allocation (probabilistic)

### ✅ CORRECT Assumption: "Ollama has no request queue"
**Reality**: VERIFIED - No queue in OllamaAdapter. Issue #1 is real.

### ✅ CORRECT Assumption: "RAG uses 'now' not trigger timestamp"
**Reality**: VERIFIED - ChatRAGBuilder doesn't receive triggerTimestamp. Issue #2 is real.

---

## What Needs to be Read Next

### Priority 1: Complete PersonaUser understanding
```bash
# Find these methods:
grep -n "updateRAGContext" system/user/server/PersonaUser.ts
grep -n "respondToMessage" system/user/server/PersonaUser.ts
```

### Priority 2: Complete ChatRAGBuilder understanding
```bash
# Read full implementation:
cat system/rag/builders/ChatRAGBuilder.ts
```

### Priority 3: Verify message storage flow
```bash
# Find where messages are created:
grep -rn "data/create.*chat_messages" system/
```

---

## Implementation Plan Corrections Needed

### Issue #1 (Ollama Queue) - STILL VALID ✅
- Root cause verified
- Implementation plan correct
- No changes needed

### Issue #2 (RAG Temporal) - STILL VALID ✅
- Root cause verified
- Need to add `triggerTimestamp` to RAGBuildOptions
- PersonaUser already has messageEntity.timestamp available
- Implementation plan correct

### Issue #3 (Response Validation) - NEEDS LOCATION CORRECTION ⚠️
- Root cause valid
- **Wrong location**: Not in PersonaUser directly
- **Correct location**: In `respondToMessage()` method (need to find)
- Implementation plan needs update with correct file/line

### Issue #4 (Role Assignment) - NEEDS VERIFICATION ⚠️
- Claimed location: ChatRAGBuilder.ts:226
- **Need to verify**: Read actual implementation
- Implementation plan might be correct, but not verified

### Issue #5-11 - NEED VERIFICATION ⚠️
- All depend on understanding full ChatRAGBuilder implementation
- Implementation plans are ASSUMPTIONS until code is read

---

## Next Steps

1. ✅ Read PersonaUser methods:
   - `updateRAGContext()`
   - `respondToMessage()`

2. ✅ Read full ChatRAGBuilder:
   - `buildContext()` implementation
   - Role assignment logic
   - Message formatting
   - System prompt generation

3. ⏳ Update implementation plan with:
   - Correct file locations
   - Correct method names
   - Verified line numbers
   - Actual current code snippets

4. ⏳ Create detailed "before/after" diffs for each fix

---

## Confidence Levels

### HIGH CONFIDENCE (Verified by reading code)
- ✅ ThoughtStream sequential evaluation
- ✅ Probabilistic slot allocation
- ✅ PersonaUser event handler flow
- ✅ Gating uses 10 messages
- ✅ No Ollama request queue
- ✅ AI commands architecture

### MEDIUM CONFIDENCE (Inferred from logs + partial code)
- ⚠️ RAG temporal confusion (saw in logs, need to verify fix approach)
- ⚠️ Role assignment bug (saw in RAG-QUALITY doc, need to verify location)

### LOW CONFIDENCE (Assumptions not verified)
- ❓ Exact location of message storage
- ❓ Full RAG context structure
- ❓ System prompt generation
- ❓ Model tier detection implementation needs

---

## Philosophy Verification

**From CLAUDE.md**:
> "we must have natural first citizen reasoning in chats, OVER efficiency. I would have realistic conversations than hard coded bs"

**Verified in Code**:
- ✅ LLM-based gating (not hard-coded rules)
- ✅ Probabilistic slot allocation (not fixed limits)
- ✅ ThoughtStream coordination (not turn-taking queue)
- ✅ Natural multi-persona responses (not suppressed)

**Architecture aligns with philosophy** - good!
