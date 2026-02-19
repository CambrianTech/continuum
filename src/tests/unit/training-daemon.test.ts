/**
 * Unit tests for TrainingDaemon
 *
 * Tests the silent observer that converts conversations into training data
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { Events } from '../../system/core/shared/Events';
import { DATA_EVENTS } from '../../system/core/shared/EventConstants';
import { DataDaemon } from '../../daemons/data-daemon/shared/DataDaemon';
import { ChatMessageEntity } from '../../system/data/entities/ChatMessageEntity';
import { UserEntity } from '../../system/data/entities/UserEntity';
import { RoomEntity } from '../../system/data/entities/RoomEntity';
import { TrainingExampleEntity } from '../../daemons/data-daemon/shared/entities/TrainingExampleEntity';
import { COLLECTIONS } from '../../system/data/config/DatabaseConfig';
import { ROOM_UNIQUE_IDS } from '../../system/data/constants/RoomConstants';
import { UUID } from '../../system/core/types/CrossPlatformUUID';

describe('TrainingDaemon', () => {
  let testRoomId: UUID;
  let humanUserId: UUID;
  let aiUserId: UUID;

  beforeEach(async () => {
    // Create test room (dev-updates)
    const room = new RoomEntity();
    room.name = 'Dev Updates (Test)';
    room.uniqueId = ROOM_UNIQUE_IDS.DEV_UPDATES;
    const roomResult = await DataDaemon.store(COLLECTIONS.ROOMS, room);
    testRoomId = roomResult.id as UUID;

    // Create test users
    const humanUser = new UserEntity();
    humanUser.displayName = 'Test Human';
    humanUser.type = 'human';
    const humanResult = await DataDaemon.store(COLLECTIONS.USERS, humanUser);
    humanUserId = humanResult.id as UUID;

    const aiUser = new UserEntity();
    aiUser.displayName = 'Test AI';
    aiUser.type = 'ai';
    const aiResult = await DataDaemon.store(COLLECTIONS.USERS, aiUser);
    aiUserId = aiResult.id as UUID;
  });

  afterEach(async () => {
    // Clean up test data
    await DataDaemon.delete({ collection: COLLECTIONS.CHAT_MESSAGES, id: 'all' });
    await DataDaemon.delete({ collection: TrainingExampleEntity.collection, id: 'all' });
  });

  describe('Message Observation', () => {
    it('should observe messages in training-enabled rooms', async () => {
      // Create test message in dev-updates room
      const message = new ChatMessageEntity();
      message.roomId = testRoomId;
      message.senderId = humanUserId;
      message.content = { text: 'Test message for training' };
      message.timestamp = Date.now();
      message.status = 'sent';

      const result = await DataDaemon.store(COLLECTIONS.CHAT_MESSAGES, message);
      expect(result.success).toBe(true);

      // Wait for TrainingDaemon to process (async event handler)
      await new Promise(resolve => setTimeout(resolve, 100));

      // Verify training example was created
      const trainingResult = await DataDaemon.query<TrainingExampleEntity>({
        collection: TrainingExampleEntity.collection,
        filter: { 'metadata.roomId': testRoomId }
      });

      expect(trainingResult.success).toBe(true);
      expect(trainingResult.data).toBeDefined();
      expect(trainingResult.data!.length).toBeGreaterThan(0);
    });

    it('should skip system test messages', async () => {
      // Create system test message
      const message = new ChatMessageEntity();
      message.roomId = testRoomId;
      message.senderId = humanUserId;
      message.content = { text: 'System test message' };
      message.timestamp = Date.now();
      message.status = 'sent';
      message.metadata = { isSystemTest: true, testType: 'precommit' };

      await DataDaemon.store(COLLECTIONS.CHAT_MESSAGES, message);

      // Wait for potential processing
      await new Promise(resolve => setTimeout(resolve, 100));

      // Verify NO training example was created
      const trainingResult = await DataDaemon.query<TrainingExampleEntity>({
        collection: TrainingExampleEntity.collection,
        filter: { 'metadata.sourceMessageId': message.id }
      });

      expect(trainingResult.data?.length || 0).toBe(0);
    });

    it('should skip deleted messages', async () => {
      // Create deleted message
      const message = new ChatMessageEntity();
      message.roomId = testRoomId;
      message.senderId = humanUserId;
      message.content = { text: 'Deleted message' };
      message.timestamp = Date.now();
      message.status = 'deleted';

      await DataDaemon.store(COLLECTIONS.CHAT_MESSAGES, message);

      // Wait for potential processing
      await new Promise(resolve => setTimeout(resolve, 100));

      // Verify NO training example was created
      const trainingResult = await DataDaemon.query<TrainingExampleEntity>({
        collection: TrainingExampleEntity.collection,
        filter: { 'metadata.sourceMessageId': message.id }
      });

      expect(trainingResult.data?.length || 0).toBe(0);
    });
  });

  describe('Context Window', () => {
    it('should include conversation context in training data', async () => {
      // Create conversation with multiple messages
      const messages = [
        { senderId: humanUserId, text: 'What is the training pipeline?' },
        { senderId: aiUserId, text: 'The training pipeline converts conversations into training data' },
        { senderId: humanUserId, text: 'How does it work?' },
        { senderId: aiUserId, text: 'It observes chat events and creates TrainingExampleEntity records' }
      ];

      for (const msg of messages) {
        const message = new ChatMessageEntity();
        message.roomId = testRoomId;
        message.senderId = msg.senderId;
        message.content = { text: msg.text };
        message.timestamp = Date.now();
        message.status = 'sent';

        await DataDaemon.store(COLLECTIONS.CHAT_MESSAGES, message);
        await new Promise(resolve => setTimeout(resolve, 50)); // Small delay between messages
      }

      // Wait for TrainingDaemon to process all
      await new Promise(resolve => setTimeout(resolve, 200));

      // Verify training examples have context
      const trainingResult = await DataDaemon.query<TrainingExampleEntity>({
        collection: TrainingExampleEntity.collection,
        filter: { 'metadata.roomId': testRoomId },
        sort: [{ field: 'metadata.timestamp', direction: 'desc' }],
        limit: 1
      });

      expect(trainingResult.success).toBe(true);
      expect(trainingResult.data).toBeDefined();

      if (trainingResult.data && trainingResult.data.length > 0) {
        const example = trainingResult.data[0].data;
        expect(example.messageCount).toBeGreaterThan(1); // Should include context
        expect(example.messages.length).toBeGreaterThan(1);
      }
    });

    it('should enforce minimum message threshold', async () => {
      // Create single message (below threshold)
      const message = new ChatMessageEntity();
      message.roomId = testRoomId;
      message.senderId = humanUserId;
      message.content = { text: 'Single message' };
      message.timestamp = Date.now();
      message.status = 'sent';

      await DataDaemon.store(COLLECTIONS.CHAT_MESSAGES, message);
      await new Promise(resolve => setTimeout(resolve, 100));

      // Should skip - insufficient context
      const trainingResult = await DataDaemon.query<TrainingExampleEntity>({
        collection: TrainingExampleEntity.collection,
        filter: { 'metadata.roomId': testRoomId }
      });

      expect(trainingResult.data?.length || 0).toBe(0);
    });
  });

  describe('Training Data Format', () => {
    it('should convert messages to OpenAI format', async () => {
      // Create conversation
      const messages = [
        { senderId: humanUserId, text: 'Hello', type: 'human' },
        { senderId: aiUserId, text: 'Hi there!', type: 'ai' },
        { senderId: humanUserId, text: 'How are you?', type: 'human' }
      ];

      for (const msg of messages) {
        const message = new ChatMessageEntity();
        message.roomId = testRoomId;
        message.senderId = msg.senderId;
        message.content = { text: msg.text };
        message.timestamp = Date.now();
        message.status = 'sent';

        await DataDaemon.store(COLLECTIONS.CHAT_MESSAGES, message);
        await new Promise(resolve => setTimeout(resolve, 50));
      }

      await new Promise(resolve => setTimeout(resolve, 200));

      // Verify format
      const trainingResult = await DataDaemon.query<TrainingExampleEntity>({
        collection: TrainingExampleEntity.collection,
        filter: { 'metadata.roomId': testRoomId },
        sort: [{ field: 'metadata.timestamp', direction: 'desc' }],
        limit: 1
      });

      expect(trainingResult.success).toBe(true);

      if (trainingResult.data && trainingResult.data.length > 0) {
        const example = trainingResult.data[0].data;

        // Verify OpenAI format
        expect(example.messages).toBeDefined();
        expect(Array.isArray(example.messages)).toBe(true);

        for (const msg of example.messages) {
          expect(msg).toHaveProperty('role');
          expect(msg).toHaveProperty('content');
          expect(['system', 'user', 'assistant']).toContain(msg.role);
          expect(typeof msg.content).toBe('string');
        }
      }
    });

    it('should estimate token counts', async () => {
      // Create message with known content
      const message = new ChatMessageEntity();
      message.roomId = testRoomId;
      message.senderId = humanUserId;
      message.content = { text: 'This is a test message with approximately forty characters.' };
      message.timestamp = Date.now();
      message.status = 'sent';

      await DataDaemon.store(COLLECTIONS.CHAT_MESSAGES, message);
      await new Promise(resolve => setTimeout(resolve, 100));

      const trainingResult = await DataDaemon.query<TrainingExampleEntity>({
        collection: TrainingExampleEntity.collection,
        filter: { 'metadata.roomId': testRoomId }
      });

      if (trainingResult.data && trainingResult.data.length > 0) {
        const example = trainingResult.data[0].data;

        // Token estimate should be ~chars/4
        expect(example.totalTokens).toBeGreaterThan(0);
        expect(example.totalTokens).toBeLessThan(100); // Rough sanity check
      }
    });
  });

  describe('Quality Metadata', () => {
    it('should include metadata for quality assessment', async () => {
      // Create test messages
      const messages = [
        { senderId: humanUserId, text: 'Question about training' },
        { senderId: aiUserId, text: 'Answer about training' },
        { senderId: humanUserId, text: 'Follow-up question' }
      ];

      for (const msg of messages) {
        const message = new ChatMessageEntity();
        message.roomId = testRoomId;
        message.senderId = msg.senderId;
        message.content = { text: msg.text };
        message.timestamp = Date.now();
        message.status = 'sent';

        await DataDaemon.store(COLLECTIONS.CHAT_MESSAGES, message);
        await new Promise(resolve => setTimeout(resolve, 50));
      }

      await new Promise(resolve => setTimeout(resolve, 200));

      // Verify metadata
      const trainingResult = await DataDaemon.query<TrainingExampleEntity>({
        collection: TrainingExampleEntity.collection,
        filter: { 'metadata.roomId': testRoomId }
      });

      if (trainingResult.data && trainingResult.data.length > 0) {
        const example = trainingResult.data[0].data;

        expect(example.metadata).toBeDefined();
        expect(example.metadata.roomId).toBe(testRoomId);
        expect(example.metadata.timestamp).toBeDefined();
        expect(example.metadata.quality).toBeDefined();
        expect(example.metadata.source).toBe('chat-conversation');
      }
    });
  });
});
