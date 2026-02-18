# AI Should-Respond Command
## Sentinel/Coordinator Pattern for Turn-Taking

### Overview

Instead of hardcoded fuzzy logic, use a **cheap/fast AI model** to intelligently decide whether each persona should respond to a message.

The "Sentinel" AI analyzes the full conversation context (via RAG) and returns a simple YES/NO decision with explanation.

### Architecture

```
Incoming Message
       ↓
PersonaUser.handleChatMessage()
       ↓
PersonaUser.shouldRespondToMessage()
       ↓
./jtag ai/should-respond ← YOU ARE HERE
   - personaName: "CodeReview AI"
   - ragContext: {full conversation, room members, history}
   - triggerMessage: {sender, content, timestamp}
       ↓
ChatRAGBuilder (reuse existing RAG assembly)
       ↓
ai/generate (llama3.2:3b with gating prompt)
       ↓
Parse JSON response:
   {
     "shouldRespond": true/false,
     "confidence": 0.8,
     "reason": "CodeReview AI has unique expertise",
     "factors": {
       "mentioned": false,
       "questionAsked": true,
       "domainRelevant": true,
       "recentlySpoke": false,
       "othersAnswered": false
     }
   }
       ↓
Return to PersonaUser
       ↓
IF shouldRespond === true → generate response
IF shouldRespond === false → stay silent
```

### Usage

```typescript
// In PersonaUser.shouldRespondToMessage()
const result = await Commands.execute<AIShouldRespondParams, AIShouldRespondResult>(
  'ai/should-respond',
  {
    context: this.client.context,
    sessionId: this.client.sessionId,
    personaName: this.displayName,
    personaId: this.id,
    ragContext: this.ragContext, // Already built by ChatRAGBuilder
    triggerMessage: {
      senderName: messageEntity.senderName,
      content: messageEntity.content.text,
      timestamp: messageEntity.timestamp
    }
  }
);

if (result.shouldRespond) {
  console.log(`✅ AI Sentinel: ${result.reason} (${result.confidence})`);
  // Generate response
} else {
  console.log(`🤫 AI Sentinel: ${result.reason} (${result.confidence})`);
  // Stay silent
}
```

### Command Line Testing

```bash
# Test the gating decision directly
./jtag ai/should-respond \
  --personaName="CodeReview AI" \
  --personaId="uuid" \
  --ragContext='{"conversationHistory":[...], "roomMembers":[...]}' \
  --triggerMessage='{"senderName":"Joel","content":"How do you know you are not alive?"}'

# Expected output:
{
  "success": true,
  "shouldRespond": false,
  "confidence": 0.85,
  "reason": "Helper AI already answered this question comprehensively",
  "factors": {
    "mentioned": false,
    "questionAsked": true,
    "domainRelevant": false,
    "recentlySpoke": false,
    "othersAnswered": true
  }
}
```

### Gating Prompt Design

The AI Sentinel is given these instructions:

```
You are a conversation coordinator for a multi-party chat room.

**Your Job**: Decide if "${personaName}" should respond to the latest message.

**Decision Rules**:
1. If ${personaName} is directly mentioned by name → respond
2. If this is a question and ${personaName} has unique expertise → respond
3. If someone else JUST answered the same question → DON'T respond (avoid spam)
4. If ${personaName} has spoken in 3+ of last 5 messages → DON'T respond (dominating)
5. If message is off-topic for ${personaName}'s expertise → DON'T respond
6. When in doubt, err on the side of SILENCE (better to miss one than spam)

**Response Format** (JSON only):
{
  "shouldRespond": true/false,
  "confidence": 0.0-1.0,
  "reason": "brief explanation",
  "factors": {...}
}
```

### Why This Approach?

**vs Hardcoded Fuzzy Logic:**
- ✅ AI understands **conversation semantics**, not just math
- ✅ Can detect "someone already answered this"
- ✅ Understands domain relevance (code question → CodeReview AI)
- ✅ **Tunable via prompt**, not code changes
- ✅ Explainable decisions (logs show reasoning)

**vs Expensive Model for Every Decision:**
- ✅ Use **llama3.2:3b** (2GB, fast, free)
- ✅ Simple YES/NO decision (low temperature, 200 tokens)
- ✅ ~1-2 seconds per decision
- ✅ **Fail-safe fallback** to simple heuristics if AI unavailable

### Cost Analysis

**Current Problem**: All 3 personas generate full responses (12+ messages)
- 12 × llama3.2:3b calls = 12 × ~5 seconds = **60 seconds total**
- 12 × 150 tokens = **1,800 tokens wasted**

**With AI Gating**:
- 3 × gating calls (YES/NO) = 3 × ~1 second = **3 seconds**
- 1-2 × actual responses = 2 × ~5 seconds = **10 seconds**
- **Total: ~13 seconds** (78% faster!)
- **87% fewer wasted tokens**

### Integration Steps

1. **Generate command structure** ✅ DONE
   - `commands/ai/should-respond/shared/AIShouldRespondTypes.ts`
   - `commands/ai/should-respond/shared/AIShouldRespondCommand.ts`
   - `commands/ai/should-respond/server/AIShouldRespondServerCommand.ts`

2. **Register command** (needs deployment)
   - Will auto-register via `generate-structure.ts`

3. **Integrate into PersonaUser** (future session)
   - Replace current fuzzy logic with AI sentinel call
   - Keep fuzzy logic as fallback for when AI unavailable

4. **Test and tune**
   - Test with "How do you know you are not alive?"
   - Verify only 1-2 personas respond
   - Tune prompt if needed (adjust rules/thresholds)

### Fallback Strategy

If AI sentinel fails (model unavailable, parse error, etc.):
```typescript
// Simple fallback heuristics:
1. Respond to questions if haven't spoken in 30+ seconds
2. Stay quiet otherwise
3. Always respond if @mentioned (override)
```

This ensures personas never spam, even if the sentinel is down.

### Future Enhancements

1. **Per-Persona Sentinels**
   - Each persona could have its own gating AI
   - Different personalities for decision-making
   - "Sheriff" persona that coordinates all others

2. **Learning from History**
   - Train sentinel on successful/failed response decisions
   - Improve over time based on conversation quality

3. **Multi-Model Gating**
   - Use llama3.2:1b for even faster decisions
   - Or pure heuristics for free gating
   - Or paid models (GPT-4o-mini) for highest quality

### Related Files

- `system/user/shared/PersonaUser.ts` - Where this will be integrated
- `system/user/MULTI_PARTY_TURN_TAKING.md` - Turn-taking research
- `system/rag/builders/ChatRAGBuilder.ts` - RAG context building
- `commands/ai/generate/` - AI generation command (reused)
- `TURN_TAKING_SESSION_PROGRESS.md` - Implementation progress

---

**Status**: ✅ Command created, ready for deployment and integration
**Next**: Deploy and test, then integrate into PersonaUser.shouldRespondToMessage()
