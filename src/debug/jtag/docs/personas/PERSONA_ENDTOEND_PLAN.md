# Persona End-to-End Plan: From Chat Message to AI Response

## 🎯 Mission
Get PersonaUsers fully functional in chat - end-to-end AI that responds to messages using RAG context.

**Current Status:** Chatting with the void - PersonaUsers exist in database but don't respond.
**Goal:** PersonaUser subscribes to chat events, loads history, generates AI response, posts back to chat.

---

## 🏗️ Architecture Overview

```
Human sends message in chat
        ↓
ChatWidget → data/create command
        ↓
DataDaemon stores ChatMessageEntity
        ↓
DataDaemon emits: data:ChatMessage:created event
        ↓
PersonaUser (in UserDaemon) hears event
        ↓
PersonaUser.handleChatMessage()
        ├─ Check if in room (Room.participants)
        ├─ Load chat history (last 50 messages as RAG context)
        └─ Should respond? (not always!)
        ↓
AIUser.generateResponse()
        ├─ Prepare context from chat history
        ├─ Add persona configuration
        └─ Call AI provider
        ↓
AIProviderDaemon.generateText()
        ├─ Select provider (Claude/GPT)
        ├─ Format request with RAG context
        ├─ Call AI API
        └─ Return generated text
        ↓
PersonaUser.postChatMessage()
        ↓
DataDaemon stores response ChatMessageEntity
        ↓
DataDaemon emits: data:ChatMessage:created event
        ↓
ChatWidget receives event → displays response ✅
```

---

## 🚨 Architecture Rules Compliance

**CRITICAL: All AI logic goes in AIUser and AIProviderDaemon:**

✅ **AIUser base class** - Shared AI functionality (RAG context, response generation)
✅ **PersonaUser extends AIUser** - Persona-specific behavior (auto-respond, event subscriptions)
✅ **AgentUser extends AIUser** - Agent-specific behavior (external connections)
✅ **AIProviderDaemon** - Generic AI API abstraction (OpenAI, Anthropic, local models)

**NOT in PersonaUser:**
❌ Direct AI API calls (goes in AIProviderDaemon)
❌ Provider-specific logic (goes in AIProviderDaemon adapters)
❌ Generic AI response logic (goes in AIUser base class)

**PersonaUser ONLY handles:**
- Event subscriptions (data:ChatMessage:created)
- Room participation checks
- Decision to respond (not always!)
- Inherits response generation from AIUser

---

## 📋 Implementation Phases

### **Phase 1: User Creation Foundation** ✅ DESIGNED
**Status:** Design documents complete, ready to implement

**What:**
- `user/create` command with factory pattern
- BaseUser.create() routes to PersonaUser/AgentUser/HumanUser
- Each user type implements creation recipe
- Entities stored to database with type-specific defaults

**Files:**
- `commands/user/create/` - User creation command
- `system/user/shared/BaseUser.ts` - Add static create() factory
- `system/user/shared/PersonaUser.ts` - Add static create() recipe
- `system/user/shared/AgentUser.ts` - Add static create() recipe
- `system/user/shared/HumanUser.ts` - Add static create() recipe

**Test:** `tests/integration/user-creation-crud.test.ts`
- Verify UserEntity + UserStateEntity in database
- Verify events emitted
- Verify user-list-widget synchronization

---

### **Phase 2: UserDaemon Lifecycle Management** ⚠️ PARTIAL
**Status:** Skeleton exists with TypeScript errors, needs fixing

**What:**
- UserDaemonServer listens for `data:User:created` events
- Creates persistent PersonaUser instances for type=persona
- Maintains `users: Map<UUID, BaseUser>` collection
- PersonaUser instances live in UserDaemon (not SessionDaemon)

**Files:**
- `daemons/user-daemon/server/UserDaemonServer.ts` - Fix compilation errors
- `daemons/user-daemon/shared/UserDaemon.ts` - Base daemon class
- `server/generated.ts` - Already registered ✅

**Test:** Manual verification
```bash
./jtag user/create --type=persona --displayName="TestBot" --addToRooms=general
# Check logs for "PersonaUser initialized"
./jtag debug/logs --filterPattern="UserDaemon"
```

---

### **Phase 3: AIUser Base Class** 🆕 NEW
**Status:** Exists but minimal, needs AI response logic

**What:**
- **RAG Context Management** - Load and prepare chat history
- **Response Generation Interface** - Abstract method for generating responses
- **Provider Communication** - Interface with AIProviderDaemon
- **Shared AI Logic** - Used by both PersonaUser and AgentUser

**Add to AIUser:**
```typescript
abstract class AIUser extends BaseUser {
  // RAG Context Loading
  protected async loadChatContext(
    roomId: UUID,
    limit: number = 50
  ): Promise<ChatMessageEntity[]>;

  // Format context for AI
  protected formatContextForAI(
    messages: ChatMessageEntity[]
  ): AIContextMessage[];

  // Generate response using AI
  protected async generateAIResponse(
    context: AIContextMessage[],
    currentMessage: ChatMessageEntity
  ): Promise<string>;

  // Decision logic - should respond?
  protected async shouldRespondToMessage(
    message: ChatMessageEntity
  ): Promise<boolean>;
}
```

**Files:**
- `system/user/shared/AIUser.ts` - Add response generation methods
- `system/user/shared/AIUserTypes.ts` - AI-specific type definitions

**Test:** Unit tests for context formatting
```typescript
test('AIUser formats chat history for AI context', async () => {
  const messages = [...]; // Mock messages
  const context = aiUser.formatContextForAI(messages);
  expect(context).toHaveLength(messages.length);
  expect(context[0]).toHaveProperty('role');
  expect(context[0]).toHaveProperty('content');
});
```

---

### **Phase 4: PersonaUser Event Subscriptions** ⚠️ PARTIAL
**Status:** Skeleton exists, needs integration with AIUser

**What:**
- PersonaUser subscribes to `data:ChatMessage:created` events
- Checks room participation via Room.participants
- Decides whether to respond (not always!)
- Inherits response generation from AIUser

**Update PersonaUser:**
```typescript
class PersonaUser extends AIUser {
  async initialize(): Promise<void> {
    // Subscribe to chat messages
    this.eventManager.on('data:ChatMessage:created',
      (message) => this.handleChatMessage(message)
    );
  }

  private async handleChatMessage(message: ChatMessageEntity): Promise<void> {
    // Don't respond to own messages
    if (message.senderId === this.id) return;

    // Check room participation
    if (!await this.isInRoom(message.roomId)) return;

    // Should respond? (base class decision logic)
    if (!await this.shouldRespondToMessage(message)) return;

    // Load context (base class method)
    const context = await this.loadChatContext(message.roomId);

    // Generate response (base class method)
    const response = await this.generateAIResponse(context, message);

    // Post response
    await this.postChatMessage(message.roomId, response);
  }
}
```

**Files:**
- `system/user/shared/PersonaUser.ts` - Use AIUser base methods
- `system/user/shared/PersonaUserTypes.ts` - Persona-specific types

**Test:** Integration test
```bash
# Create persona and add to room
./jtag user/create --type=persona --displayName="TestBot" --addToRooms=general

# Send message to room
./jtag debug/chat-send --roomId=general --content="Hello TestBot"

# Check for response
./jtag debug/logs --filterPattern="PersonaUser.*response"
./jtag data/list --collection=ChatMessage --filter='{"roomId":"general"}' | tail -5
```

---

### **Phase 5: AIProviderDaemon Implementation** 🆕 NEW
**Status:** Design exists, not implemented

**What:**
- Generic AI provider abstraction
- Pluggable adapters for different AI services
- Text generation command with streaming support
- Provider selection strategy

**Minimal Implementation (Claude adapter only):**
```typescript
class AIProviderDaemonServer extends DaemonBase {
  private providers: Map<string, AIProviderAdapter> = new Map();

  async initialize(): Promise<void> {
    // Register Claude provider
    this.providers.set('anthropic', new AnthropicAdapter());
  }

  async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
    const provider = this.providers.get(request.provider || 'anthropic');
    return await provider.generateText(request);
  }
}
```

**Files:**
- `daemons/ai-provider-daemon/shared/AIProviderDaemon.ts` - Base daemon
- `daemons/ai-provider-daemon/server/AIProviderDaemonServer.ts` - Server implementation
- `daemons/ai-provider-daemon/shared/AIProviderTypes.ts` - Type definitions
- `daemons/ai-provider-daemon/providers/anthropic/AnthropicAdapter.ts` - Claude adapter
- `server/generated.ts` - Register daemon

**Test:** Direct API call
```bash
./jtag ai-provider/generate-text \
  --messages='[{"role":"user","content":"Hello"}]' \
  --model="claude-3-5-sonnet-20241022"
```

---

### **Phase 6: AIUser → AIProviderDaemon Integration** 🆕 NEW
**Status:** Not started

**What:**
- AIUser calls AIProviderDaemon for response generation
- Passes chat context formatted for AI
- Handles streaming responses (optional)

**Add to AIUser:**
```typescript
protected async generateAIResponse(
  context: AIContextMessage[],
  currentMessage: ChatMessageEntity
): Promise<string> {
  // Call AI Provider Daemon
  const request: TextGenerationRequest = {
    messages: [
      ...this.formatContextForAI(context),
      { role: 'user', content: currentMessage.content.text }
    ],
    model: this.entity.modelConfig?.model || 'claude-3-5-sonnet-20241022',
    temperature: this.entity.modelConfig?.temperature || 0.7,
    maxTokens: this.entity.modelConfig?.maxTokens || 1000
  };

  // Route to AI Provider Daemon
  const response = await this.context.router.routeMessage({
    endpoint: '/ai-provider/generate-text',
    payload: request
  });

  return response.data.text;
}
```

**Files:**
- `system/user/shared/AIUser.ts` - Add provider integration
- Test with real AI calls

---

### **Phase 7: End-to-End Testing** 🎯 FINAL
**Status:** Not started

**What:**
- Complete flow from human message → persona response
- Verify chat widget displays response
- Test multiple personas in same room
- Test RAG context (responses reference earlier messages)

**Manual Test Sequence:**
```bash
# 1. Start system
npm start

# 2. Create persona
./jtag user/create --type=persona --displayName="TestBot" --addToRooms=general

# 3. Verify persona initialized
./jtag debug/logs --filterPattern="PersonaUser.*initialized"

# 4. Send message
./jtag debug/chat-send --roomId=general --content="Hello TestBot, what is 2+2?"

# 5. Wait for response (2-5 seconds)
sleep 5

# 6. Check for AI response
./jtag data/list --collection=ChatMessage --filter='{"roomId":"general"}' | tail -10

# 7. Verify response in widget
./jtag debug/widget-state --widgetSelector=chat-widget

# 8. Take screenshot
./jtag screenshot --querySelector=chat-widget --filename=persona-response.png
```

**Automated Test:** `tests/integration/persona-chat-endtoend.test.ts`
```typescript
test('PersonaUser responds to chat message with AI', async () => {
  // Create persona
  const personaResult = await createPersona('TestBot', ['general']);

  // Send message
  const messageResult = await sendChatMessage('general', 'Hello TestBot');

  // Wait for response
  await sleep(5000);

  // Verify response exists
  const messages = await getChatMessages('general');
  const responses = messages.filter(m => m.senderId === personaResult.userId);

  expect(responses.length).toBeGreaterThan(0);
  expect(responses[0].content.text).toBeTruthy();
  expect(responses[0].metadata.source).toBe('persona-ai');
});
```

---

## 📊 Success Criteria

### **Phase 1 Success:**
✅ `./jtag user/create --type=persona` creates UserEntity + UserStateEntity
✅ Events `data:User:created` and `data:UserState:created` emitted
✅ user-list-widget displays new persona

### **Phase 2 Success:**
✅ UserDaemonServer starts without errors
✅ Creates PersonaUser instance when persona entity created
✅ Logs show "PersonaUser initialized"

### **Phase 3 Success:**
✅ AIUser base class compiles without errors
✅ Context loading methods work with real chat data
✅ Context formatting produces valid AI messages

### **Phase 4 Success:**
✅ PersonaUser subscribes to chat events
✅ Logs show "PersonaUser received message"
✅ Room participation check works

### **Phase 5 Success:**
✅ AIProviderDaemon starts and registers
✅ Can call `ai-provider/generate-text` directly
✅ Returns AI response from Claude API

### **Phase 6 Success:**
✅ AIUser successfully calls AIProviderDaemon
✅ Response generation works end-to-end
✅ No TypeScript errors

### **Phase 7 Success:**
✅ Human sends message → PersonaUser responds with AI
✅ Response appears in ChatWidget
✅ Response references chat context (RAG working)
✅ Multiple personas can coexist in same room

---

## 🎯 Implementation Order

**CRITICAL: Follow this exact order to avoid integration hell**

1. ✅ **Design Phase** - Complete (USER_CREATION_DESIGN.md, USER_CREATION_TEST_DESIGN.md)
2. **Phase 1** - User creation foundation (2-3 hours)
3. **Phase 2** - UserDaemon fixes (1 hour)
4. **Phase 3** - AIUser base class (2-3 hours)
5. **Phase 5** - AIProviderDaemon minimal (Claude only) (3-4 hours)
6. **Phase 4** - PersonaUser event handling (1-2 hours)
7. **Phase 6** - Integration (1-2 hours)
8. **Phase 7** - End-to-end testing (2-3 hours)

**Total Estimated Time:** 12-18 hours for fully functional AI personas in chat

---

## 🔍 Key Design Decisions

### **1. AI Logic Separation**
**Decision:** All generic AI logic in AIUser, provider communication in AIProviderDaemon
**Rationale:** Follows ARCHITECTURE-RULES.md abstraction principles, enables AgentUser to reuse same logic

### **2. RAG Context in Base Class**
**Decision:** AIUser handles chat history loading and context formatting
**Rationale:** Both PersonaUser and AgentUser need RAG context, DRY principle

### **3. Response Decision Logic**
**Decision:** AIUser.shouldRespondToMessage() - not always respond
**Rationale:** Personas shouldn't spam, need intelligent response triggers (mentioned by name, question marks, etc.)

### **4. Provider Abstraction**
**Decision:** AIProviderDaemon with pluggable adapters
**Rationale:** Can switch between Claude/GPT/local models without changing PersonaUser code

### **5. Event-Driven Architecture**
**Decision:** PersonaUser subscribes to data:ChatMessage:created, doesn't poll
**Rationale:** Follows system event architecture, real-time responses

---

## 🚨 Critical Gotchas

### **1. Don't Put AI Logic in PersonaUser**
❌ **Wrong:** PersonaUser calls Anthropic API directly
✅ **Right:** PersonaUser → AIUser.generateAIResponse() → AIProviderDaemon → Anthropic

### **2. Don't Always Respond**
❌ **Wrong:** Respond to every message in room
✅ **Right:** Intelligent decision (mentioned by name, direct question, etc.)

### **3. Don't Store Instances in SessionDaemon**
❌ **Wrong:** SessionDaemon creates PersonaUser instances
✅ **Right:** UserDaemon creates and maintains PersonaUser instances

### **4. Don't Mix Entity Types in Data Layer**
❌ **Wrong:** DataDaemon knows about ChatMessageEntity specifically
✅ **Right:** DataDaemon works generically with BaseEntity

### **5. Don't Skip Context Loading**
❌ **Wrong:** Generate response without chat history
✅ **Right:** Load last 50 messages as RAG context for AI

---

## 📚 References

**Design Documents:**
- `USER_CREATION_DESIGN.md` - Factory pattern for user creation
- `USER_CREATION_TEST_DESIGN.md` - CRUD testing strategy
- `ARCHITECTURE-RULES.md` - System architecture rules
- `daemons/ai-provider-daemon/README.md` - AI provider architecture

**Existing Code:**
- `system/user/shared/BaseUser.ts` - User hierarchy base
- `system/user/shared/AIUser.ts` - AI user base class
- `system/user/shared/PersonaUser.ts` - Persona implementation (partial)
- `daemons/user-daemon/server/UserDaemonServer.ts` - Lifecycle management (partial)
- `tests/integration/crud-db-widget.test.ts` - Testing pattern reference

---

**REMEMBER: This is the path from "chatting with the void" to "functional AI personas that respond to messages."**

**Next Step:** Begin Phase 1 implementation - user/create command with factory pattern.
