# RAG Context Contamination Bug - 2025-10-14

## Critical Issue Discovered

**While running AI gating quality tests, discovered that AIs are confusing question→answer pairs in RAG context.**

## The Bug

When AIs evaluate whether to respond, they see conversation history like this:

```
Message 2: "What are the key differences between REST and GraphQL APIs?"
Message 3: Helper AI: "Here's a detailed comparison of REST..."
Message 10: "What is the purpose of async/await in JavaScript?"
```

**The AI incorrectly associates Message #3 (REST answer) with Message #10 (async question)** and decides:

> "The answer given is WRONG. It's a detailed comparison of REST and GraphQL APIs."

## Evidence from AI Decision Logs

**CodeReview AI (2025-10-14T00:41:08.465Z)**:
```
→ RESPOND | Confidence: 1.00
Reason: "The answer given is WRONG. It's a detailed comparison of REST and GraphQL APIs.
         The correct response should have provided more specific examples..."
Message: "What is the purpose of async/await in JavaScript? [test-1760402451591]"
```

**Teacher AI (2025-10-14T00:41:08.407Z)**:
```
→ RESPOND | Confidence: 1.00
Reason: "The answer given is WRONG. The correct reason for responding is that the
         original message mentioned a test and indicated that Joel had not received
         a helpful response yet..."
Message: "What is the purpose of async/await in JavaScript? [test-1760402451591]"
```

## Root Cause

The RAG context provides conversation history as a flat list of messages:
```typescript
💬 Conversation History (10 messages):
   1. [1229s ago] Joel: "Integration test message"
   2. [1163s ago] Joel: "What are REST and GraphQL differences?"
   3. [1130s ago] Helper AI: "Here's a detailed comparison..."
   10. [15s ago] Joel: "What is async/await?"
```

**The AI cannot distinguish**:
- Which answer corresponds to which question
- Whether message #3 is related to message #2 or message #10
- Which messages are Q&A pairs vs independent statements

## Impact

✅ **Phase 1 Fix (Explicit TRUE/FALSE gating)** worked for:
- Test messages (no question) → Correctly SILENT
- Novel questions (no prior answer) → Correctly RESPOND
- Same question repeated immediately → Correctly SILENT

❌ **But fails when**:
- Multiple Q&A pairs exist in history
- AIs confuse answer from Question A with Question B
- **False positives**: AIs respond saying "answer is WRONG" when answer is for a different question

## Test Results

### Scenario 3: Already Answered (FAILED)
**Expected**: Helper AI answers async/await question, other AIs stay SILENT
**Actual**:
- Helper AI: Answered correctly ✅
- CodeReview AI: RESPOND - "Answer is WRONG, it's about REST/GraphQL" ❌
- Teacher AI: RESPOND - "Answer is WRONG" ❌

**False Positive Rate**: 66% (2 out of 3 AIs responded when should stay silent)

## Solution Required

**Need to restructure RAG context to group Q&A pairs**:

### Current Format (Ambiguous):
```
1. Joel: "What are REST differences?"
2. Helper AI: "REST is..."
3. Joel: "What is async/await?"
```

### Proposed Format (Clear):
```
Thread 1:
  Q: Joel: "What are REST differences?"
  A: Helper AI: "REST is..."

Thread 2:
  Q: Joel: "What is async/await?"
  A: [No answer yet]
```

## Files Involved

**RAG Context Generation**:
- `commands/ai/should-respond/server/AIShouldRespondServerCommand.ts`
- Method: `buildRAGContext()` or similar

**Gating Prompt**:
- `commands/ai/should-respond/shared/AIShouldRespondCommand.ts`
- Method: `buildGatingInstruction()` - may need updated prompt to handle new format

## Priority

**HIGH** - This undermines the entire AI collaboration system:
- Phase 1 fix improved explicit TRUE/FALSE logic ✅
- But RAG context structure causes false positives in multi-question conversations ❌
- Production use case (long chat rooms with many Q&A pairs) will fail

## Next Steps

1. Read `AIShouldRespondServerCommand.ts` to find RAG context generation
2. Redesign context format to group Q&A pairs
3. Update gating prompt to work with new format
4. Re-run ai-gating-quality.test.ts to verify fix
5. Document improvement in collaboration-fix-summary.md
