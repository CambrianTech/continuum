# ChatDaemon - Universal Communication Substrate

## **🎯 Mission**
Universal communication substrate enabling any intelligence (human, AI, persona, LoRA model) to participate in chat rooms through the same interface with room-scoped event distribution and storage-backed history.

## **🏗️ Architecture Pattern**
Follows the **Sparse Override Pattern** with 85% shared logic:

```
daemons/chat-daemon/
├── shared/
│   ├── ChatDaemon.ts            # Universal interface (85% of logic)
│   ├── ChatDaemonBase.ts        # Abstract base implementation
│   └── ChatTypes.ts             # Shared types and contracts
├── browser/
│   └── ChatDaemonBrowser.ts     # UI event integration (5%)
├── server/
│   └── ChatDaemonServer.ts      # Event distribution and storage (10%)
└── README.md                    # This documentation
```

## **🎯 ARCHITECTURAL VISION**

**Universal Communication Bus**: Any intelligence (human, AI, persona, LoRA model) can participate in chat rooms through the same interface. No special handling for participant types.

**Room-Scoped Events**: Participants only receive events for rooms they've joined. No global broadcasting spam.

**Storage-Backed History**: All participants in a room share the same chat history via persistent storage.

**Transport-Agnostic**: Events and commands flow through existing JTAG router/transport system.

---

## 🏗️ **CORE ARCHITECTURE COMPONENTS**

### **1. Universal Participant System**
```typescript
SessionParticipant {
  participantId: UUID        // Universal ID
  sessionId: UUID           // JTAG session connection
  displayName: string       // How they appear in chat
  capabilities: {           // What they can do (behavior-based)
    autoResponds: boolean   // AI agents, personas, bots
    canModerate: boolean    // Room management rights
    canInvite: boolean      // Can invite others
  }
  adapter: {                // HOW they connect/respond
    type: string            // 'ai-api' | 'browser-ui' | 'webhook' | 'lora-persona'
    config: object          // Adapter-specific settings
    responseStrategy: {     // When/how they auto-respond
      triggers: []          // mention, keyword, question, etc.
    }
  }
}
```

**Key Insight**: We eliminate `citizenType` completely. Instead of checking "if agent", we check "if autoResponds capability".

### **2. Room-Scoped Event Distribution**

```typescript
RoomEventSystem {
  // Subscription management
  subscribeToRoom(sessionId, roomId)     // Join room events
  unsubscribeFromRoom(sessionId, roomId) // Leave room events
  
  // Event distribution (room-scoped only)
  distributeMessageEvent(roomId, message)        // New message
  distributeParticipantJoined(roomId, participant) // Someone joined
  distributeResponseEvent(roomId, aiResponse)     // AI/bot responded
  
  // Via existing router/transport
  distributeRoomEvent(roomId, eventType, data)   // Universal delivery
}
```

**Key Insight**: Events go to room participants ONLY, not globally. Router handles delivery to correct sessions.

### **3. Universal Response Engine**

```typescript
UniversalResponseEngine {
  // Works for ANY auto-responder (AI, bot, persona)
  shouldRespond(participant, message, room) → decision
  generateResponse(participant, message, context) → content
  
  // Adapter system handles provider differences
  callAdapter(adapter, request) → response
}
```

**Key Insight**: One response system for OpenAI, Anthropic, local models, webhooks, personas, etc.

### **4. Storage Integration**

```typescript
StorageAdapter {
  // Shared chat history
  storeMessage(roomId, message)
  getRoomHistory(roomId, limit) → messages[]
  
  // Participant persistence  
  storeParticipant(participant)
  getParticipants(sessionIds) → participants[]
  
  // Room management
  createRoom(room)
  getRoom(roomId) → room
}
```

**Key Insight**: Storage is abstracted - can use database, files, or memory. All participants see same history.

---

## 🔄 **EVENT FLOW ARCHITECTURE**

### **Message Flow Example:**
```
1. Human types message in chat widget
2. Widget sends chat/send-message command via JTAG transport
3. ChatDaemonServer processes command:
   - Stores message via StorageAdapter
   - Distributes to room participants via RoomEventSystem
4. RoomEventSystem sends events via router to subscribed sessions:
   - Other humans get chat:message-received event (update UI)
   - AI participants evaluate shouldRespond() via UniversalResponseEngine
   - If AI responds, new message flows through same system
5. All participants stay synchronized via room events
```

### **Room Subscription Flow:**
```
1. Participant joins room via chat/join-room command
2. ChatDaemonServer:
   - Adds participant to room
   - Calls RoomEventSystem.subscribeToRoom(sessionId, roomId)
3. Participant now receives ALL events for this room
4. When participant leaves or disconnects:
   - RoomEventSystem.unsubscribeFromRoom(sessionId, roomId)
5. Cleanup removes stale subscriptions
```

---

## 🧩 **INTEGRATION POINTS**

### **Chat Widgets (Browser)**
```typescript
ChatWidget {
  // Receives room events via JTAG browser transport
  handleRoomEvent(eventType, data, roomId) {
    switch(eventType) {
      case 'chat:message-received':
        updateMessageList(data.message)
      case 'chat:participant-joined':
        updateParticipantList(data.participant)
      // etc.
    }
  }
  
  // Sends commands via JTAG client
  sendMessage(content) {
    jtag.commands.chatSendMessage({roomId, content})
  }
}
```

### **AI Adapters (Server)**
```typescript
AIAdapter {
  // Configures auto-response behavior
  capabilities: { autoResponds: true }
  adapter: {
    type: 'ai-api',
    config: { provider: 'openai', model: 'gpt-4' },
    responseStrategy: {
      triggers: [
        {type: 'mention', probability: 1.0},
        {type: 'question', probability: 0.8}
      ]
    }
  }
  
  // UniversalResponseEngine calls this
  async generateResponse(request) {
    return await openai.chat.completions.create(...)
  }
}
```

### **Persona LoRA Integration**
```typescript
PersonaAdapter {
  capabilities: { autoResponds: true }
  adapter: {
    type: 'lora-persona',
    config: { 
      personaName: 'Sherlock Holmes',
      modelPath: '/models/sherlock-lora.bin'
    },
    responseStrategy: {
      triggers: [{type: 'keyword', value: ['mystery', 'clue']}]
    }
  }
}
```

---

## 📊 **STORAGE REQUIREMENTS**

### **Message Storage Schema**
```sql
messages (
  message_id UUID PRIMARY KEY,
  room_id UUID NOT NULL,
  sender_id UUID NOT NULL,
  sender_name TEXT NOT NULL,
  content TEXT NOT NULL,
  timestamp TIMESTAMP NOT NULL,
  category VARCHAR(20), -- 'chat', 'system', 'response'
  mentions UUID[],
  context JSONB,
  INDEX(room_id, timestamp)
)
```

### **Participants Schema**
```sql
participants (
  participant_id UUID PRIMARY KEY,
  session_id UUID NOT NULL,
  display_name TEXT NOT NULL,
  capabilities JSONB,
  adapter_config JSONB,
  joined_at TIMESTAMP,
  last_seen TIMESTAMP,
  INDEX(session_id)
)
```

### **Room Subscriptions Schema**
```sql
room_subscriptions (
  room_id UUID,
  session_id UUID,
  subscribed_at TIMESTAMP,
  PRIMARY KEY(room_id, session_id)
)
```

**Alternative**: Use existing DataDaemon with proper schemas for each entity type.

---

## 🚀 **IMPLEMENTATION PHASES**

### **Phase 1: Core Architecture (Current)**
- ✅ Universal participant types (SessionParticipant)
- ✅ Participant-agnostic event system  
- ✅ UniversalResponseEngine foundation
- 🔄 RoomEventSystem implementation
- ❌ Storage integration
- ❌ Router integration

### **Phase 2: Storage & Persistence**
- DataDaemon integration for chat storage
- Message history API
- Participant persistence
- Room state management

### **Phase 3: Transport Integration**  
- JTAG router event routing
- WebSocket transport for real-time events
- Cross-context message correlation
- Session cleanup and error handling

### **Phase 4: Widget & Adapter Development**
- Chat widget browser component
- AI adapter implementations (OpenAI, Anthropic)
- Persona LoRA adapter
- Webhook adapter for external systems

### **Phase 5: Advanced Features**
- Message threading and replies
- File sharing and media messages
- Room moderation and permissions
- Analytics and usage tracking

---

## 🔍 **RESEARCH QUESTIONS TO RESOLVE**

### **1. Router Integration Strategy**
- How exactly does JTAG router handle room-scoped event delivery?
- Can we extend existing message correlation for event tracking?
- How do we handle cross-context events (browser ↔ server)?

### **2. Storage Adapter Selection**
- Use existing DataDaemon or build chat-specific storage?
- In-memory vs. persistent storage for development/production?
- Message retention policies and archival strategies?

### **3. Session Management**
- How do we map JTAG sessionIds to chat participants?
- Session cleanup when browsers/AI agents disconnect?
- Session migration and reconnection handling?

### **4. Performance Considerations**
- Event delivery scalability (100s of rooms, 1000s of participants)
- Message history pagination and lazy loading
- Real-time event latency requirements

### **5. Security & Privacy**
- Room access control and permissions
- Message encryption for private rooms
- Audit logging for moderation
- Rate limiting for AI auto-responders

---

## 📋 **NEXT STEPS & DECISIONS NEEDED**

### **Immediate (This Session)**
1. **Router Integration Research**: How do we route room events via JTAG transport?
2. **Storage Adapter Decision**: DataDaemon integration vs. custom chat storage?
3. **Event Correlation**: How to track event delivery and handle failures?

### **Architecture Validation**
1. **Test Room Subscription**: Can we properly subscribe/unsubscribe to room events?
2. **Message Flow Verification**: Does the complete message → event → UI update flow work?
3. **AI Response Integration**: Does UniversalResponseEngine integrate with room events?

### **Integration Points**
1. **Existing JTAG Commands**: How do chat commands integrate with current command system?
2. **Browser Widget Integration**: How do room events reach browser chat widgets?
3. **Cross-Context Coordination**: How do server AI agents communicate with browser UIs?

---

## 💡 **BREAKTHROUGH INSIGHTS**

1. **85% Code Reduction**: Eliminating participant-type branching through universal interfaces
2. **Room-Scoped Events**: Prevent event spam by only sending events to room participants
3. **Adapter Pattern**: Single interface for all auto-responders (AI, bots, personas)
4. **Transport Agnostic**: Leverage existing JTAG router for event distribution
5. **Storage Abstraction**: Same history visible to all participants regardless of connection type

This architecture enables the **Universal Communication Substrate** vision: any intelligence can participate in chat through the same elegant interface.