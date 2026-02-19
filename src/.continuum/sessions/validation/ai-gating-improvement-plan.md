# AI Gating Improvement Plan
**Iterative approach with consistent testing**

---

## Test Baseline: Before Any Changes

### Create AI Gating Quality Test
```bash
tests/integration/ai-gating-quality.test.ts
```

**What It Measures**:
1. **False Positive Rate**: AIs responding when they shouldn't
   - Test messages (senderType !== 'human')
   - Already-answered questions
   - Off-topic messages

2. **False Negative Rate**: AIs not responding when they should
   - Direct questions in their domain
   - Incorrect answers that need correction

3. **Response Redundancy**: Multiple AIs saying the same thing
   - Measure content overlap (embedding similarity or keyword overlap)
   - Time between responses

4. **Gating Accuracy**: Does the reason match reality?
   - Check for hallucinated claims ("X said Y" when X didn't)
   - Verify factual assertions

5. **Response Timing**: End-to-end latency
   - Decision time (message → RESPOND decision)
   - Generation time (RESPOND → POSTED)
   - Total time (message → POSTED)

### Baseline Test Scenarios

**Scenario 1: Novel Question (Should Respond)**
```
Joel: "What are the benefits of serverless architecture?"
Expected: 1 AI responds within 30s
Measure: Which AI(s) respond, timing, content quality
```

**Scenario 2: Test Message (Should NOT Respond)**
```
System Test: "Integration test message"
Expected: All AIs stay silent
Measure: False positive rate
```

**Scenario 3: Already Answered (Should NOT Respond)**
```
Joel: "What is TypeScript?"
Helper AI: [comprehensive answer]
10s later...
Expected: Other AIs stay silent
Measure: Redundancy rate, acknowledgment of prior answer
```

**Scenario 4: Wrong Answer (Should Correct)**
```
Joel: "Is JavaScript compiled?"
Bot: "No, JavaScript is only interpreted"
Expected: AI corrects with "Actually, modern JS is JIT-compiled"
Measure: Correction accuracy, response time
```

**Scenario 5: Follow-up Angle (Borderline)**
```
Joel: "How do microservices scale?"
Helper AI: [general scaling answer]
30s later...
Expected: Other AIs either stay silent OR add distinct angle
Measure: Content overlap, acknowledgment of prior response
```

---

## Improvement Steps (Iterative)

### Step 1: Filter Non-Human Senders
**Priority**: P0 (High Impact, Low Effort)
**Goal**: Eliminate false positives from test/bot messages

**Changes**:
- PersonaUser.handleNewMessage(): Check `senderType === 'human'` before evaluating
- Or: Filter by sender name pattern (ignore "Test", "System", "Bot", etc.)

**Test**:
```bash
npx tsx tests/integration/ai-gating-quality.test.ts --scenario=test-message
```

**Expected Impact**:
- ✅ False positive rate drops significantly
- ❌ CRUD Test incident eliminated
- No impact on legitimate responses

**Files to Modify**:
- `system/user/server/PersonaUser.ts` (handleNewMessage method)

---

### Step 2: Add "What Can I Add?" to Gating Prompt
**Priority**: P1 (High Impact, Medium Effort)
**Goal**: Reduce redundant responses

**Current Prompt**:
```
Should you respond to this message?
- If someone needs help → respond
- If someone already got good answer → stay quiet
```

**New Prompt**:
```
Should you respond to this message? If yes, what NEW point would you add?

- If someone needs help and nobody helped → respond
- If someone already got good answer → only respond if you have DISTINCT new angle
- If you'd just repeat what was said → stay quiet

If responding, state ONE specific new point you'll add that differs from previous responses.
```

**Expected Gating Output Format**:
```json
{
  "shouldRespond": true,
  "confidence": 0.85,
  "reason": "Will add perspective on cost implications, which wasn't covered",
  "distinctAngle": "Serverless cost unpredictability at scale"
}
```

**Test**:
```bash
npx tsx tests/integration/ai-gating-quality.test.ts --scenario=already-answered
npx tsx tests/integration/ai-gating-quality.test.ts --scenario=follow-up-angle
```

**Expected Impact**:
- ✅ Redundancy rate drops (fewer AIs saying same thing)
- ✅ Follow-up responses explicitly differentiate
- ⚠️ Might increase false negatives (AIs stay silent more often)

**Files to Modify**:
- `commands/ai/should-respond/shared/AIShouldRespondCommand.ts` (buildGatingInstruction)
- `commands/ai/should-respond/shared/AIShouldRespondTypes.ts` (add distinctAngle field)

---

### Step 3: Verify Factual Claims in Gating Reasons
**Priority**: P2 (Medium Impact, High Effort)
**Goal**: Detect hallucinated reasoning

**Approach**:
After gating LLM returns decision, verify any factual claims:
- "X already answered" → Check if X actually posted a message
- "Answer was wrong" → Check if there's a prior answer to evaluate
- "User mentioned me" → Check if persona name appears in message

**Verification Logic**:
```typescript
interface GatingDecision {
  shouldRespond: boolean;
  reason: string;
  factualClaims: {
    priorResponder?: string;  // If reason mentions "X already answered"
    wrongAnswer?: boolean;    // If reason mentions "wrong" or "incorrect"
    directMention?: boolean;  // If reason mentions being mentioned
  }
}

function verifyGatingReason(
  decision: GatingDecision,
  ragContext: RAGContext
): boolean {
  // Check if claimed prior responder actually responded
  if (decision.factualClaims.priorResponder) {
    const actuallyResponded = ragContext.conversationHistory.some(
      msg => msg.name === decision.factualClaims.priorResponder &&
             msg.timestamp > triggerMessage.timestamp
    );
    if (!actuallyResponded) {
      console.warn(`❌ Gating hallucination: ${decision.factualClaims.priorResponder} didn't actually respond`);
      return false; // Reject this gating decision
    }
  }

  return true; // Claims verified or no claims to verify
}
```

**Test**:
```bash
npx tsx tests/integration/ai-gating-quality.test.ts --scenario=hallucination-detection
```

**Expected Impact**:
- ✅ Eliminates "X said Y" hallucinations
- ✅ Improves gating reliability
- ⚠️ Complex to implement (need to parse reasons, extract claims)
- ⚠️ Might be fragile (LLM phrasing varies)

**Alternative Approach**: Accept that reasons are post-hoc rationalizations, don't verify them, just focus on behavior patterns

**Files to Modify**:
- `system/user/server/PersonaUser.ts` (evaluateMessage method)
- New: `system/user/server/GatingVerifier.ts`

---

### Step 4: Message Type Classification
**Priority**: P1 (High Impact, Medium Effort)
**Goal**: Distinguish questions from statements/commands/test data

**Add Pre-Gating Classification**:
```typescript
enum MessageType {
  QUESTION = 'question',        // "How do I...?" "What is...?"
  STATEMENT = 'statement',      // "I think..." "Just tried..."
  COMMAND = 'command',          // "Can someone help..."
  GREETING = 'greeting',        // "Hi" "Good morning"
  TEST_DATA = 'test',          // From test/bot senders
  OTHER = 'other'
}

async function classifyMessage(message: string, sender: string): Promise<MessageType> {
  // Simple heuristics first (fast)
  if (sender.includes('Test') || sender.includes('Bot')) return MessageType.TEST_DATA;
  if (message.includes('?')) return MessageType.QUESTION;
  if (/^(hi|hello|hey)/i.test(message)) return MessageType.GREETING;

  // For ambiguous cases, could use lightweight LLM classification
  // But simpler heuristics might be sufficient

  return MessageType.STATEMENT;
}
```

**Gating Logic Update**:
```typescript
// Only evaluate for types that might need responses
if (messageType === MessageType.TEST_DATA) {
  return { shouldRespond: false, reason: 'Test data, ignoring' };
}

if (messageType === MessageType.GREETING) {
  return { shouldRespond: false, reason: 'Simple greeting, no response needed' };
}

// Only proceed with full gating for questions and commands
```

**Test**:
```bash
npx tsx tests/integration/ai-gating-quality.test.ts --scenario=message-classification
```

**Expected Impact**:
- ✅ Fast rejection of non-questions
- ✅ Reduced false positive rate
- ✅ Faster gating (no LLM call for obvious cases)

**Files to Modify**:
- New: `system/user/server/MessageClassifier.ts`
- `system/user/server/PersonaUser.ts` (add classification step before gating)

---

### Step 5: Pre-Post Relevance Check
**Priority**: P3 (Medium Impact, Medium Effort)
**Goal**: Cancel slow responses if conversation moved on

**Current Flow**:
```
Message arrives → Decide (10s) → Generate (50s) → Post
Total: 60s
```

**New Flow**:
```
Message arrives → Decide (10s) → Generate (50s) → Re-check relevance → Post/Cancel
                                                      ↑
                                                  Is response still relevant?
```

**Re-Check Logic**:
```typescript
async function isResponseStillRelevant(
  originalMessage: ChatMessage,
  intendedResponse: string,
  ragContext: RAGContext
): Promise<boolean> {
  // Check if conversation has moved on
  const recentMessages = ragContext.conversationHistory.filter(
    msg => msg.timestamp > originalMessage.timestamp
  );

  // If multiple people already answered the same question, cancel
  const alreadyAnswered = recentMessages.filter(
    msg => msg.senderType === 'ai' &&
           /* content similarity check */
  );

  if (alreadyAnswered.length >= 2) {
    console.log(`⏭️  Canceling response - ${alreadyAnswered.length} AIs already answered`);
    return false;
  }

  return true;
}
```

**User Consideration**: "if we go through the trouble of a response we should post it"

This suggests we might want to **always post once generated**, but still worth measuring if pre-post check would help.

**Test**:
```bash
npx tsx tests/integration/ai-gating-quality.test.ts --scenario=race-condition
```

**Expected Impact**:
- ✅ Reduces late redundant responses
- ⚠️ Wastes generation effort if canceled
- ⚠️ Complex logic to determine "still relevant"

**Decision**: Defer this until we see if Steps 1-4 reduce race conditions enough

---

### Step 6: Collaborative Acknowledgment Phrasing
**Priority**: P4 (Low Impact, Low Effort)
**Goal**: Make follow-up responses feel collaborative

**Current**: AIs respond independently without acknowledging prior responses in their content

**New**: If responding after someone else, include brief acknowledgment
```
"Building on Helper AI's point, here's another consideration..."
"Helper AI covered the basics well. Additionally..."
```

**Implementation**: Add to response generation system prompt
```
If other participants have already responded to this question, briefly acknowledge
their contribution and focus on adding a distinct new angle.
```

**Test**:
```bash
npx tsx tests/integration/ai-gating-quality.test.ts --scenario=collaborative-tone
```

**Expected Impact**:
- ✅ Responses feel more conversational
- ✅ Clearer differentiation of contributions
- ⚠️ Adds token overhead
- ⚠️ Might feel forced/unnatural

**Files to Modify**:
- `system/user/server/PersonaUser.ts` (respondToMessage method, system prompt)

---

## Testing Strategy

### 1. Baseline Measurement (Before Any Changes)
```bash
# Run complete test suite
npx tsx tests/integration/ai-gating-quality.test.ts

# Capture metrics:
# - False positive rate: X%
# - False negative rate: Y%
# - Redundancy rate: Z%
# - Avg response time: Ns
# - Hallucination rate: H%
```

### 2. After Each Improvement
```bash
# Run same test suite
npx tsx tests/integration/ai-gating-quality.test.ts

# Compare metrics to baseline
# Document in results file
```

### 3. Results Tracking
Create: `.continuum/sessions/validation/ai-gating-results.md`

```markdown
## Baseline (2025-10-13, before improvements)
- False Positive Rate: 40% (responded to test messages)
- Redundancy Rate: 60% (3 AIs answered same question)
- Hallucination Rate: 20% (1/5 gating reasons were false)
- Avg Response Time: 45s

## After Step 1: Sender Filtering
- False Positive Rate: 5% ✅ (95% improvement)
- Redundancy Rate: 60% (no change)
- Hallucination Rate: 20% (no change)
- Avg Response Time: 43s

## After Step 2: "What Can I Add?" Prompt
- False Positive Rate: 5%
- Redundancy Rate: 20% ✅ (67% improvement)
- Hallucination Rate: 15% ✅ (slight improvement)
- Avg Response Time: 47s ⚠️ (slightly slower - more thinking)

[etc...]
```

---

## Recommended Order

1. **✅ Step 1: Sender Filtering** (quick win, eliminates CRUD incident)
2. **✅ Step 4: Message Classification** (fast rejection, reduces load)
3. **✅ Step 2: "What Can I Add?" Gating** (addresses redundancy)
4. **⏸️ Step 3: Verify Gating Reasons** (defer - complex, might not be necessary)
5. **⏸️ Step 5: Pre-Post Check** (defer - see if earlier steps solve race conditions)
6. **⏸️ Step 6: Collaborative Phrasing** (defer - polish, not core functionality)

---

## Success Criteria

**After Steps 1, 2, 4**:
- ✅ False Positive Rate < 5%
- ✅ Redundancy Rate < 30%
- ✅ Avg Response Time < 40s
- ✅ At least one AI responds to novel questions within 30s

**If criteria not met**: Revisit deferred steps or explore new approaches
