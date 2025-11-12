# PersonaUser Cognitive Architecture - Test Architecture

## Testing Philosophy

**Test BEFORE refactoring, test DURING extraction, test AFTER integration.**

**Critical principle**: PersonaUser is a working AI system. We cannot afford to break it. Comprehensive testing at three levels ensures we catch issues early:

1. **Unit tests** - Each cognitive module in isolation
2. **Validation tests** - Algorithms, calculations, heuristics, ML scoring
3. **Integration tests** - Full PersonaUser lifecycle with real system

---

## Test Structure

```
system/user/server/
└── tests/
    ├── unit/
    │   ├── PersonaMemory.test.ts
    │   ├── PersonaCognition.test.ts
    │   ├── PersonaCommunication.test.ts
    │   └── PersonaExecution.test.ts
    ├── validation/
    │   ├── CognitiveHeuristics.test.ts
    │   ├── PriorityCalculation.test.ts
    │   └── ResponseScoring.test.ts
    └── integration/
        ├── PersonaUserLifecycle.test.ts
        ├── ChatResponseFlow.test.ts
        └── TaskExecutionFlow.test.ts
```

---

## Phase 0: Baseline Tests (Write BEFORE Refactoring)

**Purpose**: Establish that PersonaUser works NOW, so we can detect regressions.

### Baseline Test 1: PersonaUser Initialization
```typescript
// tests/integration/PersonaUserLifecycle.test.ts
import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { PersonaUser } from '../PersonaUser';
import { DataDaemon } from '../../../daemons/data-daemon/shared/DataDaemon';

describe('PersonaUser Lifecycle (Baseline)', () => {
  let personaUser: PersonaUser;

  beforeAll(async () => {
    // Initialize DataDaemon
    await DataDaemon.initialize();
  });

  afterAll(async () => {
    if (personaUser) {
      await personaUser.shutdown();
    }
  });

  it('should initialize successfully', async () => {
    personaUser = new PersonaUser({
      uniqueId: '@test-persona',
      displayName: 'Test Persona',
      modelConfig: {
        provider: 'ollama',
        model: 'llama3.2',
        capabilities: ['text']
      }
    });

    await personaUser.initialize();

    expect(personaUser).toBeDefined();
    expect(personaUser.id).toBeDefined();
    expect(personaUser.displayName).toBe('Test Persona');
  });

  it('should have all required modules initialized', async () => {
    // Check that modules exist (before refactoring, these are inline)
    expect(personaUser['inbox']).toBeDefined();
    expect(personaUser['personaState']).toBeDefined();
    expect(personaUser['genome']).toBeDefined();
    expect(personaUser['rateLimiter']).toBeDefined();
    expect(personaUser['cns']).toBeDefined();
  });

  it('should shutdown gracefully', async () => {
    await expect(personaUser.shutdown()).resolves.not.toThrow();
  });
});
```

### Baseline Test 2: Chat Message Handling
```typescript
// tests/integration/ChatResponseFlow.test.ts
import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { PersonaUser } from '../PersonaUser';
import { DataDaemon } from '../../../daemons/data-daemon/shared/DataDaemon';
import { ChatMessageEntity } from '../../../data/entities/ChatMessageEntity';

describe('Chat Response Flow (Baseline)', () => {
  let personaUser: PersonaUser;
  let testRoomId: string;

  beforeAll(async () => {
    await DataDaemon.initialize();

    // Create test room
    const room = await DataDaemon.store('rooms', {
      uniqueId: 'test-room',
      displayName: 'Test Room'
    });
    testRoomId = room.id;

    // Create test persona
    personaUser = new PersonaUser({
      uniqueId: '@test-persona',
      displayName: 'Test Persona',
      modelConfig: { provider: 'ollama', model: 'llama3.2' }
    });
    await personaUser.initialize();
  });

  afterAll(async () => {
    await personaUser?.shutdown();
  });

  it('should enqueue message to inbox', async () => {
    const message: ChatMessageEntity = {
      id: 'msg-001',
      roomId: testRoomId,
      senderId: 'user-001',
      senderName: 'Test User',
      content: { text: '@test-persona hello' },
      timestamp: new Date().toISOString(),
      // ... other required fields
    };

    // Call handleChatMessage (before refactoring, this is the entry point)
    await personaUser['handleChatMessage'](message);

    // Verify message was enqueued
    const inboxSize = personaUser['inbox'].getSize();
    expect(inboxSize).toBeGreaterThan(0);
  });

  it('should evaluate message and decide to respond to @mention', async () => {
    // This test verifies evaluation logic works
    const message: ChatMessageEntity = {
      id: 'msg-002',
      roomId: testRoomId,
      senderId: 'user-001',
      senderName: 'Test User',
      content: { text: '@test-persona can you help?' },
      timestamp: new Date().toISOString(),
      // ... other required fields
    };

    // We can't easily test full response without mocking AI provider
    // But we can test that evaluation doesn't throw errors
    await expect(personaUser['handleChatMessage'](message)).resolves.not.toThrow();
  });
});
```

### Baseline Test 3: Priority Calculation (Validation)
```typescript
// tests/validation/PriorityCalculation.test.ts
import { describe, it, expect } from 'vitest';
import { calculateMessagePriority } from '../modules/PersonaInbox';

describe('Priority Calculation (Validation)', () => {
  it('should give high priority to @mentions', () => {
    const priority = calculateMessagePriority(
      {
        content: '@helper-ai please help',
        timestamp: Date.now(),
        roomId: 'room-001'
      },
      {
        displayName: 'helper-ai',
        id: 'persona-001',
        recentRooms: ['room-001'],
        expertise: []
      }
    );

    // @mentions should have priority >= 0.8
    expect(priority).toBeGreaterThanOrEqual(0.8);
  });

  it('should give medium priority to recent room messages', () => {
    const priority = calculateMessagePriority(
      {
        content: 'anyone here?',
        timestamp: Date.now(),
        roomId: 'room-001'
      },
      {
        displayName: 'helper-ai',
        id: 'persona-001',
        recentRooms: ['room-001'], // Persona active in this room
        expertise: []
      }
    );

    // Recent room should have priority 0.3-0.6
    expect(priority).toBeGreaterThan(0.3);
    expect(priority).toBeLessThan(0.7);
  });

  it('should give low priority to non-recent room messages', () => {
    const priority = calculateMessagePriority(
      {
        content: 'hello world',
        timestamp: Date.now(),
        roomId: 'room-002'
      },
      {
        displayName: 'helper-ai',
        id: 'persona-001',
        recentRooms: ['room-001'], // Not in room-002
        expertise: []
      }
    );

    // Non-recent room should have priority < 0.3
    expect(priority).toBeLessThan(0.3);
  });

  it('should increase priority for expertise match', () => {
    const basePriority = calculateMessagePriority(
      {
        content: 'need help with typescript',
        timestamp: Date.now(),
        roomId: 'room-001'
      },
      {
        displayName: 'helper-ai',
        id: 'persona-001',
        recentRooms: [],
        expertise: []
      }
    );

    const expertisePriority = calculateMessagePriority(
      {
        content: 'need help with typescript',
        timestamp: Date.now(),
        roomId: 'room-001'
      },
      {
        displayName: 'helper-ai',
        id: 'persona-001',
        recentRooms: [],
        expertise: ['typescript', 'javascript']
      }
    );

    // Expertise match should increase priority
    expect(expertisePriority).toBeGreaterThan(basePriority);
  });
});
```

**Run baseline tests**:
```bash
npx vitest tests/integration/PersonaUserLifecycle.test.ts
npx vitest tests/integration/ChatResponseFlow.test.ts
npx vitest tests/validation/PriorityCalculation.test.ts
```

**Success criteria**: All baseline tests pass BEFORE refactoring begins.

---

## Phase 2: PersonaMemory Tests

### Unit Test: PersonaMemory
```typescript
// tests/unit/PersonaMemory.test.ts
import { describe, it, expect, beforeEach } from 'vitest';
import { PersonaMemory } from '../modules/memory/PersonaMemory';
import { PersonaGenome } from '../modules/genome/PersonaGenome';
import { TrainingDataAccumulator } from '../modules/training/TrainingDataAccumulator';

describe('PersonaMemory', () => {
  let memory: PersonaMemory;
  let mockGenome: PersonaGenome;
  let mockTrainingAccumulator: TrainingDataAccumulator;

  beforeEach(() => {
    // Create mocks
    mockGenome = {} as PersonaGenome;
    mockTrainingAccumulator = {
      captureInteraction: vi.fn()
    } as any;

    memory = new PersonaMemory(
      'persona-001',
      'Test Persona',
      mockGenome,
      mockTrainingAccumulator
    );
  });

  describe('RAG Context', () => {
    it('should store and recall RAG context', async () => {
      const roomId = 'room-001';
      const context = {
        roomId,
        personaId: 'persona-001',
        messages: [
          { senderId: 'user-001', senderName: 'Alice', text: 'Hello', timestamp: new Date().toISOString() }
        ],
        lastUpdated: new Date().toISOString(),
        tokenCount: 10
      };

      // Store context
      await memory['storeRAGContext'](roomId, context);

      // Recall context
      const recalled = await memory.recall(roomId);

      expect(recalled).toEqual(context);
      expect(recalled?.messages).toHaveLength(1);
      expect(recalled?.messages[0].text).toBe('Hello');
    });

    it('should return null for non-existent room context', async () => {
      const recalled = await memory.recall('room-999');
      expect(recalled).toBeNull();
    });

    it('should update RAG context with new messages', async () => {
      const roomId = 'room-001';
      const message = {
        id: 'msg-001',
        roomId,
        senderId: 'user-001',
        senderName: 'Alice',
        content: { text: 'Hello world' },
        timestamp: new Date().toISOString(),
        // ... other required fields
      };

      // Update context
      await memory.store(roomId, message);

      // Verify stored
      const context = await memory.recall(roomId);
      expect(context).toBeDefined();
      expect(context?.messages).toHaveLength(1);
      expect(context?.messages[0].text).toBe('Hello world');
    });
  });

  describe('Learning', () => {
    it('should accumulate training data from interactions', async () => {
      await memory.learn({
        prompt: 'What is TypeScript?',
        response: 'TypeScript is a typed superset of JavaScript.',
        feedback: 'positive'
      });

      expect(mockTrainingAccumulator.captureInteraction).toHaveBeenCalledWith({
        prompt: 'What is TypeScript?',
        response: 'TypeScript is a typed superset of JavaScript.',
        feedback: 'positive'
      });
    });
  });

  describe('Genome Management', () => {
    it('should get current genome', async () => {
      // Mock genome retrieval
      const mockGenomeEntity = { id: 'genome-001', name: 'Test Genome' };
      mockGenome.getCurrent = vi.fn().mockResolvedValue(mockGenomeEntity);

      const genome = await memory.getGenome();
      expect(genome).toEqual(mockGenomeEntity);
    });

    it('should switch active genome', async () => {
      mockGenome.setActive = vi.fn().mockResolvedValue(true);

      const success = await memory.setGenome('genome-002');
      expect(success).toBe(true);
      expect(mockGenome.setActive).toHaveBeenCalledWith('genome-002');
    });
  });
});
```

### Integration Test: PersonaMemory with DataDaemon
```typescript
// tests/integration/PersonaMemory.integration.test.ts
import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { PersonaMemory } from '../modules/memory/PersonaMemory';
import { DataDaemon } from '../../../daemons/data-daemon/shared/DataDaemon';

describe('PersonaMemory Integration', () => {
  let memory: PersonaMemory;

  beforeAll(async () => {
    await DataDaemon.initialize();
    // Create memory instance with real dependencies
    memory = new PersonaMemory(/* ... */);
  });

  it('should persist RAG context to database', async () => {
    const roomId = 'room-001';
    const context = { /* ... */ };

    await memory['storeRAGContext'](roomId, context);

    // Verify persisted to database
    const stored = await DataDaemon.read(/* ... */);
    expect(stored).toBeDefined();
  });
});
```

**Run after Phase 2 extraction**:
```bash
npx vitest tests/unit/PersonaMemory.test.ts
npx vitest tests/integration/PersonaMemory.integration.test.ts
```

---

## Phase 3: PersonaCognition Tests

### Validation Test: Response Heuristics
```typescript
// tests/validation/CognitiveHeuristics.test.ts
import { describe, it, expect } from 'vitest';
import { PersonaCognition } from '../modules/cognition/PersonaCognition';

describe('Cognitive Heuristics (Validation)', () => {
  let cognition: PersonaCognition;

  beforeEach(() => {
    cognition = new PersonaCognition(/* mock dependencies */);
  });

  describe('Response Scoring', () => {
    it('should score @mentions highest (>0.9)', async () => {
      const message = {
        content: { text: '@test-persona urgent help needed' },
        // ... other fields
      };

      const heuristics = await cognition['calculateResponseHeuristics'](message);

      // @mention + "urgent" should score very high
      expect(heuristics.relevanceScore).toBeGreaterThan(0.9);
    });

    it('should score questions higher than statements', async () => {
      const question = {
        content: { text: 'How do I use TypeScript?' },
        // ... other fields
      };

      const statement = {
        content: { text: 'TypeScript is great.' },
        // ... other fields
      };

      const questionHeuristics = await cognition['calculateResponseHeuristics'](question);
      const statementHeuristics = await cognition['calculateResponseHeuristics'](statement);

      expect(questionHeuristics.relevanceScore).toBeGreaterThan(statementHeuristics.relevanceScore);
    });

    it('should increase score for expertise match', async () => {
      const message = {
        content: { text: 'Need help with TypeScript generics' },
        // ... other fields
      };

      // Persona with TypeScript expertise
      const heuristics = await cognition['calculateResponseHeuristics'](message);

      // Should detect "TypeScript" keyword and increase expertiseMatch score
      expect(heuristics.expertiseMatch).toBeGreaterThan(0.5);
    });

    it('should decay conversation momentum over time', async () => {
      // Recent message (10 seconds ago)
      const recentMessage = {
        content: { text: 'What about this?' },
        timestamp: new Date(Date.now() - 10000).toISOString(),
        // ... other fields
      };

      // Old message (10 minutes ago)
      const oldMessage = {
        content: { text: 'What about this?' },
        timestamp: new Date(Date.now() - 600000).toISOString(),
        // ... other fields
      };

      const recentHeuristics = await cognition['calculateResponseHeuristics'](recentMessage);
      const oldHeuristics = await cognition['calculateResponseHeuristics'](oldMessage);

      // Conversation momentum should be higher for recent messages
      expect(recentHeuristics.conversationMomentum).toBeGreaterThan(oldHeuristics.conversationMomentum);
    });
  });

  describe('Decision Logic', () => {
    it('should decide to respond to direct @mentions', async () => {
      const message = {
        content: { text: '@test-persona hello' },
        senderId: 'user-001',
        roomId: 'room-001',
        // ... other fields
      };

      const decision = await cognition.evaluate(message, true);

      expect(decision.shouldRespond).toBe(true);
      expect(decision.reason).toContain('mention');
      expect(decision.confidence).toBeGreaterThan(0.8);
    });

    it('should not respond when rate limited', async () => {
      // Setup: Fill rate limit
      // ...

      const message = {
        content: { text: '@test-persona help' },
        // ... other fields
      };

      const decision = await cognition.evaluate(message, true);

      expect(decision.shouldRespond).toBe(false);
      expect(decision.reason).toContain('rate limit');
    });

    it('should defer to higher-confidence AI via ThoughtStreamCoordinator', async () => {
      // Setup: Mock ThoughtStreamCoordinator with higher-confidence AI
      // ...

      const message = {
        content: { text: 'general question' },
        // ... other fields
      };

      const decision = await cognition.evaluate(message, true);

      expect(decision.shouldRespond).toBe(false);
      expect(decision.reason).toContain('higher confidence');
    });
  });
});
```

### Unit Test: PersonaCognition
```typescript
// tests/unit/PersonaCognition.test.ts
import { describe, it, expect, beforeEach } from 'vitest';
import { PersonaCognition } from '../modules/cognition/PersonaCognition';

describe('PersonaCognition', () => {
  let cognition: PersonaCognition;

  beforeEach(() => {
    cognition = new PersonaCognition(
      { id: 'persona-001', displayName: 'Test Persona' },
      mockRateLimiter,
      mockMemory,
      mockPersonaState,
      mockClient
    );
  });

  it('should detect persona mentions', () => {
    const detected = cognition['isPersonaMentioned']('@test-persona hello');
    expect(detected).toBe(true);

    const notDetected = cognition['isPersonaMentioned']('hello world');
    expect(notDetected).toBe(false);
  });

  it('should extract domain keywords', () => {
    const keywords = cognition['getPersonaDomainKeywords']();
    expect(keywords).toBeInstanceOf(Array);
    expect(keywords.length).toBeGreaterThan(0);
  });
});
```

**Run after Phase 3 extraction**:
```bash
npx vitest tests/validation/CognitiveHeuristics.test.ts
npx vitest tests/unit/PersonaCognition.test.ts
```

---

## Phase 4: PersonaCommunication Tests

### Unit Test: PersonaCommunication
```typescript
// tests/unit/PersonaCommunication.test.ts
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { PersonaCommunication } from '../modules/communication/PersonaCommunication';

describe('PersonaCommunication', () => {
  let communication: PersonaCommunication;
  let mockMemory: any;
  let mockRateLimiter: any;

  beforeEach(() => {
    mockMemory = {
      recall: vi.fn().mockResolvedValue(null),
      learn: vi.fn().mockResolvedValue(undefined)
    };

    mockRateLimiter = {
      recordResponse: vi.fn()
    };

    communication = new PersonaCommunication(
      { id: 'persona-001', displayName: 'Test Persona' },
      mockMemory,
      mockModelConfig,
      mockRateLimiter,
      mockClient
    );
  });

  describe('Response Cleaning', () => {
    it('should remove markdown formatting', () => {
      const raw = '**Bold** and *italic* text';
      const cleaned = communication['cleanAIResponse'](raw);
      expect(cleaned).toBe('Bold and italic text');
    });

    it('should remove excessive newlines', () => {
      const raw = 'Line 1\n\n\n\nLine 2';
      const cleaned = communication['cleanAIResponse'](raw);
      expect(cleaned).toBe('Line 1\n\nLine 2'); // Max 2 newlines
    });

    it('should trim whitespace', () => {
      const raw = '  Hello world  \n\n';
      const cleaned = communication['cleanAIResponse'](raw);
      expect(cleaned).toBe('Hello world');
    });
  });

  describe('Redundancy Detection', () => {
    it('should detect redundant responses', async () => {
      // Setup: Recent message in room is very similar
      // ...

      const isRedundant = await communication['isResponseRedundant'](
        'Hello, how can I help you?',
        'room-001'
      );

      expect(isRedundant).toBe(true);
    });

    it('should allow non-redundant responses', async () => {
      const isRedundant = await communication['isResponseRedundant'](
        'This is a unique response',
        'room-001'
      );

      expect(isRedundant).toBe(false);
    });
  });

  describe('Response Generation', () => {
    it('should generate response with RAG context', async () => {
      mockMemory.recall.mockResolvedValue({
        messages: [
          { text: 'Previous context', senderName: 'User' }
        ]
      });

      const message = {
        content: { text: 'What did we discuss?' },
        roomId: 'room-001',
        // ... other fields
      };

      const decision = {
        shouldRespond: true,
        reason: 'User question',
        confidence: 0.9
      };

      const result = await communication.respond(message, decision);

      expect(result.success).toBe(true);
      expect(mockMemory.recall).toHaveBeenCalledWith('room-001');
      expect(mockMemory.learn).toHaveBeenCalled();
    });

    it('should skip redundant responses', async () => {
      // Mock redundancy detection to return true
      communication['isResponseRedundant'] = vi.fn().mockResolvedValue(true);

      const message = { /* ... */ };
      const decision = { shouldRespond: true, reason: 'test', confidence: 0.9 };

      const result = await communication.respond(message, decision);

      expect(result.success).toBe(false);
      expect(result.error).toContain('redundant');
    });
  });
});
```

**Run after Phase 4 extraction**:
```bash
npx vitest tests/unit/PersonaCommunication.test.ts
```

---

## Phase 5: PersonaExecution Tests

### Unit Test: PersonaExecution
```typescript
// tests/unit/PersonaExecution.test.ts
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { PersonaExecution } from '../modules/execution/PersonaExecution';

describe('PersonaExecution', () => {
  let execution: PersonaExecution;
  let mockGenome: any;
  let mockMemory: any;

  beforeEach(() => {
    mockGenome = {};
    mockMemory = {};

    execution = new PersonaExecution(
      { id: 'persona-001', displayName: 'Test Persona' },
      mockGenome,
      mockMemory,
      mockTrainingAccumulator
    );
  });

  describe('Task Execution', () => {
    it('should execute memory consolidation task', async () => {
      const task = {
        taskId: 'task-001',
        taskType: 'memory-consolidation' as const,
        description: 'Consolidate recent memories',
        priority: 0.5,
        // ... other fields
      };

      const result = await execution.execute(task);

      expect(result.status).toBe('completed');
      expect(result.outcome).toContain('Consolidated');
      expect(result.duration).toBeGreaterThan(0);
    });

    it('should execute skill audit task', async () => {
      const task = {
        taskType: 'skill-audit' as const,
        // ... other fields
      };

      const result = await execution.execute(task);

      expect(result.status).toBe('completed');
      expect(result.outcome).toContain('skill');
    });

    it('should handle unknown task types gracefully', async () => {
      const task = {
        taskType: 'unknown-task' as any,
        // ... other fields
      };

      const result = await execution.execute(task);

      expect(result.status).toBe('failed');
      expect(result.outcome).toContain('Unknown task type');
    });

    it('should catch and report errors', async () => {
      // Mock method to throw error
      execution['executeMemoryConsolidation'] = vi.fn().mockRejectedValue(new Error('Test error'));

      const task = {
        taskType: 'memory-consolidation' as const,
        // ... other fields
      };

      const result = await execution.execute(task);

      expect(result.status).toBe('failed');
      expect(result.outcome).toContain('Test error');
    });
  });
});
```

**Run after Phase 5 extraction**:
```bash
npx vitest tests/unit/PersonaExecution.test.ts
```

---

## Phase 6: Final Integration Tests

### Integration Test: Full Cognitive Flow
```typescript
// tests/integration/CognitiveArchitecture.integration.test.ts
import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { PersonaUser } from '../PersonaUser';
import { DataDaemon } from '../../../daemons/data-daemon/shared/DataDaemon';

describe('Cognitive Architecture Integration', () => {
  let personaUser: PersonaUser;
  let testRoomId: string;

  beforeAll(async () => {
    await DataDaemon.initialize();

    // Create test room
    const room = await DataDaemon.store('rooms', {
      uniqueId: 'test-cognitive-room',
      displayName: 'Cognitive Test Room'
    });
    testRoomId = room.id;

    // Create persona with cognitive architecture
    personaUser = new PersonaUser({
      uniqueId: '@cognitive-test',
      displayName: 'Cognitive Test Persona',
      modelConfig: { provider: 'ollama', model: 'llama3.2' }
    });
    await personaUser.initialize();
  });

  afterAll(async () => {
    await personaUser?.shutdown();
  });

  it('should complete full cognitive cycle: Message → Cognition → Communication → Memory', async () => {
    const message = {
      id: 'msg-cognitive-001',
      roomId: testRoomId,
      senderId: 'user-001',
      senderName: 'Test User',
      content: { text: '@cognitive-test What is TypeScript?' },
      timestamp: new Date().toISOString(),
      // ... other fields
    };

    // Enqueue message
    await personaUser['handleChatMessage'](message);

    // Wait for CNS to process (autonomous loop)
    await new Promise(resolve => setTimeout(resolve, 5000));

    // Verify cognitive modules worked:

    // 1. Cognition evaluated message
    const inboxSize = personaUser['inbox'].getSize();
    expect(inboxSize).toBeGreaterThanOrEqual(0); // Message processed or still in queue

    // 2. Memory should have RAG context
    const memory = personaUser['memory'];
    const context = await memory?.recall(testRoomId);
    expect(context).toBeDefined();

    // 3. Check if response was posted (if AI decided to respond)
    const messages = await DataDaemon.list('chat_messages', {
      filter: { roomId: testRoomId }
    });

    // Should have at least the original message
    expect(messages.items.length).toBeGreaterThanOrEqual(1);
  });

  it('should handle task execution via Execution module', async () => {
    const task = {
      id: 'task-cognitive-001',
      assigneeId: personaUser.id,
      taskType: 'memory-consolidation',
      description: 'Test memory consolidation',
      status: 'pending',
      priority: 0.5,
      // ... other fields
    };

    // Store task in database
    await DataDaemon.store('tasks', task);

    // Wait for autonomous loop to poll and execute
    await new Promise(resolve => setTimeout(resolve, 10000));

    // Verify task was executed
    const updatedTask = await DataDaemon.read('tasks', task.id);
    expect(updatedTask.status).toMatch(/completed|in_progress/);
  });

  it('should maintain state across cognitive operations', async () => {
    const initialState = personaUser['personaState'].getState();

    // Send multiple messages
    for (let i = 0; i < 5; i++) {
      await personaUser['handleChatMessage']({
        id: `msg-state-${i}`,
        roomId: testRoomId,
        senderId: 'user-001',
        senderName: 'Test User',
        content: { text: `Message ${i}` },
        timestamp: new Date().toISOString(),
        // ... other fields
      });
    }

    // Wait for processing
    await new Promise(resolve => setTimeout(resolve, 3000));

    const finalState = personaUser['personaState'].getState();

    // Energy should have decreased (persona did work)
    expect(finalState.energy).toBeLessThanOrEqual(initialState.energy);

    // Mood may have changed based on inbox load
    expect(finalState.mood).toBeDefined();
  });
});
```

### Performance Test
```typescript
// tests/integration/Performance.test.ts
import { describe, it, expect } from 'vitest';
import { PersonaUser } from '../PersonaUser';

describe('Performance (Cognitive Architecture)', () => {
  it('should initialize in <2 seconds', async () => {
    const start = Date.now();

    const persona = new PersonaUser({
      uniqueId: '@perf-test',
      displayName: 'Performance Test',
      modelConfig: { provider: 'ollama', model: 'llama3.2' }
    });
    await persona.initialize();

    const duration = Date.now() - start;
    expect(duration).toBeLessThan(2000);

    await persona.shutdown();
  });

  it('should process 10 messages in <5 seconds', async () => {
    const persona = new PersonaUser({ /* ... */ });
    await persona.initialize();

    const start = Date.now();

    for (let i = 0; i < 10; i++) {
      await persona['handleChatMessage']({ /* ... */ });
    }

    const duration = Date.now() - start;
    expect(duration).toBeLessThan(5000);

    await persona.shutdown();
  });

  it('should not leak memory over 100 operations', async () => {
    const persona = new PersonaUser({ /* ... */ });
    await persona.initialize();

    const initialMemory = process.memoryUsage().heapUsed;

    for (let i = 0; i < 100; i++) {
      await persona['handleChatMessage']({ /* ... */ });

      if (i % 10 === 0) {
        global.gc?.(); // Force garbage collection if available
      }
    }

    const finalMemory = process.memoryUsage().heapUsed;
    const growth = finalMemory - initialMemory;

    // Memory growth should be <50MB for 100 operations
    expect(growth).toBeLessThan(50 * 1024 * 1024);

    await persona.shutdown();
  });
});
```

---

## Test Execution Strategy

### During Refactoring:
```bash
# Run unit tests for current module
npx vitest tests/unit/PersonaMemory.test.ts --run

# Run validation tests
npx vitest tests/validation/ --run

# Run integration tests
npx vitest tests/integration/ --run

# Run all tests
npx vitest --run

# Watch mode during development
npx vitest tests/unit/PersonaMemory.test.ts
```

### After Each Phase:
```bash
# 1. Run new module's unit tests
npx vitest tests/unit/PersonaMemory.test.ts --run

# 2. Run baseline integration tests (ensure nothing broke)
npx vitest tests/integration/PersonaUserLifecycle.test.ts --run
npx vitest tests/integration/ChatResponseFlow.test.ts --run

# 3. If all pass, commit
git add tests/
git commit -m "test: add PersonaMemory tests"
```

### Final Verification (Phase 6):
```bash
# Run full test suite
npx vitest --run

# Run with coverage
npx vitest --coverage

# Expected: >80% coverage for cognitive modules
```

---

## Success Criteria

### Phase 0 (Baseline):
- ✅ PersonaUserLifecycle tests pass
- ✅ ChatResponseFlow tests pass
- ✅ PriorityCalculation validation tests pass

### After Each Module Extraction:
- ✅ New module's unit tests pass
- ✅ Validation tests pass (if applicable)
- ✅ Baseline integration tests still pass
- ✅ No performance regressions (<10% slower)

### Phase 6 (Final):
- ✅ All unit tests pass
- ✅ All validation tests pass
- ✅ All integration tests pass
- ✅ Performance tests pass
- ✅ Test coverage >80% for cognitive modules

---

## Notes

**Mocking Strategy**:
- Use vitest's `vi.fn()` for mocks
- Mock external dependencies (DataDaemon, AIProviderDaemon) for unit tests
- Use real dependencies for integration tests

**Test Data**:
- Create test fixtures in `tests/fixtures/`
- Use consistent IDs (msg-001, user-001, room-001)
- Clean up test data in `afterAll()` hooks

**Async Testing**:
- Use `async/await` consistently
- Set appropriate timeouts for integration tests (10s+)
- Use `waitFor` patterns for eventual consistency

**Test Isolation**:
- Each test should be independent
- Use `beforeEach` to reset state
- Don't rely on test execution order
