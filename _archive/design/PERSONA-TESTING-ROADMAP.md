# PERSONA TESTING ROADMAP

**Date**: 2025-10-08
**Purpose**: Elegant, type-safe integration tests for persona evolution milestones
**Pattern**: Follow `tests/integration/crud-db-widget.test.ts` quality standards

---

## Testing Philosophy

**From CLAUDE.md:**
> "Extremely high professional grade rust like obsession using quality typescript patterns"

**Key Principles:**
1. **Type-safe generics** - `Commands.execute<TParams, TResult>()` with explicit types
2. **Real commands** - Use actual `genome/*`, `lora/*`, `rag/*` commands, not raw `data/create`
3. **Database verification** - Every operation persists correctly
4. **Integration tests** - Test complete flows, not isolated units
5. **Clean up** - `afterEach()` removes all test data
6. **No mocks** - Test against real running system

---

## Milestone 1: RAG-Enhanced Persona Responses

**Goal**: Personas respond to chat naturally using RAG context (conversation history, room members, persona identity)

### Test Suite: `tests/integration/persona-rag.test.ts`

#### Test 1: ChatRAGBuilder builds context correctly
```typescript
test('ChatRAGBuilder - should build complete RAG context for persona', async () => {
  // Setup: Create test room with members and message history
  const testRoom = await createTestRoom('rag-test-room', ['Joel', 'TestPersona']);
  const testPersona = await createTestPersona('TestPersona', {
    displayName: 'Test AI',
    bio: 'A helpful AI assistant for testing'
  });

  // Seed conversation history (10 messages)
  const messages = await seedConversationHistory(testRoom.id, [
    { sender: 'Joel', text: 'Hello everyone!' },
    { sender: 'TestPersona', text: 'Hi Joel, how can I help?' },
    { sender: 'Joel', text: 'Can you explain RAG?' },
    // ... 7 more messages
  ]);

  // Execute: Build RAG context
  const ragBuilder = new ChatRAGBuilder();
  const context = await ragBuilder.buildContext(
    testRoom.id,
    testPersona.id,
    { maxMessages: 10 }
  );

  // Verify: Context structure
  expect(context.domain).toBe('chat');
  expect(context.contextId).toBe(testRoom.id);
  expect(context.personaId).toBe(testPersona.id);

  // Verify: Persona identity loaded
  expect(context.identity.name).toBe('Test AI');
  expect(context.identity.bio).toBe('A helpful AI assistant for testing');
  expect(context.identity.systemPrompt).toContain('Test AI');
  expect(context.identity.systemPrompt).toContain('Joel'); // Room members

  // Verify: Conversation history (chronological order, oldest first)
  expect(context.conversationHistory).toHaveLength(10);
  expect(context.conversationHistory[0].content).toBe('Hello everyone!');
  expect(context.conversationHistory[0].role).toBe('user');
  expect(context.conversationHistory[0].name).toBe('Joel');

  expect(context.conversationHistory[1].content).toBe('Hi Joel, how can I help?');
  expect(context.conversationHistory[1].role).toBe('assistant'); // Persona's own message

  // Verify: Metadata
  expect(context.metadata.messageCount).toBe(10);
  expect(context.metadata.artifactCount).toBe(0); // No images in test
  expect(context.metadata.builtAt).toBeInstanceOf(Date);
});
```

#### Test 2: RAG context includes room member names
```typescript
test('ChatRAGBuilder - system prompt includes all room members', async () => {
  const testRoom = await createTestRoom('member-test-room', [
    'Joel',
    'Alice',
    'Bob',
    'TestPersona'
  ]);
  const testPersona = await createTestPersona('TestPersona');

  const ragBuilder = new ChatRAGBuilder();
  const context = await ragBuilder.buildContext(testRoom.id, testPersona.id);

  // Verify: System prompt mentions all members
  expect(context.identity.systemPrompt).toContain('Joel');
  expect(context.identity.systemPrompt).toContain('Alice');
  expect(context.identity.systemPrompt).toContain('Bob');

  // Verify: Instructions to NOT invent participants
  expect(context.identity.systemPrompt).toContain('DO NOT invent participants');
  expect(context.identity.systemPrompt).toContain('ONLY these people exist');
});
```

#### Test 3: RAG context respects message limit
```typescript
test('ChatRAGBuilder - respects maxMessages limit', async () => {
  const testRoom = await createTestRoom('limit-test-room');
  const testPersona = await createTestPersona('TestPersona');

  // Seed 50 messages
  await seedConversationHistory(testRoom.id, Array(50).fill(null).map((_, i) => ({
    sender: i % 2 === 0 ? 'Joel' : 'TestPersona',
    text: `Message ${i + 1}`
  })));

  const ragBuilder = new ChatRAGBuilder();

  // Test with limit of 20
  const context20 = await ragBuilder.buildContext(testRoom.id, testPersona.id, {
    maxMessages: 20
  });
  expect(context20.conversationHistory).toHaveLength(20);

  // Verify: Most recent 20 messages (chronologically oldest-first after filtering)
  expect(context20.conversationHistory[19].content).toContain('Message 50'); // Most recent
  expect(context20.conversationHistory[0].content).toContain('Message 31'); // 20th from end
});
```

#### Test 4: RAG context converts timestamps correctly
```typescript
test('ChatRAGBuilder - converts timestamps to number (milliseconds)', async () => {
  const testRoom = await createTestRoom('timestamp-test');
  const testPersona = await createTestPersona('TestPersona');

  // Seed messages with explicit timestamps
  const now = Date.now();
  await seedConversationHistory(testRoom.id, [
    { sender: 'Joel', text: 'Test 1', timestamp: now - 10000 },
    { sender: 'Joel', text: 'Test 2', timestamp: now - 5000 },
    { sender: 'Joel', text: 'Test 3', timestamp: now }
  ]);

  const ragBuilder = new ChatRAGBuilder();
  const context = await ragBuilder.buildContext(testRoom.id, testPersona.id);

  // Verify: Timestamps are numbers in milliseconds
  expect(typeof context.conversationHistory[0].timestamp).toBe('number');
  expect(context.conversationHistory[0].timestamp).toBeGreaterThan(now - 11000);
  expect(context.conversationHistory[0].timestamp).toBeLessThan(now);
});
```

#### Test 5: PersonaUser uses RAG for natural responses
```typescript
test('PersonaUser - generates contextually-aware response using RAG', async () => {
  // Setup: Room with conversation about specific topic
  const testRoom = await createTestRoom('context-aware-test');
  const testPersona = await createTestPersona('ContextAwareAI', {
    displayName: 'Context AI',
    bio: 'An AI that understands context'
  });

  // Seed conversation about TypeScript
  await seedConversationHistory(testRoom.id, [
    { sender: 'Joel', text: 'I love TypeScript for its strict typing' },
    { sender: 'Alice', text: 'Yeah, the type safety catches so many bugs' },
    { sender: 'Bob', text: 'But the generics can be confusing sometimes' }
  ]);

  // Execute: Send new message mentioning "types"
  const newMessage = await sendMessage(testRoom.id, 'Joel', '@ContextAwareAI What are your thoughts on types?');

  // Trigger persona response (simulates Postmaster routing decision)
  const response = await executePersonaResponse(testPersona.id, testRoom.id, newMessage.id);

  // Verify: Response is contextually aware
  expect(response.success).toBe(true);
  expect(response.message).toBeDefined();

  // Response should reference the conversation context
  const responseText = response.message!.content.text.toLowerCase();

  // Should mention TypeScript or typing (from conversation context)
  const mentionsContext =
    responseText.includes('typescript') ||
    responseText.includes('typing') ||
    responseText.includes('type safety') ||
    responseText.includes('strict');

  expect(mentionsContext).toBe(true);

  // Response should NOT include persona name prefix (RAG system prompt instructs this)
  expect(response.message!.content.text).not.toMatch(/^Context AI:/);
  expect(response.message!.content.text).not.toMatch(/^Assistant:/);
});
```

#### Test 6: RAG context excludes future messages (race condition protection)
```typescript
test('ChatRAGBuilder - respects triggeringTimestamp cutoff', async () => {
  const testRoom = await createTestRoom('cutoff-test');
  const testPersona = await createTestPersona('TestPersona');

  // Seed messages with specific timestamps
  const baseTime = Date.now();
  await seedConversationHistory(testRoom.id, [
    { sender: 'Joel', text: 'Message 1', timestamp: baseTime },
    { sender: 'Joel', text: 'Message 2', timestamp: baseTime + 1000 },
    { sender: 'Joel', text: 'Message 3', timestamp: baseTime + 2000 },
    { sender: 'Joel', text: 'Message 4', timestamp: baseTime + 3000 },
  ]);

  // Build context with cutoff at baseTime + 2000
  // Should only include messages 1, 2, 3 (exclude message 4)
  const ragBuilder = new ChatRAGBuilder();
  const context = await ragBuilder.buildContext(testRoom.id, testPersona.id, {
    triggeringTimestamp: baseTime + 2000
  });

  expect(context.conversationHistory).toHaveLength(3);
  expect(context.conversationHistory[2].content).toBe('Message 3');

  // Should NOT include Message 4 (sent after trigger)
  const hasMessage4 = context.conversationHistory.some(m => m.content === 'Message 4');
  expect(hasMessage4).toBe(false);
});
```

### Helper Functions (Test Utilities)
```typescript
// tests/integration/helpers/persona-test-helpers.ts

import { randomUUID } from 'crypto';
import { Commands } from '../../../system/core/shared/Commands';
import type { RoomEntity } from '../../../system/data/entities/RoomEntity';
import type { UserEntity } from '../../../system/data/entities/UserEntity';
import type { ChatMessageEntity } from '../../../system/data/entities/ChatMessageEntity';

/**
 * Create test room with members
 */
export async function createTestRoom(
  name: string,
  memberNames: string[] = []
): Promise<RoomEntity> {
  const roomId = randomUUID();

  const result = await Commands.execute('data/create', {
    collection: 'rooms',
    data: {
      id: roomId,
      name,
      description: `Test room: ${name}`,
      members: memberNames.map(name => ({ userId: name, role: 'member' })),
      createdAt: new Date()
    }
  });

  if (!result.success) {
    throw new Error(`Failed to create test room: ${result.error}`);
  }

  return result.data as RoomEntity;
}

/**
 * Create test persona
 */
export async function createTestPersona(
  username: string,
  options: {
    displayName?: string;
    bio?: string;
    autoResponds?: boolean;
  } = {}
): Promise<UserEntity> {
  const personaId = randomUUID();

  const result = await Commands.execute('data/create', {
    collection: 'users',
    data: {
      id: personaId,
      username,
      displayName: options.displayName ?? username,
      type: 'persona',
      profile: {
        bio: options.bio ?? 'A test AI persona'
      },
      capabilities: {
        autoResponds: options.autoResponds ?? true
      },
      createdAt: new Date()
    }
  });

  if (!result.success) {
    throw new Error(`Failed to create test persona: ${result.error}`);
  }

  return result.data as UserEntity;
}

/**
 * Seed conversation history
 */
export async function seedConversationHistory(
  roomId: string,
  messages: Array<{
    sender: string;
    text: string;
    timestamp?: number;
  }>
): Promise<ChatMessageEntity[]> {
  const results: ChatMessageEntity[] = [];

  for (const [index, msg] of messages.entries()) {
    const messageId = randomUUID();
    const timestamp = msg.timestamp ?? Date.now() + index * 1000;

    const result = await Commands.execute('data/create', {
      collection: 'chat_messages',
      data: {
        id: messageId,
        roomId,
        senderId: msg.sender,
        senderName: msg.sender,
        content: { text: msg.text },
        timestamp: new Date(timestamp)
      }
    });

    if (!result.success) {
      throw new Error(`Failed to seed message: ${result.error}`);
    }

    results.push(result.data as ChatMessageEntity);
  }

  return results;
}

/**
 * Send message to room (via chat/send command)
 */
export async function sendMessage(
  roomId: string,
  sender: string,
  text: string
): Promise<ChatMessageEntity> {
  const result = await Commands.execute('chat/send', {
    roomId,
    senderId: sender,
    content: { text }
  });

  if (!result.success) {
    throw new Error(`Failed to send message: ${result.error}`);
  }

  return result.data as ChatMessageEntity;
}

/**
 * Execute persona response (simulates Postmaster decision + PersonaUser.respond())
 */
export async function executePersonaResponse(
  personaId: string,
  roomId: string,
  triggeringMessageId: string
): Promise<{
  success: boolean;
  message?: ChatMessageEntity;
  error?: string;
}> {
  // This will eventually call persona/respond command
  // For now, stub implementation that tests RAG integration
  const result = await Commands.execute('persona/respond', {
    personaId,
    contextId: roomId,
    triggeringMessageId
  });

  return result;
}
```

---

## Milestone 2: LoRA-Based Persona Genomes

**Goal**: Personas can be created with LoRA genome layers, loaded dynamically, and trained via Academy

### Test Suite: `tests/integration/persona-genome.test.ts`

#### Test 1: Create genome layer
```typescript
test('genome/layer/create - should create LoRA layer entity', async () => {
  const result = await Commands.execute<
    GenomeLayerCreateParams,
    GenomeLayerCreateResult
  >(GENOME_COMMANDS.LAYER.CREATE, {
    name: 'test-tone-layer',
    traitType: 'tone_and_voice',
    modelPath: '/test/layers/tone.safetensors',
    sizeMB: 25,
    rank: 16,
    embedding: Array(768).fill(0).map(() => Math.random()), // Mock embedding
    source: 'trained'
  });

  expect(result.success).toBe(true);
  expect(result.layerId).toBeDefined();

  // Verify persistence
  const layers = await Commands.execute('data/read', {
    collection: 'genome_layers',
    id: result.layerId!
  });

  expect(layers.success).toBe(true);
  expect(layers.data.name).toBe('test-tone-layer');
  expect(layers.data.traitType).toBe('tone_and_voice');
  expect(layers.data.rank).toBe(16);
});
```

#### Test 2: Search layers by similarity
```typescript
test('genome/layer/search - should find similar layers via cosine similarity', async () => {
  // Create target layer
  const targetEmbedding = Array(768).fill(0).map(() => Math.random());

  // Create similar layer (high similarity)
  const similarEmbedding = targetEmbedding.map(v => v + Math.random() * 0.1);
  await createTestGenomeLayer('similar-layer', 'tone_and_voice', similarEmbedding);

  // Create dissimilar layer
  const dissimilarEmbedding = Array(768).fill(0).map(() => Math.random());
  await createTestGenomeLayer('dissimilar-layer', 'tone_and_voice', dissimilarEmbedding);

  // Search for layers similar to target
  const result = await Commands.execute<
    GenomeLayerSearchParams,
    GenomeLayerSearchResult
  >(GENOME_COMMANDS.LAYER.SEARCH, {
    targetEmbedding,
    traitType: 'tone_and_voice',
    minSimilarity: 0.70,
    limit: 10
  });

  expect(result.success).toBe(true);
  expect(result.layers).toBeDefined();
  expect(result.layers!.length).toBeGreaterThan(0);

  // Verify: Similar layer is found
  const foundSimilar = result.layers!.find(l => l.name === 'similar-layer');
  expect(foundSimilar).toBeDefined();
  expect(foundSimilar!.similarity).toBeGreaterThan(0.90); // High similarity

  // Verify: Dissimilar layer is NOT in results (or has low similarity)
  const foundDissimilar = result.layers!.find(l => l.name === 'dissimilar-layer');
  if (foundDissimilar) {
    expect(foundDissimilar.similarity).toBeLessThan(0.70); // Below threshold
  }
});
```

#### Test 3: Assemble persona genome
```typescript
test('genome/assemble - should create genome from layer references', async () => {
  // Create test layers
  const toneLayer = await createTestGenomeLayer('friendly-tone', 'tone_and_voice');
  const ethicsLayer = await createTestGenomeLayer('balanced-ethics', 'ethical_reasoning');
  const domainLayer = await createTestGenomeLayer('typescript-expert', 'domain_expertise');

  // Create test persona
  const persona = await createTestPersona('GenomeTestAI');

  // Assemble genome
  const result = await Commands.execute<
    GenomeAssembleParams,
    GenomeAssembleResult
  >(GENOME_COMMANDS.ASSEMBLE, {
    personaId: persona.id,
    baseModel: 'llama-3.1-8B',
    layers: [
      { layerId: toneLayer.id, weight: 1.0, enabled: true },
      { layerId: ethicsLayer.id, weight: 0.8, enabled: true },
      { layerId: domainLayer.id, weight: 1.0, enabled: true }
    ]
  });

  expect(result.success).toBe(true);
  expect(result.genomeId).toBeDefined();

  // Verify: Genome persisted with correct structure
  const genome = await Commands.execute('data/read', {
    collection: 'genomes',
    id: result.genomeId!
  });

  expect(genome.success).toBe(true);
  expect(genome.data.personaId).toBe(persona.id);
  expect(genome.data.baseModel).toBe('llama-3.1-8B');
  expect(genome.data.layers).toHaveLength(3);

  // Verify: Composite embedding calculated
  expect(genome.data.compositeEmbedding).toHaveLength(768);
  expect(genome.data.compositeEmbedding.every((v: number) => typeof v === 'number')).toBe(true);
});
```

#### Test 4: Mount LoRA layers for inference
```typescript
test('lora/load - should load LoRA adapter into model process', async () => {
  // Create genome with layers
  const genome = await createTestGenome('test-persona', {
    baseModel: 'llama-3.1-8B',
    layers: [
      { layerId: 'layer-1', weight: 1.0 },
      { layerId: 'layer-2', weight: 0.8 }
    ]
  });

  // Load LoRA adapters
  const result = await Commands.execute<
    LoRALoadParams,
    LoRALoadResult
  >(LORA_COMMANDS.LOAD, {
    genomeId: genome.id,
    priority: 'standard'
  });

  expect(result.success).toBe(true);
  expect(result.loadedLayers).toHaveLength(2);
  expect(result.cacheSize).toBeGreaterThan(0);

  // Verify: Layers are in cache
  const status = await Commands.execute<LoRAStatusParams, LoRAStatusResult>(
    LORA_COMMANDS.STATUS,
    {}
  );

  expect(status.activeLayers).toContain('layer-1');
  expect(status.activeLayers).toContain('layer-2');
});
```

#### Test 5: LoRA cache eviction (LRU)
```typescript
test('lora/load - should evict LRU layers when cache full', async () => {
  // Configure small cache (5 layers max)
  const cacheConfig = { maxLayers: 5 };

  // Create 7 genomes (will overflow cache)
  const genomes = await Promise.all(
    Array(7).fill(null).map((_, i) =>
      createTestGenome(`persona-${i}`, {
        baseModel: 'llama-3.1-8B',
        layers: [{ layerId: `layer-${i}`, weight: 1.0 }]
      })
    )
  );

  // Load all 7 (should trigger evictions)
  for (const genome of genomes) {
    await Commands.execute(LORA_COMMANDS.LOAD, {
      genomeId: genome.id,
      priority: 'standard'
    });
  }

  // Verify: Only 5 layers in cache (most recent)
  const status = await Commands.execute(LORA_COMMANDS.STATUS, {});
  expect(status.activeLayers).toHaveLength(5);

  // Verify: Most recently loaded layers are cached
  expect(status.activeLayers).toContain('layer-6');
  expect(status.activeLayers).toContain('layer-5');
  expect(status.activeLayers).toContain('layer-4');

  // Verify: Oldest layers evicted
  expect(status.activeLayers).not.toContain('layer-0');
  expect(status.activeLayers).not.toContain('layer-1');
});
```

#### Test 6: End-to-end genome-based inference
```typescript
test('PersonaUser with genome - should use LoRA layers for response', async () => {
  // Create specialized persona genome (TypeScript expert)
  const expertLayer = await createTestGenomeLayer('typescript-expert', 'domain_expertise', {
    // Embedding trained on TypeScript docs
    embedding: mockTypeScriptExpertEmbedding()
  });

  const persona = await createTestPersona('TypeScriptExpert');
  const genome = await createTestGenome(persona.id, {
    baseModel: 'llama-3.1-8B',
    layers: [{ layerId: expertLayer.id, weight: 1.0 }]
  });

  // Mount genome
  await Commands.execute(GENOME_COMMANDS.MOUNT, {
    personaId: persona.id,
    genomeId: genome.id
  });

  // Create test room and conversation
  const room = await createTestRoom('typescript-help');
  await seedConversationHistory(room.id, [
    { sender: 'Joel', text: 'I need help with TypeScript generics' }
  ]);

  // Execute persona response
  const response = await executePersonaResponse(persona.id, room.id, 'latest');

  expect(response.success).toBe(true);
  expect(response.message).toBeDefined();

  // Verify: Response demonstrates TypeScript expertise
  const responseText = response.message!.content.text.toLowerCase();

  // Should use technical terminology
  const usesTechnicalTerms =
    responseText.includes('type parameter') ||
    responseText.includes('constraint') ||
    responseText.includes('extends') ||
    responseText.includes('generic');

  expect(usesTechnicalTerms).toBe(true);

  // Verify: Genome was actually loaded (check logs/metrics)
  const genomeStatus = await Commands.execute('genome/status', {
    personaId: persona.id
  });

  expect(genomeStatus.mountedGenomeId).toBe(genome.id);
  expect(genomeStatus.loadedLayers).toContain(expertLayer.id);
});
```

---

## Test Execution Plan

### Phase 1: RAG Integration (Current Sprint)
1. ✅ Create test helpers (`persona-test-helpers.ts`)
2. ✅ Write RAG context tests (6 tests)
3. ⚠️ Implement missing `persona/respond` command
4. ⚠️ Wire up RAG → AIProvider integration
5. ✅ All tests passing

### Phase 2: Genome Layer CRUD (Next Sprint)
1. ⚠️ Implement `genome/layer/create` command
2. ⚠️ Implement `genome/layer/search` (cosine similarity)
3. ⚠️ Implement `genome/assemble` command
4. ✅ Write genome entity tests (3 tests)
5. ✅ All tests passing

### Phase 3: LoRA Runtime (Sprint 3)
1. ⚠️ Implement `lora/load` command (adapter mounting)
2. ⚠️ Implement `lora/unload` command (cache eviction)
3. ⚠️ Implement `lora/status` command (cache inspection)
4. ⚠️ Implement LRU cache logic
5. ✅ Write LoRA runtime tests (3 tests)
6. ✅ All tests passing

### Phase 4: End-to-End Integration (Sprint 4)
1. ⚠️ Wire up PersonaUser → Genome → LoRA → AIProvider
2. ⚠️ Implement process-per-persona spawning
3. ✅ Write end-to-end test (persona with genome responds)
4. ✅ All 15+ tests passing
5. 🎉 **MILESTONE ACHIEVED**: Functional LoRA-based personas

---

## Test Quality Standards

### Type Safety
```typescript
// ✅ CORRECT: Explicit generic types
const result = await Commands.execute<
  GenomeLayerCreateParams,
  GenomeLayerCreateResult
>(GENOME_COMMANDS.LAYER.CREATE, params);

// ❌ WRONG: Loose typing
const result = await Commands.execute('genome/layer/create', params) as any;
```

### Command Constants
```typescript
// ✅ CORRECT: Use constants (no magic strings)
import { GENOME_COMMANDS } from '../../../system/genome/shared/GenomeCommandConstants';
await Commands.execute(GENOME_COMMANDS.LAYER.CREATE, params);

// ❌ WRONG: Magic strings
await Commands.execute('genome/layer/create', params);
```

### Database Verification
```typescript
// ✅ CORRECT: Verify persistence
const createResult = await Commands.execute(GENOME_COMMANDS.LAYER.CREATE, params);
expect(createResult.success).toBe(true);

// Re-read from database to verify
const readResult = await Commands.execute('data/read', {
  collection: 'genome_layers',
  id: createResult.layerId
});
expect(readResult.success).toBe(true);
expect(readResult.data.name).toBe('test-layer');
```

### Cleanup
```typescript
afterEach(async () => {
  // Clean up ALL test data
  await cleanupTestRooms();
  await cleanupTestPersonas();
  await cleanupTestGenomes();
  await cleanupTestMessages();
});
```

### Real Commands Only
```typescript
// ✅ CORRECT: Use actual user-facing commands
await Commands.execute('genome/layer/create', params);
await Commands.execute('lora/load', params);

// ❌ WRONG: Direct database access (bypasses business logic)
await DataDaemon.create('genome_layers', data);
```

---

## Success Criteria

### Milestone 1: RAG-Enhanced Personas
- ✅ 6 RAG integration tests passing
- ✅ Personas use conversation history for context
- ✅ Personas understand room members
- ✅ Responses are natural (no name prefixes, contextually aware)

### Milestone 2: LoRA-Based Personas
- ✅ 9 genome/LoRA tests passing
- ✅ Genome layers can be created and searched
- ✅ Personas can be assembled with LoRA stacks
- ✅ LoRA adapters load/unload dynamically
- ✅ LRU cache eviction works
- ✅ End-to-end: Persona with genome responds differently than base model

---

## Next Actions

1. **Create test helper file** - `tests/integration/helpers/persona-test-helpers.ts`
2. **Write first RAG test** - `tests/integration/persona-rag.test.ts` (Test 1)
3. **Run test** - Identify missing dependencies
4. **Implement minimum viable** - Get first test passing
5. **Iterate** - Add tests 2-6 incrementally
6. **Move to Milestone 2** - Once all RAG tests pass

**Philosophy**: Test-driven development with elegant, type-safe integration tests that validate our architectural conceptions.
