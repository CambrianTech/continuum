/**
 * RAG Completeness Test
 *
 * Verifies that event coalescing does NOT cause data loss in RAG context.
 * RAG context should always contain ALL messages from database, regardless
 * of how many events were coalesced.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { ChatRAGBuilder } from '../../system/rag/builders/ChatRAGBuilder';
import { DataDaemon } from '../../daemons/data-daemon/shared/DataDaemon';
import { ChatMessageEntity } from '../../system/data/entities/ChatMessageEntity';
import { RoomEntity } from '../../system/data/entities/RoomEntity';
import { COLLECTIONS } from '../../system/data/config/DatabaseConfig';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';

describe('RAG Completeness with Event Coalescing', () => {
  let testRoomId: UUID;
  let testPersonaId: UUID;
  const TEST_MESSAGE_COUNT = 14;

  beforeAll(async () => {
    // Create test room
    const room = new RoomEntity();
    room.uniqueId = 'rag-test-room';
    room.name = 'rag-test-room';
    room.displayName = 'RAG Test Room';
    room.description = 'Test room for RAG completeness';
    room.type = 'public';
    room.status = 'active';
    room.ownerId = 'test-owner-id' as UUID;
    room.recipeId = 'general-chat';
    room.privacy = {
      isPublic: true,
      requiresInvite: false,
      allowGuestAccess: true,
      searchable: true
    };
    room.settings = {
      allowThreads: true,
      allowReactions: true,
      allowFileSharing: true,
      messageRetentionDays: 365,
      slowMode: 0
    };
    room.members = [];
    room.tags = ['test'];

    const roomResult = await DataDaemon.create(RoomEntity.collection, room);
    if (!roomResult.success || !roomResult.data) {
      throw new Error('Failed to create test room');
    }
    testRoomId = roomResult.data.id as UUID;

    testPersonaId = 'test-persona-id' as UUID;

    // Create 14 messages in database (simulating rapid-fire chat)
    for (let i = 1; i <= TEST_MESSAGE_COUNT; i++) {
      const message = new ChatMessageEntity();
      message.roomId = testRoomId;
      message.senderId = 'test-user-id' as UUID;
      message.senderName = 'Test User';
      message.senderType = 'human';
      message.content = {
        text: `Message ${i} of ${TEST_MESSAGE_COUNT}`,
        attachments: []
      };
      message.status = 'sent';
      message.priority = 'normal';
      message.timestamp = new Date(Date.now() + i * 1000); // 1 second apart
      message.reactions = [];

      const result = await DataDaemon.create(ChatMessageEntity.collection, message);
      if (!result.success) {
        throw new Error(`Failed to create message ${i}`);
      }
    }

    console.log(`✅ Created ${TEST_MESSAGE_COUNT} messages in database for RAG test`);
  });

  afterAll(async () => {
    // Cleanup: delete test messages and room
    await DataDaemon.query({
      collection: COLLECTIONS.CHAT_MESSAGES,
      filter: { roomId: testRoomId }
    }).then(async (result) => {
      if (result.success && result.data) {
        for (const record of result.data) {
          await DataDaemon.delete(COLLECTIONS.CHAT_MESSAGES, record.id);
        }
      }
    });

    await DataDaemon.delete(COLLECTIONS.ROOMS, testRoomId);
  });

  it('should include ALL messages in RAG context regardless of event coalescing', async () => {
    const ragBuilder = new ChatRAGBuilder();

    // Build RAG context (this queries database, not events)
    const ragContext = await ragBuilder.buildContext(
      testRoomId,
      testPersonaId,
      { maxMessages: 20, includeArtifacts: false, includeMemories: false }
    );

    // Verify ALL 14 messages are in RAG context
    expect(ragContext.conversationHistory.length).toBe(TEST_MESSAGE_COUNT);

    // Verify messages are in chronological order (oldest first)
    for (let i = 0; i < TEST_MESSAGE_COUNT; i++) {
      const message = ragContext.conversationHistory[i];
      expect(message.content).toContain(`Message ${i + 1} of ${TEST_MESSAGE_COUNT}`);
      expect(message.name).toBe('Test User');
      expect(message.role).toBe('user');
    }

    console.log(`✅ RAG context contains all ${TEST_MESSAGE_COUNT} messages despite event coalescing`);
  });

  it('should build complete RAG context even if events were coalesced 14→1', async () => {
    // This test simulates the scenario where:
    // - 14 events were emitted (data:chat_messages:created)
    // - Event coalescing merged them into 1 event
    // - PersonaUser calls buildContext() once instead of 14 times
    // - RAG context should STILL contain all 14 messages

    const ragBuilder = new ChatRAGBuilder();

    // Simulate building context after event coalescing
    // (In reality, PersonaUser.handleChatMessage would be called once with latest message)
    const ragContext = await ragBuilder.buildContext(
      testRoomId,
      testPersonaId,
      { maxMessages: 20 }
    );

    // ALL messages should be in context
    expect(ragContext.conversationHistory.length).toBe(TEST_MESSAGE_COUNT);

    // Verify each message is complete
    ragContext.conversationHistory.forEach((msg, idx) => {
      expect(msg.content).toBeTruthy();
      expect(msg.name).toBeTruthy();
      expect(msg.role).toBeTruthy();
      expect(msg.timestamp).toBeTruthy();
    });

    console.log(`✅ Complete RAG context built from database, not from coalesced events`);
  });

  it('should include currentMessage in RAG even if not yet in database', async () => {
    const ragBuilder = new ChatRAGBuilder();

    // Simulate the case where a message just arrived but might not be in DB yet
    const currentMessage = {
      role: 'user' as const,
      content: 'Brand new message not yet saved',
      name: 'Human User',
      timestamp: Date.now()
    };

    const ragContext = await ragBuilder.buildContext(
      testRoomId,
      testPersonaId,
      {
        maxMessages: 20,
        currentMessage // PersonaUser passes this explicitly
      }
    );

    // Should have 14 from database + 1 current = 15 total
    expect(ragContext.conversationHistory.length).toBe(TEST_MESSAGE_COUNT + 1);

    // Last message should be the current one
    const lastMessage = ragContext.conversationHistory[ragContext.conversationHistory.length - 1];
    expect(lastMessage.content).toBe('Brand new message not yet saved');
    expect(lastMessage.name).toBe('Human User');

    console.log(`✅ Current message included in RAG even before database persistence`);
  });

  it('should query database with correct filters and limits', async () => {
    const ragBuilder = new ChatRAGBuilder();

    // Build context with specific limits
    const ragContext = await ragBuilder.buildContext(
      testRoomId,
      testPersonaId,
      { maxMessages: 5 } // Limit to 5 messages
    );

    // Should only have 5 most recent messages
    expect(ragContext.conversationHistory.length).toBe(5);

    // Should be messages 10-14 (most recent 5)
    for (let i = 0; i < 5; i++) {
      const message = ragContext.conversationHistory[i];
      const expectedMessageNum = 10 + i; // Messages 10, 11, 12, 13, 14
      expect(message.content).toContain(`Message ${expectedMessageNum} of ${TEST_MESSAGE_COUNT}`);
    }

    console.log(`✅ RAG respects maxMessages limit when querying database`);
  });
});
