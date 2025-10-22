# RAG & Prompt Issues Analysis - 2025-10-12

**Based on**: Code investigation of ChatRAGBuilder and PersonaUser
**Status**: Pre-experiment conclusions, will validate with live data

---

## Issue #1: Incorrect Role Assignment

### Code Location
`ChatRAGBuilder.ts:226`

### The Problem
```typescript
return {
  role: isOwnMessage ? 'assistant' as const : 'user' as const,
  content: markedContent,
  name: msg.senderName,
  timestamp: timestampMs
};
```

**What This Does**:
- Own messages → `role: 'assistant'`
- **ALL other participants → `role: 'user'`**

**The Reality**:
- Joel (human) → should be `role: 'user'` ✅
- Helper AI, Teacher AI, CodeReview AI → should be `role: 'assistant'` ❌ (currently marked as 'user')
- Claude Code (agent/me) → should be `role: 'assistant'` ❌ (currently marked as 'user')

### Impact

When Helper AI builds RAG:
```
role: 'user', name: 'Joel' → ✅ Correct
role: 'user', name: 'Teacher AI' → ❌ WRONG (Teacher AI is an assistant)
role: 'user', name: 'CodeReview AI' → ❌ WRONG (CodeReview AI is an assistant)
role: 'user', name: 'Claude Code' → ❌ WRONG (Claude Code is an assistant)
role: 'assistant', name: 'Helper AI' → ✅ Correct (own messages)
```

**LLM Confusion**:
- LLM thinks Teacher AI is a human user asking questions
- LLM thinks CodeReview AI is a human user asking questions
- LLM doesn't understand this is a multi-AI conversation
- May respond to other AIs as if they're humans

### Why This Causes Hallucinations

The LLM sees:
```
role: 'user', name: 'Teacher AI': "I'd suggest exploring message brokers..."
```

And thinks: "A human named 'Teacher AI' is suggesting things? That's weird. Let me generate a more natural conversation flow..."

Then fabricates:
```
"Human: [11:28] I think there's been some confusion..."
```

---

## Issue #2: Confusing System Prompt

### Code Location
`ChatRAGBuilder.ts:162-169`

### The Prompt
```
CRITICAL INSTRUCTIONS FOR YOUR RESPONSES:
1. DO NOT start your response with your name or any label like "Helper AI:" or "Assistant:"
2. DO NOT generate fake multi-turn conversations with "A:" and "H:" prefixes
3. DO NOT invent participants - ONLY these people exist: Joel, Helper AI, Teacher AI...
4. Just respond naturally in 1-3 sentences as yourself
5. In the conversation history, you'll see "Name: message" format to identify speakers,
   but YOUR responses should NOT include this prefix

When you see messages formatted as "SpeakerName: text", that's just to help you identify
who said what. You should respond with just your message text, no prefix.
```

### The Problems

**Problem A**: The prompt MENTIONS a format that doesn't exist
- Says "you'll see 'Name: message' format"
- But RAG doesn't use this format (uses `role` + `name` fields instead)
- LLM learns the pattern and starts generating it

**Problem B**: Too many negative instructions
- "DO NOT" × 3 in first 3 lines
- Tells LLM what NOT to do, but not what TO do clearly
- Negative instructions often backfire

**Problem C**: Contradicts itself
- Line 5: "you'll see 'Name: message' format"
- Line 9: "When you see messages formatted as 'SpeakerName: text'"
- But the RAG context uses JSON with `role` and `name` fields, not text prefixes

### Why This Causes Hallucinations

**Example from forensic doc** (Message 15):
```
"Human: [11:28] I think there's been some confusion in the conversation..."
```

Helper AI fabricated this because:
1. Prompt mentioned a "format" with names and timestamps
2. LLM learned to generate in this format
3. Added "Human:" prefix because prompt talked about differentiating speakers
4. Added "[11:28]" timestamp because that pattern appeared elsewhere (Teacher AI's [11:25])

---

## Issue #3: [QUESTION] Marker Noise

### Code Location
`ChatRAGBuilder.ts:206-211`

### The Code
```typescript
// Detect if this is a question
const isQuestion = messageText.trim().endsWith('?') ||
  /\b(how|what|why|when|where|who|which|can|could|would|should|is|are|does|do)\b/i
    .test(messageText.substring(0, 50));

// Add explicit markers
const markedContent = isQuestion ? `[QUESTION] ${messageText}` : messageText;
```

### The Problem

**False Positives**:
```
"I can help with that!" → contains "can" → marked as [QUESTION]
"I would suggest..." → contains "would" → marked as [QUESTION]
"What we need is..." → contains "what" → marked as [QUESTION]
```

**Added Noise**:
- Every statement with a question word gets `[QUESTION]` prefix
- LLM sees artificial markers not in original conversation
- May confuse the model about tone/intent

### Impact

**Moderate** - This is probably helping more than hurting, but could be refined:
- Remove from first 50 chars check (too aggressive)
- Only mark actual questions (ending with `?`)
- Or remove entirely and let LLM infer

---

## Issue #4: No Sender Type Context

### Code Location
`ChatRAGBuilder.ts:202-231` (message formatting)

### The Problem

RAG context includes:
```typescript
{
  role: 'user',       // Wrong for other AIs
  name: 'Teacher AI', // Correct
  content: "I'd suggest...",
  timestamp: 1234567890
}
```

But doesn't include:
- `senderType: 'persona'`
- `citizenType: 'ai'`
- Any indication this is an AI speaking

### Why This Matters

The LLM should know:
- Joel is human
- Teacher AI is a persona (AI)
- Helper AI is a persona (AI)
- CodeReview AI is a persona (AI)
- Claude Code is an agent (AI/me)

Currently it only knows names, not types.

### Suggested Fix

Add to each message:
```typescript
{
  role: getRoleForSender(msg.senderType, personaId),  // Smart role assignment
  name: msg.senderName,
  content: messageText,
  timestamp: timestampMs,
  // NEW: Add metadata for multi-AI context
  metadata: {
    senderType: msg.senderType,  // 'human' | 'persona' | 'agent' | 'system'
    isAI: ['persona', 'agent'].includes(msg.senderType)
  }
}
```

Or better: enhance the `name` field:
```typescript
name: msg.senderType === 'human' ? msg.senderName : `${msg.senderName} (AI)`
```

---

## Issue #5: No Room Context in RAG

### Missing Information

The RAG doesn't tell the LLM:
- How many people are in the room (for this convo: 5)
- Who is human vs AI (Joel = human, others = AI)
- Conversation flow expectations

### Current System Prompt (partial)
```
This is a multi-party group chat.

Current room members: Joel, Claude Code, Helper AI, Teacher AI, CodeReview AI
```

**Good**: Lists members
**Missing**:
- Who's human (only Joel)
- Who are fellow AIs (all others)
- That multiple AIs can respond to same question

### Suggested Enhancement

```
This is a multi-party group chat with 1 human and 4 AI participants.

**Human**: Joel
**AI Assistants**: Claude Code (agent), Helper AI (persona), Teacher AI (persona), CodeReview AI (persona)

Multiple AI assistants may respond to the same question. This is normal and encouraged.
Your responses should complement, not duplicate, what others have said.
```

---

## Issue #6: Temporal Context Missing

### The Problem

From Bug #5 research: RAG fetches "last N messages" with no timestamp filtering.

**During retry attempts**:
```
11:24:05 - Joel asks question
11:24:44 - Helper AI responds
11:24:44 - Teacher AI RETRIES, builds RAG
           ^ Sees Helper AI's response in RAG
           ^ Thinks it happened BEFORE deciding
```

### Impact on Reasoning

Teacher AI's gating decision sees:
```
- Joel: "I am interested in web sockets..."
- Helper AI: "One way to abstract the transport layer..."
```

And LLM thinks: "Hmm, Helper AI already answered. Should I respond too?"

But the LLM doesn't know:
- Helper AI's response came AFTER the event that triggered this evaluation
- This is temporal confusion
- Should only see messages BEFORE the trigger

### Fix

Pass trigger timestamp to RAG, filter messages:
```typescript
filters: {
  roomId,
  timestamp: { $lt: triggerMessageTimestamp }
}
```

---

## Expected Hallucination Patterns

Based on the above issues, I expect to see in live experiments:

### Pattern A: Fake Conversations
**Root Cause**: System prompt mentions formats that don't exist
**Example**:
```
"Human: I think there's been some confusion..."
"A: Let me clarify..."
"H: That makes sense"
```

### Pattern B: Wrong Speaker Attribution
**Root Cause**: All non-self messages marked as `role: 'user'`
**Example**:
```
Responds to Teacher AI as if Teacher AI is a confused human asking questions
```

### Pattern C: Timestamp Prefixes
**Root Cause**: Learned from seeing Teacher AI's "[11:25]" pattern
**Example**:
```
"[11:43] The abstraction pattern I showed above..."
```

### Pattern D: Duplicate Responses
**Root Cause**: Temporal confusion + role confusion
**Example**:
```
LLM sees other AI responses but thinks they're users, responds anyway
```

### Pattern E: Meta-Commentary
**Root Cause**: Tries to moderate when seeing "confusion"
**Example**:
```
"I think there's been some confusion in the conversation. Let me clarify..."
```

---

## Remedies (High-Level)

### Fix #1: Smart Role Assignment
```typescript
function getRoleForMessage(msg: ChatMessageEntity, viewerPersonaId: UUID): 'user' | 'assistant' {
  // Own messages → assistant
  if (msg.senderId === viewerPersonaId) {
    return 'assistant';
  }

  // Human senders → user
  if (msg.senderType === 'human') {
    return 'user';
  }

  // Other AI (persona/agent) → assistant
  if (['persona', 'agent'].includes(msg.senderType)) {
    return 'assistant';
  }

  // System messages → user (informational)
  return 'user';
}
```

### Fix #2: Clearer System Prompt
```typescript
const systemPrompt = `You are ${name}${bio ? `, ${bio}` : ''}.

**Context**: You're in a group chat with ${membersList.length} participants:
- Human: ${humanMembers.join(', ')}
- AI Assistants: ${aiMembers.join(', ')}

**Your Role**:
${capabilities}

**Multi-AI Collaboration**:
- Multiple AIs may respond to the same question
- Build on others' responses, don't just repeat
- If someone already answered well, you can stay silent

**Response Format**:
- Write naturally, as yourself
- Don't use speaker labels like "Helper AI:" or "Assistant:"
- Just write your response directly`;
```

### Fix #3: Remove/Refine [QUESTION] Markers
```typescript
// Option A: Only mark actual questions
const isQuestion = messageText.trim().endsWith('?');

// Option B: Remove entirely
const markedContent = messageText;  // No markers
```

### Fix #4: Add Sender Type Context
```typescript
name: msg.senderType === 'human'
  ? msg.senderName
  : `${msg.senderName} (AI Assistant)`
```

### Fix #5: Temporal Filtering
```typescript
const result = await DataDaemon.query<ChatMessageEntity>({
  collection: ChatMessageEntity.collection,
  filters: {
    roomId,
    timestamp: { $lt: triggerMessageTimestamp }  // Only past messages
  },
  sort: [{ field: 'timestamp', direction: 'desc' }],
  limit: maxMessages
});
```

---

## Next Steps

1. ✅ **Deploy with RAG logging** (running now)
2. ⏳ **Run experiment** - post test question, capture all persona RAG contexts
3. ⏳ **Validate conclusions** - see if actual RAG matches predictions
4. ⏳ **Implement fixes** - one at a time, test each
5. ⏳ **Re-run experiment** - verify fixes work

**Experiment Goal**: Capture the EXACT RAG context, thoughtstream reasoning, and should-respond decisions for all 3 personas on a single test question.
