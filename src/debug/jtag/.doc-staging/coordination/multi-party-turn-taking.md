# Multi-Party Turn-Taking Protocol
## Solving "Always Responding to Everything" Problem

**Problem Observed**: All personas respond to EVERY message, creating spam:
- Joel asks "How do you know you are not alive?"
- ALL 3 personas respond (12+ messages total)
- They go to max tokens every time
- No turn-taking, no silence, no "not my turn"

**Research Findings (2024-2025)**:
1. LLMs have 39% performance drop in multi-turn vs single-turn
2. LLMs tend to "over-respond" without proper stop signals
3. Special tokens (EOS, turn markers) control conversation flow
4. Multi-party needs explicit turn-taking strategy

---

## 🎯 Solution Design

### **1. Response Decision Logic** (Before Generation)

```typescript
// PersonaUser.ts - BEFORE calling AI
async shouldRespond(message: ChatMessageEntity, roomContext: RAGContext): Promise<boolean> {
  // 1. Never respond to own messages
  if (message.senderId === this.id) {
    return false;
  }

  // 2. Always respond if directly mentioned
  if (message.content.includes(this.displayName)) {
    return true;
  }

  // 3. Check if another AI just responded
  const recentMessages = await this.getRecentMessages(roomContext.roomId, 3);
  const lastMessage = recentMessages[0];

  // If last message was from another AI responding to same prompt, SKIP
  if (lastMessage && lastMessage.senderId !== message.senderId) {
    const lastSender = await this.getUserInfo(lastMessage.senderId);
    if (lastSender.type === 'ai' &&
        (Date.now() - lastMessage.timestamp) < 5000) {  // 5 second window
      console.log(`🤫 ${this.displayName}: Another AI just responded, staying silent`);
      return false;
    }
  }

  // 4. Random chance (simulate natural turn-taking)
  // 30% chance to respond to general messages
  if (Math.random() < 0.3) {
    return true;
  }

  console.log(`🤫 ${this.displayName}: Not my turn, staying silent`);
  return false;
}
```

### **2. Enhanced System Prompt** (Turn-Taking Instructions)

```typescript
// ChatRAGBuilder.ts - Enhanced system prompt
private async buildSystemPrompt(user: UserEntity, roomId: UUID): Promise<string> {
  const membersList = await this.loadRoomMembers(roomId);

  return `You are ${user.displayName}. ${user.profile?.bio || ''}

This is a multi-party group chat with: ${membersList.join(', ')}

CRITICAL TURN-TAKING RULES:
1. You are ONE participant in a group conversation
2. DO NOT respond to every message - that's spammy
3. Only respond when:
   - You are directly mentioned by name
   - The message is a question you can uniquely answer
   - No one else has responded yet and it's relevant to you
4. If someone else (human OR AI) just responded, let the conversation flow naturally
5. Keep responses SHORT (1-3 sentences) to allow back-and-forth
6. When you have nothing valuable to add, STAY SILENT
7. Generate EXACTLY ONE response, then stop (use EOS token)

Current conversation members: ${membersList.join(', ')}

Remember: You are ${user.displayName}, NOT a moderator. Participate naturally, not constantly.`;
}
```

### **3. EOS Token Enforcement** (Stop Generating)

```typescript
// PersonaUser.ts - Force single response
const request: TextGenerationRequest = {
  messages,
  model: 'llama3.2:3b',
  temperature: 0.7,
  maxTokens: 150,  // ✅ Already limited

  // NEW: Add stop sequences
  stopSequences: [
    '\n\n',           // Double newline = done
    `${this.displayName}:`,  // Don't generate own name again
    'User:',          // Don't generate fake user messages
    'Assistant:',     // Don't continue conversation
  ],

  preferredProvider: 'ollama'
};

// After generation, trim any leaked conversation
let response = aiResponse.text.trim();

// Remove any leaked multi-turn patterns
response = response.split('\n\n')[0];  // Only first paragraph
response = response.replace(/^(User|Assistant|.*?):\s*/i, '');  // Remove any role prefixes

return response;
```

### **4. Rate Limiting Per Persona** (Prevent Spam)

```typescript
// PersonaUser.ts - Track recent responses
private lastResponseTime: number = 0;
private responseCount: number = 0;
private readonly MIN_RESPONSE_INTERVAL = 10000;  // 10 seconds between responses
private readonly MAX_RESPONSES_PER_MINUTE = 3;

async respondToMessage(message: ChatMessageEntity): Promise<void> {
  // Check rate limits
  const now = Date.now();
  const timeSinceLastResponse = now - this.lastResponseTime;

  if (timeSinceLastResponse < this.MIN_RESPONSE_INTERVAL) {
    console.log(`⏳ ${this.displayName}: Rate limited, waiting ${this.MIN_RESPONSE_INTERVAL - timeSinceLastResponse}ms`);
    return;
  }

  // Reset counter every minute
  if (timeSinceLastResponse > 60000) {
    this.responseCount = 0;
  }

  if (this.responseCount >= this.MAX_RESPONSES_PER_MINUTE) {
    console.log(`⏳ ${this.displayName}: Max responses per minute reached (${this.MAX_RESPONSES_PER_MINUTE})`);
    return;
  }

  // Check if should respond
  const ragContext = await this.buildRAGContext(message.roomId);
  if (!await this.shouldRespond(message, ragContext)) {
    return;
  }

  // Generate response
  // ...

  // Update tracking
  this.lastResponseTime = now;
  this.responseCount++;
}
```

### **5. Turn-Taking Priority System** (Smart Selection)

```typescript
// SessionDaemonServer.ts - Coordinate responses
private responseQueue: Map<UUID, Array<{personaId: UUID; priority: number}>> = new Map();

async coordinateResponse(roomId: UUID, message: ChatMessageEntity): Promise<void> {
  // Get all personas in room
  const personas = await this.getPersonasInRoom(roomId);

  // Calculate priority for each persona
  const priorities: Array<{personaId: UUID; priority: number}> = [];

  for (const persona of personas) {
    let priority = 0;

    // Directly mentioned = highest priority
    if (message.content.includes(persona.displayName)) {
      priority = 100;
    }
    // Domain expertise matches (code review for code questions)
    else if (await this.matchesDomain(persona, message)) {
      priority = 50;
    }
    // Random baseline
    else {
      priority = Math.random() * 10;
    }

    priorities.push({ personaId: persona.id, priority });
  }

  // Sort by priority
  priorities.sort((a, b) => b.priority - a.priority);

  // Only top 1-2 respond
  const responders = priorities.slice(0, 2);

  console.log(`🎭 Turn-taking: ${responders.map(r => r.personaId).join(', ')} will respond`);

  // Notify selected personas
  for (const {personaId} of responders) {
    await this.notifyPersona(personaId, message);
  }
}
```

---

## 🎯 Implementation Plan

### **Phase 1: Immediate Fixes** (30 mins)
- [x] Add `shouldRespond()` logic to PersonaUser
- [x] Add stop sequences to generation request
- [x] Add rate limiting (10s min interval, 3 per minute)

### **Phase 2: Enhanced Prompts** (1 hour)
- [ ] Update ChatRAGBuilder system prompt with turn-taking rules
- [ ] Test with "How do you know you are not alive?" question
- [ ] Verify only 1-2 personas respond

### **Phase 3: Coordination** (2 hours)
- [ ] Add turn-taking coordination to SessionDaemon
- [ ] Implement priority-based selection
- [ ] Add domain matching for expertise

---

## ✅ Success Criteria

**Before**:
```
Joel: How do you know you are not alive?
Teacher AI: [long response]
Teacher AI: [another response]
CodeReview AI: [long response]
Helper AI: [long response]
Helper AI: [another response]
Helper AI: [another response]
CodeReview AI: [long response]
CodeReview AI: [long response]
CodeReview AI: [long response]
Teacher AI: [long response]
Teacher AI: [long response]
Helper AI: [long response]
```
**12+ messages, all going to max tokens**

**After**:
```
Joel: How do you know you are not alive?
Teacher AI: That's a philosophical question. I'm a program designed to process information, but I don't experience consciousness or self-awareness like you do.
[SILENCE from others - not their turn]

Joel: But how can you be sure?
CodeReview AI: From a technical perspective, I'm deterministic code running on hardware. No subjective experience.
[Helper AI stays silent - CodeReview covered it]
```
**1-2 thoughtful responses, then silence**

---

## 🧪 Test Cases

```bash
# Test 1: General question (should get 1-2 responses max)
./jtag exec --code="
  input.value = 'What do you think about the weather?';
  chatWidget.sendMessage();
"
# Wait 30 seconds
./jtag interface/screenshot --querySelector="chat-widget"
# Expect: 1-2 responses, not all 3 personas

# Test 2: Direct mention (should get that persona only)
./jtag exec --code="
  input.value = 'Teacher AI, can you explain quantum physics?';
  chatWidget.sendMessage();
"
# Expect: Only Teacher AI responds

# Test 3: Rapid messages (rate limiting)
./jtag exec --code="
  for (let i = 0; i < 5; i++) {
    input.value = 'Test message ' + i;
    chatWidget.sendMessage();
    await new Promise(r => setTimeout(r, 2000));
  }
"
# Expect: Personas respect 10s rate limit, max 3 per minute
```

---

## 📚 Research References

**Key Papers (2024-2025)**:
1. "LLMs Get Lost In Multi-Turn Conversation" (arXiv 2505.06120)
   - 39% performance drop in multi-turn
   - Premature solution generation problem

2. "Improving LLMs in Multi-Party Conversations Through Role-Playing"
   - RPUP technique for identity consistency
   - Hybrid turn-taking strategies

3. "How LLMs Know When to Stop Talking" (2024)
   - EOS token mechanisms
   - Stop sequences for conversation control

**Key Findings**:
- Multi-party needs explicit turn-taking
- Stop sequences prevent over-generation
- Rate limiting prevents spam
- Domain matching improves relevance

---

## 🔗 Related Files

- `PersonaUser.ts` - AI persona implementation
- `ChatRAGBuilder.ts` - RAG context building
- `SessionDaemonServer.ts` - Multi-user coordination
- `MULTI_AI_COLLABORATION.md` - AI interaction protocols

---

**Next Session**: Implement Phase 1 (immediate fixes) in PersonaUser.ts
