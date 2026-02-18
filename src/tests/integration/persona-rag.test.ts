/**
 * Persona RAG Integration Tests
 *
 * Tests for RAG-enhanced persona responses (Milestone 1)
 * Validates that personas use conversation history, room context, and identity
 * for natural, contextually-aware responses.
 */

import { describe, test, expect, afterEach } from 'vitest';
import { ChatRAGBuilder } from '../../system/rag/builders/ChatRAGBuilder';
import {
  createTestRoom,
  createTestPersona,
  seedConversationHistory,
  sendMessage,
  executePersonaResponse,
  cleanupAllTestData
} from './helpers/persona-test-helpers';

describe('ChatRAGBuilder Integration Tests', () => {
  afterEach(async () => {
    await cleanupAllTestData();
  });

  test('should build complete RAG context for persona', async () => {
    // Setup: Create test room with members and message history
    const testRoom = await createTestRoom('rag-test-room', ['Joel', 'TestPersona']);
    const testPersona = await createTestPersona('TestPersona', {
      displayName: 'Test AI',
      bio: 'A helpful AI assistant for testing'
    });

    // Seed conversation history (10 messages)
    await seedConversationHistory(testRoom.id, [
      { sender: 'Joel', text: 'Hello everyone!' },
      { sender: 'TestPersona', text: 'Hi Joel, how can I help?' },
      { sender: 'Joel', text: 'Can you explain RAG?' },
      { sender: 'TestPersona', text: 'RAG stands for Retrieval-Augmented Generation...' },
      { sender: 'Joel', text: 'How does it improve AI responses?' },
      { sender: 'TestPersona', text: 'It provides relevant context from conversations...' },
      { sender: 'Joel', text: 'That makes sense!' },
      { sender: 'TestPersona', text: 'Glad I could help clarify!' },
      { sender: 'Joel', text: 'Thanks for the explanation' },
      { sender: 'TestPersona', text: 'You\'re welcome!' }
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

  test('system prompt should include all room members', async () => {
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

  test('should respect maxMessages limit', async () => {
    const testRoom = await createTestRoom('limit-test-room');
    const testPersona = await createTestPersona('TestPersona');

    // Seed 50 messages
    await seedConversationHistory(
      testRoom.id,
      Array(50).fill(null).map((_, i) => ({
        sender: i % 2 === 0 ? 'Joel' : 'TestPersona',
        text: `Message ${i + 1}`
      }))
    );

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

  test('should convert timestamps to number (milliseconds)', async () => {
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
    expect(context.conversationHistory[0].timestamp).toBeLessThan(now + 1000);
  });

  test('should handle empty room gracefully', async () => {
    const testRoom = await createTestRoom('empty-room');
    const testPersona = await createTestPersona('TestPersona');

    // Don't seed any messages

    const ragBuilder = new ChatRAGBuilder();
    const context = await ragBuilder.buildContext(testRoom.id, testPersona.id);

    // Verify: Empty conversation history but valid context
    expect(context.conversationHistory).toHaveLength(0);
    expect(context.metadata.messageCount).toBe(0);
    expect(context.identity.name).toBe('TestPersona'); // Still has identity
  });

  test('should handle persona not in room members', async () => {
    // Create room without persona as member
    const testRoom = await createTestRoom('non-member-room', ['Joel', 'Alice']);
    const testPersona = await createTestPersona('OutsidePersona');

    // Seed some conversation
    await seedConversationHistory(testRoom.id, [
      { sender: 'Joel', text: 'Hello Alice!' },
      { sender: 'Alice', text: 'Hi Joel!' }
    ]);

    const ragBuilder = new ChatRAGBuilder();
    const context = await ragBuilder.buildContext(testRoom.id, testPersona.id);

    // Verify: Can still build context (persona might be invited later)
    expect(context.conversationHistory).toHaveLength(2);
    expect(context.identity.name).toBe('OutsidePersona');

    // Verify: System prompt mentions actual members
    expect(context.identity.systemPrompt).toContain('Joel');
    expect(context.identity.systemPrompt).toContain('Alice');
  });
});

describe('PersonaUser RAG Response Tests', () => {
  afterEach(async () => {
    await cleanupAllTestData();
  });

  test.skip('should generate contextually-aware response using RAG', async () => {
    // SKIP: Requires persona/respond command implementation
    // TODO: Implement persona/respond command, then unskip

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

  test.skip('should respect triggeringTimestamp cutoff', async () => {
    // SKIP: Requires RAGBuildOptions.triggeringTimestamp implementation
    // TODO: Implement timestamp cutoff logic, then unskip

    const testRoom = await createTestRoom('cutoff-test');
    const testPersona = await createTestPersona('TestPersona');

    // Seed messages with specific timestamps
    const baseTime = Date.now();
    await seedConversationHistory(testRoom.id, [
      { sender: 'Joel', text: 'Message 1', timestamp: baseTime },
      { sender: 'Joel', text: 'Message 2', timestamp: baseTime + 1000 },
      { sender: 'Joel', text: 'Message 3', timestamp: baseTime + 2000 },
      { sender: 'Joel', text: 'Message 4', timestamp: baseTime + 3000 }
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
});
