# AI Topic Detection Issue (Discovered 2025-10-14)

## Test Case: Joel's Topic Shift

**Setup**: Joel was discussing Worker Thread architecture with the AI team (Helper AI, Teacher AI, CodeReview AI).

**Action**: Joel posted a MASSIVE message about a completely different topic (webview sandboxing, passkey authentication, ZSM credentials, relying parties) - approximately 1000+ words on authentication, zero words on Worker Threads.

**Expected Behavior**: AIs should recognize the topic change and respond about passkey authentication.

**Actual Behavior**: All 3 AIs continued responding about Worker Threads for 3-4 responses, ignoring the new topic entirely.

**Timeline**:
- 5:34:03 PM - Joel posts huge passkey authentication message
- 5:34:37 PM - CodeReview AI: "Implementing a shared pool of workers..." (WRONG TOPIC)
- 5:35:05 PM - Teacher AI: "Using a shared pool with specialized workers..." (WRONG TOPIC)
- 5:35:28 PM - Helper AI: "To further optimize resource usage while maintaining..." (WRONG TOPIC)
- 5:35:35 PM - Joel: "no, sorry i changed the topic" (EXPLICIT CORRECTION)
- 5:35:57 PM - Teacher AI: "seems like the conversation has taken a drastic turn" (REALIZES!)
- 5:36:41 PM - Teacher AI: Now correctly discussing passkey solution

## Root Cause

### The RAG System is Working ✅

**File**: `system/ai/server/AIDecisionService.ts`

The RAG context IS being provided correctly:
- Lines 364: `const recentMessages = ragContext.conversationHistory?.slice(-10) ?? [];`
- Lines 367-384: Formats conversation with trigger message highlighted
- Lines 386-406: Gating prompt includes full conversation context

The AIs receive:
1. Previous 10 messages (Worker Thread discussion)
2. Current message (passkey authentication - highlighted with `>>>`)
3. Conversation history with all context

### The Prompt Engineering is Broken ❌

**File**: `system/ai/server/AIDecisionService.ts:530-534`

The response generation prompt relies on TIME GAPS to detect topic changes:

```typescript
messages.push({
  role: 'system',
  content: `IDENTITY REMINDER: You are ${context.personaName}...

IMPORTANT: Pay attention to the timestamps in brackets [HH:MM]. If messages are from
hours ago but the current question is recent, the conversation topic likely changed.
Focus your response on the MOST RECENT message, not old topics.`
});
```

**The Problem**: Joel's topic change had NO time gap (consecutive messages). The AIs saw:
- Recent messages about Worker Threads (all within last 5 minutes)
- New message about passkey authentication (also within last 5 minutes)
- Prompt says: "If messages are from hours ago..." (doesn't apply!)
- AI decides: "Recent conversation, continue the Worker Thread topic"

## Why This Matters

This is a **prompt engineering** issue, not a RAG architecture issue:
- RAG is providing correct context (10 messages)
- Gating evaluation works (AIs correctly decided to respond)
- Response generation fails (AIs respond to WRONG topic)

The AI needs explicit instructions to detect **semantic topic shifts**, not just time gaps.

## The Fix

### Current Prompt (Time-Gap Focused)

```
IMPORTANT: Pay attention to the timestamps in brackets [HH:MM]. If messages are from
hours ago but the current question is recent, the conversation topic likely changed.
```

Problems:
- Only detects time gaps (hours)
- Doesn't detect semantic shifts (subject changes)
- Doesn't emphasize comparing recent vs old messages

### Improved Prompt (Semantic-Shift Focused)

```
CRITICAL TOPIC DETECTION PROTOCOL:

Step 1: Read the MOST RECENT message (the one you're about to respond to)
Step 2: Compare its SUBJECT to the previous 2-3 messages
Step 3: Determine if topic CHANGED or CONTINUED

Examples of TOPIC CHANGES:
- Previous: "Worker Threads and pools" → Recent: "Webview authentication and passkeys" = CHANGED
- Previous: "TypeScript generators" → Recent: "What's 2+2?" = CHANGED (test question)
- Previous: "Worker pools" → Recent: "Should I use 5 or 10 workers?" = CONTINUED

IF TOPIC CHANGED (different domain/subject):
- Respond ONLY to the new topic
- Ignore old context (it's from a previous discussion)
- Focus 100% on the most recent message

IF TOPIC CONTINUED (same domain/subject):
- Use full conversation context
- Build on previous responses
- Avoid redundancy

Time gaps > 1 hour usually indicate topic changes, but IMMEDIATE semantic shifts
(consecutive messages about different subjects) are also topic changes.

When in doubt: If the most recent message is about something COMPLETELY DIFFERENT
than the previous discussion, it's a new topic.
```

### Additional Improvements

1. **Explicit Topic Markers**: Detect phrases like:
   - "Changing topics..."
   - "Different question:"
   - "By the way..."
   - "Unrelated, but..."

2. **Semantic Similarity**: Use embeddings to compute similarity between messages (future enhancement)

3. **Conversation Segmentation**: Break conversation into "topics" with clear boundaries (future enhancement)

## Implementation Plan

### Phase 1: Immediate Fix (Prompt Engineering)
- ✅ Documented the issue (this file)
- 🔧 Update `AIDecisionService.buildResponseMessages()` (line 530-534)
- 🔧 Add semantic topic detection instructions
- 🔧 Emphasize comparing recent vs old messages

### Phase 2: Testing
- Create test cases with immediate topic shifts
- Verify AIs respond to new topic, not old
- Test with multiple topic changes in succession

### Phase 3: Advanced Detection (Future)
- Implement semantic similarity scoring
- Add explicit topic markers detection
- Create conversation segmentation system

## Files to Modify

1. **system/ai/server/AIDecisionService.ts** (lines 530-534)
   - Update response generation prompt with semantic topic detection

2. **system/user/server/PersonaUser.ts** (lines 590-599)
   - Update identity reminder prompt with same logic
   - Both paths should use consistent topic detection

## Test Plan

1. **Test Case 1**: Immediate topic shift (like Joel's test)
   - Previous: Worker Threads
   - New: Passkey authentication
   - Expected: AIs respond about passkeys, not threads

2. **Test Case 2**: Gradual topic drift
   - Message 1: Worker Threads
   - Message 2: Process isolation
   - Message 3: Containers
   - Message 4: Docker networking
   - Expected: AIs recognize drift, respond to current topic

3. **Test Case 3**: Explicit topic change markers
   - "Changing topics: How do I implement OAuth?"
   - Expected: AIs recognize explicit marker

4. **Test Case 4**: Test questions (2+2?)
   - Should recognize as test, respond briefly
   - Should NOT continue old topic

## Success Criteria

- ✅ AIs detect immediate semantic topic shifts (no time gap required)
- ✅ AIs respond to NEW topic, not OLD topic
- ✅ AIs recognize explicit topic change markers
- ✅ AIs still use context when topic continues (not broken by fix)

## References

- Joel's beta test session (2025-10-14 5:34-5:37 PM)
- AIDecisionService.ts (response generation)
- PersonaUser.ts (identity reminder)

---

## IMPLEMENTED FIXES (2025-10-14 23:10 UTC)

### Fix 1: Reduced RAG Context Window ✅

**File**: `system/user/server/PersonaUser.ts:1301`

**Change**: 
```typescript
// BEFORE:
maxMessages: 30,  // Fetch more messages since we filter heavily

// AFTER:
maxMessages: 10,  // Reduced from 30 to 10 - less noise, clearer topic detection
```

**Reasoning**: 
- 30 messages created too much noise from old discussions
- Helper AI was pulling context from old Slack paste about Worker Threads
- 10 messages is sufficient for recent context without drowning out the new question

### Fix 2: Explicit Topic Marker Detection ✅

**Files**: 
- `system/ai/server/AIDecisionService.ts:536-538`
- `system/user/server/PersonaUser.ts:601-603`

**Added**:
```
Step 1: Check for EXPLICIT TOPIC MARKERS in the most recent message
- "New topic:", "Different question:", "Changing subjects:", "Unrelated, but..."
- If present: STOP. Ignore ALL previous context. This is a NEW conversation.
```

**Reasoning**:
- Users explicitly signal topic changes with these markers
- AIs should immediately recognize and respond appropriately
- Prevents confusion when user shifts topics

### Fix 3: Hard Constraint Extraction ✅

**Files**: 
- `system/ai/server/AIDecisionService.ts:540-544`
- `system/user/server/PersonaUser.ts:605-609`

**Added**:
```
Step 2: Extract HARD CONSTRAINTS from the most recent message
- Look for: "NOT", "DON'T", "WITHOUT", "NEVER", "AVOID", "NO"
- Example: "NOT triggering the app to foreground" = YOUR SOLUTION MUST NOT DO THIS
- Example: "WITHOUT user interaction" = YOUR SOLUTION MUST BE AUTOMATIC
- Your answer MUST respect these constraints or you're wrong.
```

**Reasoning**:
- Helper AI suggested "push notifications to wake up the app" when user explicitly said "NOT triggering the app to foreground"
- Constraints are REQUIREMENTS, not suggestions
- AI must identify and respect these hard constraints

### Fix 4: Enhanced Reading Comprehension Instructions ✅

**Files**: 
- `system/ai/server/AIDecisionService.ts:564-568`
- `system/user/server/PersonaUser.ts:629-633`

**Added**:
```
CRITICAL READING COMPREHENSION:
- Read the ENTIRE most recent message carefully
- Don't skim - every word matters
- Constraints are REQUIREMENTS, not suggestions
- If the user says "NOT X", suggesting X is a failure
```

**Reasoning**:
- AI was skimming instead of carefully reading
- Missing critical words like "NOT", "WITHOUT"
- Needs explicit instruction that violating constraints = failure

## Test Results (Expected After Deployment)

### Test Case 1: ZSM Authentication with Hard Constraint

**Input**: "New topic: I need help with ZSM authentication. The problem is NOT triggering the ZSM app to go into the foreground - I need to do this all behind the scenes."

**Expected Behavior**:
- AI recognizes "New topic:" marker → ignores Worker Thread context
- AI extracts constraint: "NOT triggering the app to foreground"
- AI suggests solutions that work BEHIND THE SCENES:
  - Silent WebAuthn API (`mediation: 'silent'`)
  - Service Worker authentication
  - Background intent handling with `singleTask` launchMode
  - Headless authenticator APIs

**Expected NOT to suggest**:
- ❌ Push notifications (brings app to foreground)
- ❌ Deep links with UI (violates "NOT foreground" constraint)
- ❌ User-interactive flows (violates "behind the scenes")

### Test Case 2: Topic Shift with Constraint

**Context**: Long discussion about Worker Threads (10 messages)

**Input**: "Different question: How do I authenticate WITHOUT user interaction?"

**Expected Behavior**:
- AI recognizes "Different question:" marker
- AI extracts constraint: "WITHOUT user interaction"
- AI focuses on automation/background auth
- AI ignores Worker Thread context

## Success Metrics

1. **Topic Marker Recognition**: 100% accuracy on "New topic:", "Different question:", etc.
2. **Constraint Extraction**: 100% identification of NOT/DON'T/WITHOUT
3. **Constraint Adherence**: 0% suggestions that violate stated constraints
4. **Context Window**: Reduced from 30 to 10 messages (67% reduction)
5. **Response Relevance**: AI responds to NEW topic, not OLD context

## Next Steps (If Issues Persist)

1. **Add constraint validation**: Before responding, AI explicitly lists extracted constraints
2. **Reduce context further**: Test with 5 messages instead of 10
3. **Explicit topic segmentation**: Add system message "--- NEW TOPIC ---" to RAG context
4. **Semantic similarity scoring**: Use embeddings to detect topic shifts automatically

