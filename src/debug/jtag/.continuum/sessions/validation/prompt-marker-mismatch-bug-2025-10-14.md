# Prompt-Marker Mismatch Bug - 2025-10-14

## Critical Failure: All AIs Stopped Responding

**Symptoms**: After deploying Q&A annotation fix, ALL three AIs (Helper, CodeReview, Teacher) stopped responding to ANY questions, staying SILENT with hallucinated reasons like "already discussed" when no answer existed.

## Root Cause

**Prompt-Message Mismatch**: Updated gating prompt to reference `[Q]` and `[A→Q]` markers, but those markers were NOT present in the conversation history shown to the LLM.

**What the LLM saw**:
```
Prompt: "When checking if a question was answered, ONLY look at [A→Q] messages immediately after the [Q]."

Conversation History:
   Joel: "What is functional programming?"
   (no [Q] or [A→Q] markers anywhere)
```

**Result**: LLM confusion → Hallucination that answer exists → All AIs stay SILENT

## Why Markers Weren't Present

Two possible explanations:
1. **Annotation code didn't run**: annotateQAPairs() method exists but may not be executing
2. **Logs show pre-annotation messages**: AIDecisionService logs original ragContext for diagnostics, not the annotated version sent to LLM

## Evidence

### Before Revert (Broken):
```
[2025-10-14T00:48:26.632Z] Helper AI → SILENT
Reason: "already discussed in previous conversation"
Message: "What is functional programming? [debug-test-17797]"

📊 RAG Context: 12/31 messages
💬 Conversation History (12 messages):
   1. Joel: "Integration test message"
   2. Joel: "What are the key differences between REST and GraphQL APIs?"
   3. Helper AI: "Here's a detailed comparison of REST..."
   4. Joel: "What is the purpose of async/await in JavaScript?"
   5. Helper AI: "**Async/Await in JavaScript**..."
   6. Joel: "What is dependency injection in software design?"
   7. Helper AI: "**Dependency Injection**..."
   8-11. [various other messages]
   12. Joel: "What is functional programming? [debug-test-17797]"
```

**NO answer about functional programming exists, but AI says "already discussed"!**

### After Revert (Fixed):
```
[2025-10-14T00:59:05.161Z] Helper AI → RESPOND
Confidence: 1.00
Reason: "The previous response does not address the question about recursion."
Message: "Can you explain recursion? [recovery-test-29408]"
```

**AI correctly identifies no answer exists and decides to RESPOND.**

## Lesson Learned

**Never reference markers in prompt unless you can VERIFY they exist in the actual messages sent to LLM.**

The disconnect between what the prompt describes and what the LLM actually sees causes:
- Confusion
- Hallucination
- False negatives (thinking answer exists when it doesn't)

## Fix Applied

Reverted `commands/ai/should-respond/shared/AIShouldRespondCommand.ts` to original working prompt without marker references.

**Original Working Prompt**:
```typescript
CRITICAL RULES:
1. If someone ALREADY answered the question → shouldRespond: FALSE, stay silent
2. If you would just repeat what was already said → shouldRespond: FALSE, stay silent
3. If the answer is WRONG and needs correction → shouldRespond: TRUE, correct it
4. If nobody helped yet and question needs answer → shouldRespond: TRUE, help them
```

## Annotation Code Status

The annotation code in `AIShouldRespondServerCommand.ts` remains but may not be working:
- Method exists: `annotateQAPairs()`
- Debug logging added but NOT appearing in logs
- Unclear if annotations reach the LLM or if logs just don't show them

**Recommendation**: Remove or fix annotation code since it's unverified and the prompt revert shows it's not needed for basic operation.

## Next Steps

1. ✅ Reverted prompt - AIs responding again
2. Remove unused annotation code OR verify it works with proper logging
3. Address original Q&A contamination issue with different approach (if still needed after Phase 1 fix)

## Status

- **System Status**: ✅ RECOVERED - AIs responding normally
- **Annotation Fix**: ❌ FAILED - Caused complete system breakage
- **Phase 1 Fix (explicit TRUE/FALSE)**: ✅ Still working after recovery
