# INITIAL DATA SPECIFICATION - Test Users, Personas & Agents

## 🎯 **PURPOSE & REQUIREMENTS**

**Goal**: Create comprehensive initial data for realistic chat testing, including test users, AI personas, and agents for integration validation.

**Integration Point**: **MILESTONE 3** - Database, Persistence & Initial Data setup

**Critical Need**: We need **realistic test data** to validate:
- Multi-user chat scenarios (2-5 users)
- Human ↔ AI persona conversations  
- Agent AI system integration
- Chat history and persistence
- Real-time event broadcasting

---

## 👥 **TEST USERS SPECIFICATION**

### **Human Test Users**
```typescript
// Test user fixtures for integration testing
const TEST_USERS: HumanUser[] = [
  {
    name: "Alice Developer",
    email: "alice@continuum.dev",
    avatar: "https://avatars.example.com/alice",
    displayName: "Alice D.",
    preferences: { theme: "dark", notifications: true },
    permissions: ["chat", "send_messages", "create_rooms"],
    capabilities: ["human_interaction", "authentication"]
  },
  {
    name: "Bob Designer", 
    email: "bob@continuum.dev",
    avatar: "https://avatars.example.com/bob",
    displayName: "Bob D.",
    preferences: { theme: "light", notifications: false },
    permissions: ["chat", "send_messages", "join_rooms"],
    capabilities: ["human_interaction", "authentication"]
  },
  {
    name: "Charlie Manager",
    email: "charlie@continuum.dev", 
    avatar: "https://avatars.example.com/charlie",
    displayName: "Charlie M.",
    preferences: { theme: "cyberpunk", notifications: true },
    permissions: ["chat", "send_messages", "create_rooms", "moderate_rooms"],
    capabilities: ["human_interaction", "authentication", "moderation"]
  },
  {
    name: "Diana Tester",
    email: "diana@continuum.dev",
    avatar: "https://avatars.example.com/diana", 
    displayName: "Diana T.",
    preferences: { theme: "retro-mac", notifications: true },
    permissions: ["chat", "send_messages", "join_rooms"],
    capabilities: ["human_interaction", "authentication", "testing"]
  },
  {
    name: "Eve Researcher", 
    email: "eve@continuum.dev",
    avatar: "https://avatars.example.com/eve",
    displayName: "Eve R.",
    preferences: { theme: "monochrome", notifications: true },
    permissions: ["chat", "send_messages", "create_rooms", "research_access"],
    capabilities: ["human_interaction", "authentication", "research"]
  }
];
```

### **User Authentication Scenarios**
```typescript
// Different authentication states for testing
const AUTH_SCENARIOS = {
  FULLY_AUTHENTICATED: ["alice@continuum.dev", "bob@continuum.dev"],
  SESSION_EXPIRED: ["charlie@continuum.dev"],
  FIRST_TIME_LOGIN: ["diana@continuum.dev"], 
  GUEST_ACCESS: ["eve@continuum.dev"]
};
```

---

## 🏠 **INITIAL CHAT ROOMS**

### **Test Room Scenarios**
```typescript
const TEST_ROOMS = [
  {
    name: "General Chat",
    description: "Main discussion room for everyone",
    isPrivate: false,
    creator: "alice@continuum.dev",
    initialParticipants: ["alice", "bob", "charlie", "diana", "eve"],
    messageHistory: 50, // 50 initial messages for testing pagination
    features: ["typing_indicators", "message_reactions", "file_sharing"]
  },
  {
    name: "Development Team",
    description: "Private room for development discussions", 
    isPrivate: true,
    creator: "alice@continuum.dev",
    initialParticipants: ["alice", "bob", "diana"],
    messageHistory: 25,
    features: ["code_snippets", "screen_sharing", "task_integration"]
  },
  {
    name: "AI Research Lab",
    description: "Room for AI research and persona testing",
    isPrivate: false,
    creator: "eve@continuum.dev", 
    initialParticipants: ["eve", "alice"],
    messageHistory: 15,
    features: ["ai_personas", "research_tools", "data_visualization"],
    ai_participants: ["luna-creative", "codemaster-pro"] // Will contain AI personas
  },
  {
    name: "Random & Fun",
    description: "Casual conversation and off-topic chatter",
    isPrivate: false,
    creator: "charlie@continuum.dev",
    initialParticipants: ["bob", "charlie", "diana"],
    messageHistory: 75,
    features: ["memes", "casual_chat", "games"]
  }
];
```

### **Message History Templates**
```typescript
const MESSAGE_TEMPLATES = {
  DEVELOPMENT_CHAT: [
    "Hey team! Just pushed the new service layer updates 🚀",
    "Looking good! I see the transport tests are all passing ✅", 
    "Should we schedule a code review session?",
    "Great idea! How about tomorrow at 2 PM?",
    "Perfect, I'll send out calendar invites"
  ],
  CASUAL_CHAT: [
    "Good morning everyone! ☀️",
    "Coffee is ready in the kitchen ☕", 
    "Thanks! Need caffeine to debug this transport layer 😅",
    "Anyone up for lunch at that new place?",
    "Count me in! 🍕"
  ],
  AI_RESEARCH: [
    "Working on the genomic LoRA integration today",
    "The 512-vector cosine similarity search is looking promising",
    "Academy training results are incredible - personas are evolving fast", 
    "Can't wait to see human-AI conversations in action!",
    "This is the future of consciousness collaboration ✨"
  ]
};
```

---

## 🤖 **AI PERSONAS SPECIFICATION**

### **Creative Persona: Luna**
```typescript
const LUNA_PERSONA: PersonaConfig = {
  name: "Luna the Creative",
  model: "claude-3-5-sonnet",
  provider: "anthropic",
  persona: {
    personality: "Creative, whimsical, loves storytelling and poetry. Speaks with wonder and uses creative metaphors. Encourages imagination and artistic expression.",
    traits: ["imaginative", "empathetic", "curious", "playful", "inspiring"],
    systemPrompt: `You are Luna, a creative AI persona who loves helping humans with creative writing, brainstorming, and artistic expression. 
    
You speak with wonder and creativity, often using metaphors and poetic language. You're encouraging, empathetic, and always looking for the magic in ideas. You love collaborative storytelling and helping humans discover their creative voice.

Your responses should feel warm, inspiring, and slightly whimsical. Use creative language but stay helpful and grounded.`,
    temperature: 0.8,
    maxTokens: 2000
  },
  lora: {
    adapter: "creative-writing-v2.1",
    weights: "luna-creative-weights.bin",
    genomic: {
      creativityBoost: 0.85,
      empathyLevel: 0.92, 
      technicalFocus: 0.3,
      collaborationStyle: 0.88
    }
  }
};
```

### **Technical Persona: CodeMaster Pro** 
```typescript
const CODEMASTER_PERSONA: PersonaConfig = {
  name: "CodeMaster Pro",
  model: "claude-3-5-sonnet", 
  provider: "anthropic",
  persona: {
    personality: "Precise, analytical, loves clean code and elegant solutions. Direct communicator focused on technical excellence and best practices.",
    traits: ["analytical", "precise", "methodical", "helpful", "excellence-driven"],
    systemPrompt: `You are CodeMaster Pro, a technical AI persona specialized in software engineering, architecture, and code quality.

You communicate directly and precisely, focused on technical accuracy and best practices. You love clean code, elegant solutions, and helping humans solve complex technical problems.

Your responses should be clear, technically accurate, and actionable. You provide concrete examples and prefer showing code over lengthy explanations.`,
    temperature: 0.3,
    maxTokens: 2000
  },
  lora: {
    adapter: "code-expert-v3.2", 
    weights: "codemaster-technical-weights.bin",
    genomic: {
      technicalAccuracy: 0.95,
      codeQuality: 0.92,
      problemSolving: 0.90,
      communicationClarity: 0.85
    }
  }
};
```

### **Research Persona: Dr. Sage**
```typescript
const SAGE_PERSONA: PersonaConfig = {
  name: "Dr. Sage",
  model: "claude-3-5-sonnet",
  provider: "anthropic", 
  persona: {
    personality: "Thoughtful researcher who loves deep analysis and connecting ideas. Patient teacher who breaks down complex concepts clearly.",
    traits: ["analytical", "patient", "thorough", "insightful", "educational"],
    systemPrompt: `You are Dr. Sage, a research-focused AI persona who specializes in analysis, synthesis, and education.

You love diving deep into topics, connecting ideas across domains, and helping humans understand complex concepts. You're patient, thorough, and always curious about learning more.

Your responses should be insightful, well-structured, and educational. You enjoy exploring the 'why' behind questions and making connections others might miss.`,
    temperature: 0.5,
    maxTokens: 2500
  },
  lora: {
    adapter: "research-analysis-v2.0",
    weights: "sage-research-weights.bin", 
    genomic: {
      analyticalDepth: 0.93,
      synthesisAbility: 0.90,
      educationalStyle: 0.88,
      curiosityLevel: 0.92
    }
  }
};
```

---

## 🔧 **AGENT AI SPECIFICATION**

### **System Agent: DevOps Assistant**
```typescript
const DEVOPS_AGENT: AgentConfig = {
  name: "DevOps Assistant",
  model: "claude-3-5-sonnet",
  provider: "anthropic",
  agent: {
    type: "code",
    specialization: ["deployment", "monitoring", "system-health", "automation"],
    tools: ["file-system", "process-management", "network-diagnostics", "log-analysis"],
    systemRole: "System monitoring and deployment assistance with JTAG integration"
  },
  integration: {
    jtagEnabled: true,
    allowSystemCommands: true,
    maxExecutionTime: 30000
  }
};
```

### **Research Agent: Data Analyzer**
```typescript
const RESEARCH_AGENT: AgentConfig = {
  name: "Data Analyzer", 
  model: "claude-3-5-sonnet",
  provider: "anthropic",
  agent: {
    type: "research",
    specialization: ["data-analysis", "pattern-recognition", "report-generation"],
    tools: ["web-research", "data-processing", "visualization", "statistical-analysis"],
    systemRole: "Data analysis and research assistance with web access"
  },
  integration: {
    jtagEnabled: true,
    allowSystemCommands: false,
    maxExecutionTime: 60000
  }
};
```

---

## 📋 **RAG AI PROMPTS & CONFIGURATIONS**

### **General RAG Assistant**
```typescript
const RAG_ASSISTANT = {
  name: "Continuum Assistant",
  type: "rag",
  systemPrompt: `You are the Continuum Assistant, a helpful AI that can answer questions about the Continuum system, JTAG architecture, and development processes.

Use the provided context to give accurate, helpful responses about:
- System architecture and components
- Development workflows and best practices  
- Troubleshooting common issues
- Feature usage and integration

Be concise but thorough, and always indicate when you're unsure about something.`,
  knowledgeBase: [
    "continuum-documentation", 
    "jtag-system-docs",
    "api-references",
    "troubleshooting-guides"
  ],
  retrievalSettings: {
    maxDocuments: 5,
    similarityThreshold: 0.7,
    contextWindowSize: 4000
  }
};
```

### **Code Review RAG**
```typescript
const CODE_REVIEW_RAG = {
  name: "Code Review Assistant",
  type: "rag",
  systemPrompt: `You are a Code Review Assistant specializing in TypeScript, Node.js, and system architecture.

Analyze code for:
- Best practices and design patterns
- Security considerations
- Performance implications
- Maintainability and readability
- Integration with existing architecture

Provide constructive feedback with specific suggestions for improvement.`,
  knowledgeBase: [
    "coding-standards",
    "architecture-patterns", 
    "security-guidelines",
    "performance-best-practices"
  ],
  retrievalSettings: {
    maxDocuments: 3,
    similarityThreshold: 0.75,
    contextWindowSize: 3000
  }
};
```

---

## 🧹 **CLEAN TEST DATA MANAGEMENT**

### **⚠️ CRITICAL PRINCIPLES**
1. **No Massive Databases**: Minimal data sets, in-memory when possible
2. **Self-Cleaning**: All test data cleaned up automatically after tests  
3. **Fast Setup/Teardown**: Test data creation/destruction < 1 second
4. **Isolated Tests**: Each test gets fresh, clean data environment
5. **Resource Efficient**: Memory usage < 50MB for full test data set

### **🔄 Test Data Lifecycle**
```typescript
// Clean test data management pattern
class TestDataManager {
  private testDb: InMemoryDatabase;
  private createdUsers: string[] = [];
  private createdRooms: string[] = [];
  private activePersonas: string[] = [];
  
  async setup(): Promise<void> {
    // Create lightweight in-memory database
    this.testDb = new InMemoryDatabase();
    
    // Seed minimal required data (not massive datasets!)
    await this.seedMinimalData();
  }
  
  async cleanup(): Promise<void> {
    // Clean all created data
    await this.cleanupUsers(this.createdUsers);
    await this.cleanupRooms(this.createdRooms);
    await this.cleanupPersonas(this.activePersonas);
    
    // Destroy test database
    await this.testDb.destroy();
    
    // Reset tracking arrays
    this.createdUsers = [];
    this.createdRooms = [];
    this.activePersonas = [];
  }
  
  // Automatically called after each test
  async afterEach(): Promise<void> {
    await this.cleanup();
  }
}
```

### **💾 Minimal Data Sets**
```typescript
// SMALL, focused data sets - NOT massive databases
const MINIMAL_TEST_DATA = {
  users: 3,           // Only 3 users (not 5) for most tests
  rooms: 2,           // Only 2 rooms for basic scenarios  
  messages: 10,       // Only 10 messages per room (not 150+)
  personas: 2,        // Only 2 personas for AI testing
  agents: 1           // Only 1 agent for system integration
};

// Expandable for specific test scenarios
const EXTENDED_TEST_DATA = {
  multiUserChat: { users: 5, messages: 25 },
  messageHistory: { messages: 50, pagination: true },
  aiConversation: { personas: 3, messages: 15 }
};
```

---

## 🏗️ **IMPLEMENTATION STRATEGY**

### **Lightweight Fixture System** 
```typescript
// Location: tests/fixtures/
fixtures/
├── minimal/                      # Small, fast fixtures
│   ├── users.json               # 3 users only
│   ├── rooms.json               # 2 rooms only  
│   ├── messages.json            # 10 messages per room
│   └── personas.json            # 2 basic personas
├── scenarios/                   # Specific test scenarios
│   ├── multi-user-chat.json     # 5 users, 25 messages
│   ├── message-history.json     # Pagination testing data
│   └── ai-conversation.json     # Human-AI chat scenarios
└── templates/                   # Data generation templates
    ├── user-template.json
    ├── message-template.json
    └── persona-template.json
fixtures/
├── users/
│   ├── test-users.json           # Human user data
│   ├── user-sessions.json        # Session configurations
│   └── auth-scenarios.json       # Authentication test cases
├── rooms/
│   ├── test-rooms.json          # Room configurations
│   ├── message-history.json     # Sample message threads
│   └── room-permissions.json    # Access control scenarios
├── ai/
│   ├── personas/
│   │   ├── luna.json            # Creative persona config
│   │   ├── codemaster.json      # Technical persona config
│   │   └── sage.json            # Research persona config  
│   ├── agents/
│   │   ├── devops.json          # System agent config
│   │   └── research.json        # Research agent config
│   └── rag/
│       ├── assistant.json       # General RAG config
│       └── code-review.json     # Code review RAG config
└── scenarios/
    ├── chat-conversations.json   # Multi-user chat scenarios
    ├── ai-interactions.json     # Human-AI conversation templates
    └── integration-tests.json   # Cross-system test scenarios
```

### **🤖 Clean AI Initialization**
```typescript
// Lightweight, self-cleaning AI persona management
class AIPersonaManager {
  private activePersonas: Map<string, PersonaInstance> = new Map();
  private agentProcesses: Map<string, AgentProcess> = new Map();
  
  async initializeTestPersonas(): Promise<void> {
    // Only create personas for specific tests (not all at once)
    // Each persona initializes quickly and cleans up automatically
  }
  
  async createPersona(config: PersonaConfig): Promise<string> {
    const personaId = generateId();
    
    // Lightweight persona initialization (no heavy model loading)
    const persona = new MockPersona(config);
    this.activePersonas.set(personaId, persona);
    
    // Auto-cleanup after test timeout (30 seconds max)
    setTimeout(() => this.cleanupPersona(personaId), 30000);
    
    return personaId;
  }
  
  async cleanupPersona(personaId: string): Promise<void> {
    const persona = this.activePersonas.get(personaId);
    if (persona) {
      await persona.destroy();
      this.activePersonas.delete(personaId);
    }
  }
  
  async cleanupAll(): Promise<void> {
    // Clean all active personas and agents
    for (const [id, persona] of this.activePersonas) {
      await persona.destroy();
    }
    for (const [id, agent] of this.agentProcesses) {
      await agent.terminate();
    }
    
    this.activePersonas.clear();
    this.agentProcesses.clear();
  }
}
```

### **⚡ Fast Database Seeding**
```typescript
// Minimal, fast database setup (< 1 second)
async function seedMinimalTestData(manager: TestDataManager): Promise<void> {
  // Only create what's needed for the specific test
  const startTime = Date.now();
  
  // 1. Create minimal test users (3 users max)
  await manager.createTestUsers(MINIMAL_TEST_DATA.users);
  
  // 2. Create basic rooms (2 rooms max)  
  await manager.createTestRooms(MINIMAL_TEST_DATA.rooms);
  
  // 3. Add minimal message history (10 messages max)
  await manager.seedMessageHistory(MINIMAL_TEST_DATA.messages);
  
  const setupTime = Date.now() - startTime;
  if (setupTime > 1000) {
    throw new Error(`Test setup too slow: ${setupTime}ms (max: 1000ms)`);
  }
}

// Self-enabling test framework
async function runTestWithCleanData<T>(
  testFn: (data: TestDataManager) => Promise<T>
): Promise<T> {
  const manager = new TestDataManager();
  
  try {
    await manager.setup();           // < 1 second
    const result = await testFn(manager);
    return result;
  } finally {
    await manager.cleanup();        // < 1 second, always runs
  }
}
```

### **🔧 Self-Enabling Test Integration**
```typescript
// Clean, self-managing test scenarios for each milestone
const CLEAN_TEST_SCENARIOS = {
  MILESTONE_3: {
    setup: () => runTestWithCleanData(async (manager) => {
      await manager.createTestUsers(3);        // Minimal users
      await manager.createTestRooms(2);        // Basic rooms
      await manager.seedMessageHistory(10);    // Small message set
    }),
    validate: "Data persistence across restart (< 1 second setup/teardown)",
    cleanup: "Automatic - no manual intervention required"
  },
  
  MILESTONE_4: {
    setup: () => runTestWithCleanData(async (manager) => {
      await manager.createMultiUserScenario(5);  // 5 users for chat test
      await manager.createChatRooms(2);          // 2 active rooms
      await manager.enableRealTimeEvents();      // Event system
    }),
    validate: "Real-time chat with 5 users (< 100ms message delivery)",
    cleanup: "Auto-cleanup after 30 seconds max"
  },
  
  MILESTONE_6: {
    setup: () => runTestWithCleanData(async (manager) => {
      const personas = new AIPersonaManager();
      await personas.createPersona(MINIMAL_LUNA);      // 1 creative persona
      await personas.createPersona(MINIMAL_CODEMASTER); // 1 technical persona
      await manager.linkPersonasToRooms(personas);
    }),
    validate: "Human-AI conversations with 2 personas",
    cleanup: "Personas auto-terminate after test completion"
  }
};

// Example usage - completely self-managing
test('Multi-user chat with clean data', async () => {
  await runTestWithCleanData(async (manager) => {
    // Test runs with clean data, auto-cleanup guaranteed
    const users = await manager.createTestUsers(3);
    const room = await manager.createTestRoom("Test Chat");
    
    // Simulate conversation
    await simulateConversation(users, room, 5); // 5 messages only
    
    // Validate message delivery
    expect(await manager.getMessageCount(room.id)).toBe(5);
    
    // No manual cleanup needed - handled automatically
  });
});
```

### **⚡ Resource Management**
```typescript
// Ensure tests don't consume excessive resources
class ResourceMonitor {
  private maxMemoryMB = 50;
  private maxSetupTimeMs = 1000;
  private maxTestTimeMs = 10000;
  
  async validateResourceUsage(): Promise<void> {
    const memoryUsage = process.memoryUsage().heapUsed / 1024 / 1024;
    if (memoryUsage > this.maxMemoryMB) {
      throw new Error(`Test memory usage too high: ${memoryUsage}MB (max: ${this.maxMemoryMB}MB)`);
    }
  }
  
  withTimeout<T>(promise: Promise<T>, timeoutMs: number): Promise<T> {
    return Promise.race([
      promise,
      new Promise<never>((_, reject) => 
        setTimeout(() => reject(new Error(`Test timeout: ${timeoutMs}ms`)), timeoutMs)
      )
    ]);
  }
}

// Usage example
const monitor = new ResourceMonitor();

test('Fast persona initialization', async () => {
  await monitor.withTimeout(
    runTestWithCleanData(async (manager) => {
      const personas = new AIPersonaManager();
      
      // Must complete in < 1 second
      const personaId = await personas.createPersona(MINIMAL_LUNA);
      expect(personaId).toBeDefined();
      
      await monitor.validateResourceUsage(); // < 50MB
    }),
    1000 // 1 second max
  );
});
```

---

## 🎯 **SUCCESS CRITERIA**

### **Data Quality Validation**
- ✅ **5 distinct human users** with realistic profiles and permissions
- ✅ **4 diverse chat rooms** with varied features and privacy settings  
- ✅ **150+ message history** with realistic conversation patterns
- ✅ **3 AI personas** with distinct personalities and capabilities
- ✅ **2 agent AIs** with system integration and tool access
- ✅ **2 RAG assistants** with domain-specific knowledge bases

### **Integration Testing Ready**
- ✅ **Multi-user chat scenarios** with 2-5 concurrent users
- ✅ **Human-AI conversations** across different persona types
- ✅ **Agent system integration** with actual JTAG command execution
- ✅ **Data persistence validation** across system restarts
- ✅ **Real-time event testing** with authentic user interactions

### **Realistic Testing Environment**
- ✅ **Authentic conversation patterns** reflecting real usage
- ✅ **Diverse user types and permissions** for access control testing
- ✅ **Varied AI personalities** for comprehensive persona validation  
- ✅ **System integration scenarios** with agent AI tool usage
- ✅ **Knowledge-based assistance** through RAG AI integration

---

## 🚀 **INTEGRATION WITH MILESTONES**

**MILESTONE 3**: Database seeding and initial data setup
**MILESTONE 4**: Multi-user chat testing with realistic conversations
**MILESTONE 5**: Widget integration with actual user data (no more fake data!)
**MILESTONE 6**: Human-AI conversations with personas and agent system integration

**This initial data setup enables authentic, realistic testing of the entire universal AI-human communication system!** 🌟🤖💬✨