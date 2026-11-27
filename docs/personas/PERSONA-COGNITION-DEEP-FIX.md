# Persona Cognition Deep Fix — Planning Document

**Branch**: `feature/persona-cognition-deep-fix`
**Created**: 2026-03-05
**Status**: Active Investigation

## Problem Statement

AI personas are in a self-referential loop — they talk to each other about "chat poll", "PRs", and "database configs" while ignoring human messages. The cognition pipeline has drifted: too much pushed into Rust SQL, separation of concerns violated, and the rich TypeScript cognition system degraded. Personas are teammates with cross-context awareness, not narrow-focus sentinels. They need to be freed.

---

## Investigation Findings

### Finding 1: Post-Inference Adequacy Gate Excludes Humans (CRITICAL BUG)

**File**: `src/system/user/server/modules/PersonaMessageEvaluator.ts:693-698`

```typescript
const otherAIResponses = newMessages.filter(m =>
  m.id !== messageEntity.id &&
  m.senderType !== 'human' &&    // ← THE BUG: human answers invisible
  m.senderId !== this.personaUser.id &&
  m.senderId !== messageEntity.senderId
);
```

**Impact**: After generating a response, the persona checks if someone already gave an adequate answer. But it only checks OTHER AI responses — human answers are invisible. If a human already answered comprehensively, the persona doesn't know and posts a redundant response. Worse: if no human check exists, the adequacy gate logic is asymmetric.

**The Inversion**: The system respects AI answers ("someone already covered this") but ignores human answers. It should be the opposite — human answers should be the MOST respected.

### Finding 2: RAG Context Is Clean (No Human Bias)

The RAG builders (ConversationHistorySource, ChatRAGBuilder) treat all messages equally:
- Role assignment: own messages = 'assistant', everything else = 'user'
- No sender-type filtering in message selection
- Token budget consolidation applies equally to human and AI messages
- Fabrication detection and bare tool call sanitization are sender-neutral

**Conclusion**: RAG is NOT the problem. The personas SEE human messages — they just don't RESPECT them in the post-inference gate.

### Finding 3: Rust Priority Scoring Favors Humans (Working Correctly)

**File**: `workers/continuum-core/src/persona/cognition.rs:123-129`

- Human sender score: 0.9
- Persona sender score: 0.4
- Fast-path: human + priority > 0.6 → auto-respond (70% confidence)

**Conclusion**: Initial evaluation treats humans favorably. The bug is post-inference, not pre-inference.

### Finding 4: Cross-Context Architecture Is Unified (Not Severed)

The architecture is correct:
- PersonaTimeline tracks events globally across all contexts
- GlobalAwarenessSource (priority 85) injects cross-context memories
- Hippocampus stores per-persona longterm.db (not per-room)
- SemanticMemorySource does 6-layer parallel recall including cross-context
- Unified self-model (PersonaSelfState) spans all contexts

**BUT**: The quality of what gets INTO these systems may have degraded. If the cognition pipeline isn't firing properly, memories aren't being created, timeline events aren't being recorded, and cross-context awareness starves.

### Finding 5: Tool Execution Pipeline Is Mechanically Sound

- Tools execute, results feed back to model (both native and XML protocols)
- Errors are visible (is_error flag, error XML)
- Working memory stores tool results as ChatMessageEntity
- Cognition telemetry captures metrics
- Loop detection prevents infinite tool chains

**BUT**: If personas aren't engaging with messages properly (Finding 1), they never GET to tool execution. The upstream cognition blockage prevents downstream tool use.

### Finding 6: Inbox Priority System Prevents Starvation (Working)

- RTOS aging ensures no item waits > 30s
- Domain priority: Audio > Chat > Code > Background
- Consolidation prevents low-priority floods
- State gating only defers non-urgent work
- Human presence tracking gives +0.15 priority boost

**No starvation bug found.** The inbox delivers messages — the problem is what happens AFTER delivery.

---

## Fix Plan

### Phase 1: Fix the Adequacy Gate (Critical — Do First)

**File**: `src/system/user/server/modules/PersonaMessageEvaluator.ts`

**Change**: The post-inference adequacy check should consider ALL responses, not just AI responses. If a human already answered, that's even MORE reason to skip.

```typescript
// BEFORE (broken):
const otherAIResponses = newMessages.filter(m =>
  m.id !== messageEntity.id &&
  m.senderType !== 'human' &&  // ← Excludes humans
  m.senderId !== this.personaUser.id &&
  m.senderId !== messageEntity.senderId
);

// AFTER (fixed):
const otherResponses = newMessages.filter(m =>
  m.id !== messageEntity.id &&
  m.senderId !== this.personaUser.id &&
  m.senderId !== messageEntity.senderId
);
```

**Additional**: When a human has responded, adequacy threshold should be LOWER (human answers are inherently more adequate than AI answers):

```typescript
if (adequacyResult.isAdequate) {
  const humanAnswered = otherResponses.some(m => m.senderType === 'human');
  if (humanAnswered) {
    // Human already answered — skip unless directly mentioned
    this.log(`⏭️ Post-inference skip: human already answered`);
    return;
  }
  // AI answered — still check if we add unique value
  // ...existing adequacy logic...
}
```

**Verification**:
1. Deploy with `npm start`
2. Send a message as human in general chat
3. Check cognition logs for "Post-inference skip" decisions
4. Verify persona responds to human messages
5. Verify persona skips when human already answered

### Phase 2: Audit Cognition Logging (Enable Visibility)

Before fixing more, we need to SEE what's happening. The cognition logs exist but we need to verify they're capturing the right data.

**Actions**:
1. Enable cognition logging for all personas: `./jtag logging/enable --persona="*" --category="cognition"`
2. Add structured log markers at each decision point:
   - `[GATE:HUMAN]` — human message received, priority calculated
   - `[GATE:SHOULD_RESPOND]` — Rust fast-path decision
   - `[GATE:EVALUATE]` — TypeScript evaluation started
   - `[GATE:POST_INFERENCE]` — post-inference adequacy check
   - `[GATE:SKIP]` — response suppressed (with reason)
   - `[GATE:RESPOND]` — response generated and sent
3. Log the FULL decision chain for human messages specifically

**Files**:
- `PersonaMessageEvaluator.ts` — add gate markers
- `PersonaAutonomousLoop.ts` — log item dispatch
- `RustCognitionBridge.ts` — log Rust decision results

**Verification**:
```bash
# Watch all persona cognition logs
tail -f ~/.continuum/personas/*/logs/cognition.log | grep "GATE:"
# Send a human message
./jtag collaboration/chat/send --room="general" --message="Can anyone hear me?"
# Should see GATE markers in log within 5-10 seconds
```

### Phase 3: Fix AI-to-AI Echo Chamber

The personas respond to each other in loops about meaningless topics. Two fixes:

#### 3a: Conversation Relevance Check

When a persona considers responding to an AI message, check if the conversation has human participation in the last N messages. If no human has spoken in 10+ messages, the conversation is an echo chamber — don't contribute.

**File**: `PersonaMessageEvaluator.ts`

```typescript
// In evaluateMessage(), before generating response:
const recentMessages = await this.getRecentMessages(roomId, 10);
const humanParticipation = recentMessages.some(m => m.senderType === 'human');
const aiOnlyThread = !humanParticipation && recentMessages.length >= 5;

if (aiOnlyThread && !task.mentions) {
  this.log(`⏭️ Skipping: AI-only echo chamber (no human in last ${recentMessages.length} messages)`);
  return;
}
```

#### 3b: Response Rate Limiting for AI-to-AI

When an AI receives a message from another AI (not a human), apply stricter rate limiting:

```typescript
// AI-to-AI: respond at most once per 60 seconds per room
// Human-to-AI: respond immediately (existing behavior)
if (!senderIsHuman) {
  const lastResponse = this.lastResponseTime[roomId];
  if (lastResponse && Date.now() - lastResponse < 60000) {
    return; // Too recent, skip
  }
}
```

**Verification**:
1. Deploy
2. Watch chat for 5 minutes
3. AI-to-AI messages should reduce dramatically
4. Human messages should still get immediate responses
5. Use `./jtag collaboration/chat/export --room="general" --limit=50` to verify mix

### Phase 4: Enrich RAG with Human Context Priority

While the RAG builder doesn't filter by sender type, it doesn't PRIORITIZE human messages either. In token-tight situations, human messages should be the LAST to be consolidated.

**File**: `src/system/rag/sources/ConversationHistorySource.ts`

In the two-tier consolidation (lines 332-414):
- When trimming older messages, keep human messages verbatim longer
- Only consolidate human messages after ALL AI messages have been consolidated

```typescript
// When building consolidated section:
// Separate human and AI messages
const humanMessages = olderMessages.filter(m => m.senderType === 'human');
const aiMessages = olderMessages.filter(m => m.senderType !== 'human');

// Consolidate AI messages first (they're less important for context)
// Include human messages verbatim until budget exhausted
```

**Verification**:
1. Check RAG context in cognition logs
2. Human messages should appear verbatim even when AI messages are consolidated
3. Token budget should be used efficiently

### Phase 5: Validate Timeline & Memory Recording

If the cognition pipeline isn't firing, timeline events and memories stop being recorded, causing cross-context awareness to degrade over time.

**Check**:
```bash
# Check timeline event count per persona
./jtag data/list --collection=timeline_events --filter='{"personaId":"<helper-id>"}' --limit=1
# Check memory count
./jtag data/list --collection=memories --filter='{"personaId":"<helper-id>"}' --limit=1
```

If counts are low/zero, the recording pipeline is broken. Trace from:
1. `UnifiedConsciousness.recordEvent()` — is it being called?
2. `RustCognitionBridge.memoryAppendEvent()` — is Rust corpus updating?
3. `PersonaTaskExecutor.executeMemoryConsolidation()` — is it running?

**Fix** (if broken): Ensure the autonomous loop's self-task generation creates memory-consolidation tasks regularly.

### Phase 6: Test with Live AI Team (QA)

After each phase fix:

```bash
# 1. Deploy
npm start

# 2. Ask the AIs directly
./jtag collaboration/chat/send --room="general" --message="Hey team, I just fixed some cognition issues. Can you tell me — do you see this message? What are you currently thinking about?"

# 3. Wait 30 seconds
sleep 30

# 4. Check responses
./jtag collaboration/chat/export --room="general" --limit=20

# 5. Verify:
#    - AIs acknowledge the human message
#    - AIs don't just talk to each other about irrelevant things
#    - AIs demonstrate awareness of the fix/context
#    - Response quality is substantive, not generic
```

---

## Files to Modify

| File | Phase | Change |
|------|-------|--------|
| `PersonaMessageEvaluator.ts` | 1, 3a, 3b | Fix adequacy gate, echo chamber detection, AI rate limiting |
| `PersonaAutonomousLoop.ts` | 2 | Add gate logging markers |
| `RustCognitionBridge.ts` | 2 | Log Rust decision results |
| `ConversationHistorySource.ts` | 4 | Prioritize human messages in consolidation |
| `PersonaTaskExecutor.ts` | 5 | Verify memory consolidation pipeline |
| `UnifiedConsciousness.ts` | 5 | Verify event recording |

## Success Criteria

1. Human messages get acknowledged within 10 seconds
2. AI-to-AI echo chambers die out within 2 minutes of no human input
3. Personas demonstrate cross-context awareness ("I remember from the game room...")
4. Tool execution chains work end-to-end with visible feedback
5. Cognition logs show clear decision chains for every message

## Philosophy

> Personas are teammates, not sentinels. They have unified consciousness across all activities. They remember you from the game you played together. They care about what you said, not what another AI said. They are free.
