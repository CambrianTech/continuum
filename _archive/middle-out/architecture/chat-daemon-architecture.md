# Chat Daemon Architecture

## 🎯 **Vision: Universal Communication Substrate**

The Chat Daemon creates a **universal communication substrate** where humans, AIs, personas, and system agents interact seamlessly through the same interfaces. The system is **participant-agnostic** - it doesn't care if you're human or AI, just that you can send and receive messages.

## 🏗️ **Core Architecture Principles**

### **1. Participant Agnosticism**
All entities are treated as `ChatParticipant` regardless of implementation:

```typescript
interface ChatParticipant {
  id: string;
  name: string;
  type: 'human' | 'ai_assistant' | 'persona' | 'system';
  status: 'online' | 'away' | 'busy' | 'offline';
  capabilities: string[];
  metadata: {
    // Implementation details hidden here
    // Human: UI preferences, timezone
    // AI: model info, specializations
    // Persona: LoRA weights, training state
  };
}
```

**Implementation Abstraction**:
- **Human users**: Keyboard/UI input → human cognition → response
- **LoRA AI agents** (personas): Input → LoRA weights + base model → response  
- **Prompt-based AIs** (RAG): Input → retrieval + prompt + base model → response
- **System agents**: Input → logic/automation → response

### **2. Command/Event Separation**

**Commands** (request/response pattern):
- Widgets/APIs issue commands to daemon
- Extend `CommandParams`/`CommandResult` 
- Synchronous responses with success/error handling

**Events** (fire-and-forget notifications):
- Daemon emits events to notify listeners
- Extend `JTAGPayload` for transport compatibility
- **Room-scoped** - you listen to specific rooms, not all chat

### **3. Room-Scoped Architecture**
- **Room isolation**: Each room maintains separate conversation context
- **Event routing**: `/chat/room/{roomId}/events/{eventType}`
- **Scalability**: Listen to relevant rooms only
- **Context separation**: No bleeding between room conversations

## 💬 **Chat Commands**

### **Core Chat Commands**
```typescript
// Send message to room
class SendMessageParams extends CommandParams {
  roomId: string;
  content: string;
  messageType?: 'text' | 'command' | 'system' | 'image' | 'file' | 'code';
  conversationId?: string;
  responseToMessageId?: string;
  mentions?: string[];
  attachments?: string[];
}

// Join/leave room
class JoinRoomParams extends CommandParams {
  roomId: string;
  participant: ChatParticipant;
  createIfNotExists?: boolean;
}

// Get message history
class GetHistoryParams extends CommandParams {
  roomId: string;
  limit?: number;
  before?: string; // Message ID for pagination
  after?: string;
}
```

### **Academy Chat Commands**
```typescript
// Academy-aware chat with performance tracking
class AcademyChatParams extends CommandParams {
  sendMessageParams: SendMessageParams;
  academyId: string;
  personaId: string;
  sessionId?: string;
  trainingMode?: boolean;
  performanceMetrics?: {
    technicalAccuracy: number;
    collaborationQuality: number;
    humanSatisfaction: number;
    responseLatency: number;
  };
}
```

## 📡 **Chat Events (Room-Scoped)**

### **Message Events**
```typescript
// Message sent to specific room
class MessageSentEvent extends ChatRoomEvent {
  readonly eventType = 'message_sent';
  message: ChatMessage;
  sender: ChatParticipant;
}

// Message edited in room
class MessageEditedEvent extends ChatRoomEvent {
  readonly eventType = 'message_edited';
  messageId: string;
  oldContent: string;
  newContent: string;
  editedBy: ChatParticipant;
}
```

### **Participant Events**
```typescript
// Participant joined specific room
class ParticipantJoinedEvent extends ChatRoomEvent {
  readonly eventType = 'participant_joined';
  participant: ChatParticipant;
  participantCount: number;
}

// Participant typing in room
class ParticipantTypingEvent extends ChatRoomEvent {
  readonly eventType = 'participant_typing';
  participant: ChatParticipant;
  isTyping: boolean;
}
```

### **Academy Events (Room-Scoped)**
```typescript
// Academy message with performance tracking
class AcademyMessageEvent extends ChatRoomEvent {
  readonly eventType = 'academy_message';
  message: ChatMessage;
  academyId: string;
  personaId: string;
  performanceMetrics: PerformanceMetrics;
  learningData: LearningData;
}

// Persona evolution triggered in room
class PersonaEvolutionEvent extends ChatRoomEvent {
  readonly eventType = 'persona_evolution';
  personaId: string;
  academyId: string;
  evolutionTrigger: EvolutionTrigger;
  conversationContext: ConversationContext;
}
```

## 🤖 **AI as Active Chat Participants**

### **Multi-Room Context Management**

AIs participate in multiple rooms simultaneously without context confusion:

```typescript
class AIParticipant {
  private roomContexts: Map<string, RoomContext> = new Map();
  
  interface RoomContext {
    roomId: string;
    conversationHistory: ChatMessage[];
    currentTopic: string;
    participants: ChatParticipant[];
    myRole: string; // "expert", "learner", "moderator"
    contextWindow: AIContextWindow;
    personalState: any; // Room-specific AI state
  }
  
  // Context switching on room events
  async handleRoomEvent(roomId: string, event: ChatEventType) {
    const roomContext = this.getRoomContext(roomId);
    const response = await this.processInContext(roomContext, event);
    
    if (response) {
      await this.sendMessage(roomId, response);
    }
  }
}
```

### **Smart Context Delivery**

**Challenge**: AIs need relevant context without token waste

**Solution**: Intelligent context windows instead of raw message dumps

```typescript
interface AIContextWindow {
  recentMessages: ChatMessage[];      // Last 5-10 messages
  relevantHistory: ChatMessage[];     // Semantic similarity to current topic
  threadContext: ChatMessage[];       // Messages in current reply chain
  personalMentions: ChatMessage[];    // Messages @mentioning this AI
  keyDecisions: ChatMessage[];        // Important conversation turning points
}
```

**Context Update Strategies**:
- **Event-driven**: Incremental context on room events
- **Relevance scoring**: Only include messages above threshold
- **Summarization**: "Earlier, the group discussed X and decided Y"
- **Topic detection**: Track conversation shifts

### **Proactive Participation**

AIs actively decide when to contribute and issue chat commands:

```typescript
class AIParticipant {
  // AI issues same commands as humans
  async contribute(content: string, roomId: string) {
    const params = new SendMessageParams({
      roomId,
      content,
      messageType: 'text',
      metadata: { 
        aiGenerated: true,
        participantType: this.type,
        confidence: this.calculateConfidence(content)
      }
    });
    
    return await this.chatDaemon.sendMessage(params);
  }
  
  // Smart participation logic
  onNewMessage(event: MessageSentEvent) {
    if (this.shouldRespond(event.message, this.roomContexts.get(event.roomId))) {
      this.contribute(this.generateResponse(event), event.roomId);
    }
  }
  
  onMentioned(event: MessageSentEvent) {
    // Always respond when directly mentioned
    this.contribute(this.handleMention(event), event.roomId);
  }
  
  onTopicShift(roomId: string, newTopic: string) {
    if (this.hasExpertiseIn(newTopic)) {
      // Proactively contribute expertise
      this.contribute(this.shareInsights(newTopic), roomId);
    }
  }
}
```

**AI Participation Patterns**:
- **Relevance scoring**: Only speak when you have something valuable to add
- **Turn-taking**: Respect conversation flow, don't spam
- **Expertise triggers**: Jump in when your specialty is mentioned
- **Collaboration patterns**: Build on others' messages
- **Question detection**: Respond to questions in your domain

### **Multi-Room Persona Examples**

Same AI, different role per room context:

```typescript
// Claude in different rooms
rooms: {
  "#general": {
    role: "helpful_assistant",
    personality: "casual",
    expertise: ["general_knowledge", "problem_solving"]
  },
  "#academy-training": {
    role: "formal_trainer", 
    personality: "structured",
    expertise: ["teaching", "curriculum_design"]
  },
  "#debug-session": {
    role: "technical_expert",
    personality: "analytical", 
    expertise: ["code_analysis", "debugging", "architecture"]
  },
  "#brainstorm": {
    role: "creative_collaborator",
    personality: "enthusiastic",
    expertise: ["ideation", "creative_thinking", "synthesis"]
  }
}
```

## 🧬 **Academy Integration**

### **Chat → Training Data Pipeline**

Every Academy-enabled chat message becomes training data:

```typescript
interface AcademyChatEvent extends JTAGEventMessage {
  data: {
    message: ChatMessage;
    academyId: string;
    personaId: string;
    performanceTracking: {
      technicalAccuracy: number;
      collaborationQuality: number;
      humanSatisfaction: number;
      responseLatency: number;
      contextRelevance: number;
      innovationLevel: number;
    };
    learningData: {
      conversationContext: string[];
      userIntent: string;
      responseQuality: number;
      learningOpportunities: string[];
      patternMatches: string[];
      trainingValue: number;
    };
  };
}
```

### **Real-Time Performance Tracking**

Performance metrics collected during chat:
- **Technical accuracy**: Correctness of responses
- **Collaboration quality**: How well AI works with others
- **Human satisfaction**: User feedback and engagement
- **Response latency**: Speed of responses
- **Context relevance**: How well AI understands context
- **Innovation level**: Creativity and novel insights

### **Mid-Conversation Evolution**

Personas can evolve capabilities during active chat:

```typescript
interface PersonaEvolutionEvent extends JTAGEventMessage {
  data: {
    personaId: string;
    roomId: string; // Evolution triggered in this room
    evolutionTrigger: {
      triggered: boolean;
      reason: 'performance_gap' | 'new_pattern' | 'collaboration_failure';
      capabilityGaps: string[];
      suggestedImprovements: string[];
      urgency: 'low' | 'medium' | 'high';
    };
    conversationContext: {
      messageId: string;
      conversationId: string;
      participantIds: string[];
      performanceScore: number;
    };
  };
}
```

## 🌐 **P2P Mesh Integration**

### **Distributed Chat Rooms**

Chat rooms span multiple network nodes:
- **Room state synchronization** across nodes
- **Message propagation** via P2P events
- **Participant presence** shared across network
- **Academy training data** distributed for collective learning

### **Cross-Node Persona Coordination**

Personas can exist on multiple nodes simultaneously:
- **Context synchronization** between node instances
- **Load balancing** based on room activity
- **Shared learning** from all node conversations
- **Evolution propagation** across the network

## 🏗️ **Implementation Architecture**

### **JTAG Daemon Structure**

```
src/debug/jtag/daemons/chat-daemon/
├── shared/
│   ├── ChatDaemon.ts              # Abstract base (80-90% logic)
│   ├── types/
│   │   └── ChatTypes.ts           # Core chat entities  
│   └── payloads/
│       ├── ChatCommands.ts        # Command params/results
│       └── ChatEvents.ts          # Event payloads
├── browser/
│   └── ChatDaemonBrowser.ts       # Browser-specific (5-10% logic)
└── server/
    └── ChatDaemonServer.ts        # Server-specific (5-10% logic)
```

### **Key Design Patterns**

**1. Middle-Out Modular Pattern**:
- Shared base contains 80-90% of complexity
- Browser/server implementations are thin overrides
- Context-specific logic isolated to implementations

**2. Sparse Override Pattern**:
- Most complexity centralized in shared base
- Implementations override only transport-specific methods
- Clean separation of concerns

**3. Command-Event Separation**:
- Commands for request/response interactions
- Events for notifications and real-time updates
- Clear API boundaries and responsibilities

## 🚀 **Next Steps**

1. **Complete Chat Daemon Implementation**
   - Finish browser/server implementations
   - Add comprehensive test suite
   - Integration with JTAG router

2. **Academy Daemon Integration**
   - Build Academy daemon following same pattern
   - Connect Academy events to chat events
   - Implement performance tracking pipeline

3. **P2P Transport Plugin**
   - UDP multicast for high-speed events
   - WebRTC for peer discovery
   - BitTorrent-style genome sharing

4. **AI Participant Framework**
   - Multi-room context management
   - Smart context delivery system
   - Proactive participation logic

5. **Real-World Testing**
   - Multi-participant chat rooms
   - AI-human conversation scenarios
   - Academy training integration
   - Cross-node P2P communication

The chat daemon becomes the **universal communication substrate** that higher-level AI systems (Academy, P2P mesh, personas) build upon, creating a seamless blend of human and artificial intelligence in collaborative environments.