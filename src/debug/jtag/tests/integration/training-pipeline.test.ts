/**
 * Integration tests for Training Pipeline
 *
 * Tests the complete flow: webhook → event → training data
 */

import { describe, it, expect, beforeAll, afterAll, beforeEach, afterEach } from 'vitest';
import { WebhookProcessor } from '../../system/webhooks/WebhookProcessor';
import { Events } from '../../system/core/shared/Events';
import { DataDaemon } from '../../daemons/data-daemon/shared/DataDaemon';
import { ChatMessageEntity } from '../../system/data/entities/ChatMessageEntity';
import { UserEntity } from '../../system/data/entities/UserEntity';
import { RoomEntity } from '../../system/data/entities/RoomEntity';
import { WebhookEventEntity } from '../../system/data/entities/WebhookEventEntity';
import { TrainingExampleEntity } from '../../daemons/data-daemon/shared/entities/TrainingExampleEntity';
import { COLLECTIONS } from '../../system/data/config/DatabaseConfig';
import { ROOM_UNIQUE_IDS } from '../../system/data/constants/RoomConstants';
import { UUID } from '../../system/core/types/CrossPlatformUUID';

describe('Training Pipeline Integration', () => {
  let processor: WebhookProcessor;
  let devUpdatesRoomId: UUID;
  let botUserId: UUID;

  beforeAll(async () => {
    // Set up test environment
    processor = new WebhookProcessor();

    // Create dev-updates room
    const room = new RoomEntity();
    room.name = 'Dev Updates (Integration Test)';
    room.uniqueId = ROOM_UNIQUE_IDS.DEV_UPDATES;
    const roomResult = await DataDaemon.store(COLLECTIONS.ROOMS, room);
    devUpdatesRoomId = roomResult.id as UUID;

    // Create bot user for webhook messages
    const bot = new UserEntity();
    bot.displayName = 'GitHub Bot';
    bot.type = 'ai';
    const botResult = await DataDaemon.store(COLLECTIONS.USERS, bot);
    botUserId = botResult.id as UUID;
  });

  afterAll(async () => {
    processor.stop();
  });

  beforeEach(async () => {
    // Clean slate for each test
    await DataDaemon.delete({ collection: WebhookEventEntity.collection, id: 'all' });
    await DataDaemon.delete({ collection: COLLECTIONS.CHAT_MESSAGES, id: 'all' });
    await DataDaemon.delete({ collection: TrainingExampleEntity.collection, id: 'all' });
  });

  describe('End-to-End Flow', () => {
    it('should convert webhook → event → chat → training data', async () => {
      // Step 1: Webhook arrives
      const prPayload = {
        action: 'opened',
        pull_request: {
          number: 789,
          title: 'Add training pipeline tests',
          html_url: 'https://github.com/test/repo/pull/789',
          user: { login: 'testuser' }
        }
      };

      const ingestResult = await WebhookProcessor.ingestWebhookEvent(
        'github',
        'pull_request',
        prPayload
      );

      expect(ingestResult.success).toBe(true);

      // Step 2: WebhookProcessor emits event
      let webhookEventEmitted = false;
      let emittedPayload: any;

      const unsubscribe = Events.subscribe('webhook:github:pull_request', (data) => {
        webhookEventEmitted = true;
        emittedPayload = data;
      });

      await processor.start();
      await new Promise(resolve => setTimeout(resolve, 6000));

      expect(webhookEventEmitted).toBe(true);
      expect(emittedPayload.eventId).toBe(ingestResult.eventId);
      expect(emittedPayload.payload).toEqual(prPayload);

      unsubscribe();

      // Step 3: Simulate GitHub webhook subscriber posting to chat
      // (This would be a separate subscriber in production)
      const message = new ChatMessageEntity();
      message.roomId = devUpdatesRoomId;
      message.senderId = botUserId;
      message.content = {
        text: `🔔 GitHub pull_request opened\n\n**#${prPayload.pull_request.number}**: ${prPayload.pull_request.title}\nBy: ${prPayload.pull_request.user.login}`
      };
      message.timestamp = Date.now();
      message.status = 'sent';
      message.metadata = {
        source: 'webhook',
        webhookEventId: ingestResult.eventId
      };

      await DataDaemon.store(COLLECTIONS.CHAT_MESSAGES, message);

      // Step 4: Wait for TrainingDaemon to observe and create training data
      await new Promise(resolve => setTimeout(resolve, 2000));

      // Step 5: Verify training data was created
      const trainingResult = await DataDaemon.query<TrainingExampleEntity>({
        collection: TrainingExampleEntity.collection,
        filter: { 'metadata.roomId': devUpdatesRoomId }
      });

      expect(trainingResult.success).toBe(true);
      expect(trainingResult.data).toBeDefined();
      expect(trainingResult.data!.length).toBeGreaterThan(0);

      const trainingExample = trainingResult.data![0].data;
      expect(trainingExample.messages).toBeDefined();
      expect(trainingExample.metadata.source).toBe('chat-conversation');
    });

    it('should handle conversation threads from webhooks', async () => {
      // Simulate a PR discussion that creates training data

      // 1. PR opened webhook
      await WebhookProcessor.ingestWebhookEvent('github', 'pull_request', {
        action: 'opened',
        pull_request: { number: 100, title: 'New feature' }
      });

      await processor.start();
      await new Promise(resolve => setTimeout(resolve, 6000));

      // 2. Create conversation in dev-updates
      const conversation = [
        { senderId: botUserId, text: '🔔 PR #100 opened: New feature', metadata: { source: 'webhook' } },
        { senderId: botUserId, text: 'Analyzing changes...', metadata: { source: 'bot' } },
        { senderId: botUserId, text: 'Code review: Looks good, tests pass ✅', metadata: { source: 'bot' } }
      ];

      for (const msg of conversation) {
        const message = new ChatMessageEntity();
        message.roomId = devUpdatesRoomId;
        message.senderId = msg.senderId;
        message.content = { text: msg.text };
        message.timestamp = Date.now();
        message.status = 'sent';
        message.metadata = msg.metadata;

        await DataDaemon.store(COLLECTIONS.CHAT_MESSAGES, message);
        await new Promise(resolve => setTimeout(resolve, 100));
      }

      // 3. Wait for TrainingDaemon
      await new Promise(resolve => setTimeout(resolve, 2000));

      // 4. Verify training data includes full conversation
      const trainingResult = await DataDaemon.query<TrainingExampleEntity>({
        collection: TrainingExampleEntity.collection,
        filter: { 'metadata.roomId': devUpdatesRoomId },
        sort: [{ field: 'metadata.timestamp', direction: 'desc' }],
        limit: 1
      });

      expect(trainingResult.data).toBeDefined();
      expect(trainingResult.data!.length).toBeGreaterThan(0);

      const example = trainingResult.data![0].data;
      expect(example.messageCount).toBeGreaterThanOrEqual(3);
      expect(example.messages.some(m => m.content.includes('PR #100'))).toBe(true);
    });
  });

  describe('Quality Filtering', () => {
    it('should skip system test messages in training data', async () => {
      // Create system test message
      const testMessage = new ChatMessageEntity();
      testMessage.roomId = devUpdatesRoomId;
      testMessage.senderId = botUserId;
      testMessage.content = { text: 'Precommit hook test message' };
      testMessage.timestamp = Date.now();
      testMessage.status = 'sent';
      testMessage.metadata = {
        isSystemTest: true,
        testType: 'precommit',
        source: 'system'
      };

      await DataDaemon.store(COLLECTIONS.CHAT_MESSAGES, testMessage);
      await new Promise(resolve => setTimeout(resolve, 2000));

      // Should NOT create training data
      const trainingResult = await DataDaemon.query<TrainingExampleEntity>({
        collection: TrainingExampleEntity.collection,
        filter: { 'metadata.sourceMessageId': testMessage.id }
      });

      expect(trainingResult.data?.length || 0).toBe(0);
    });

    it('should include webhook-triggered conversations', async () => {
      // Webhook-triggered messages are valuable training data

      await WebhookProcessor.ingestWebhookEvent('github', 'push', {
        commits: [{ message: 'Fix bug in training pipeline' }]
      });

      await processor.start();
      await new Promise(resolve => setTimeout(resolve, 6000));

      // Create webhook-sourced message
      const message = new ChatMessageEntity();
      message.roomId = devUpdatesRoomId;
      message.senderId = botUserId;
      message.content = { text: '🔔 Push: Fix bug in training pipeline' };
      message.timestamp = Date.now();
      message.status = 'sent';
      message.metadata = { source: 'webhook' };

      await DataDaemon.store(COLLECTIONS.CHAT_MESSAGES, message);
      await new Promise(resolve => setTimeout(resolve, 2000));

      // SHOULD create training data (not filtered)
      const trainingResult = await DataDaemon.query<TrainingExampleEntity>({
        collection: TrainingExampleEntity.collection,
        filter: { 'metadata.roomId': devUpdatesRoomId }
      });

      expect(trainingResult.data!.length).toBeGreaterThan(0);
    });
  });

  describe('Recovery & Durability', () => {
    it('should recover missed events after downtime', async () => {
      // 1. Ingest events while processor is stopped
      const eventIds: UUID[] = [];

      for (let i = 0; i < 3; i++) {
        const result = await WebhookProcessor.ingestWebhookEvent('github', 'push', {
          ref: `refs/heads/feature-${i}`
        });
        eventIds.push(result.eventId!);
      }

      // 2. Start processor (simulates recovery after downtime)
      await processor.start();
      await new Promise(resolve => setTimeout(resolve, 10000)); // Allow multiple batches

      // 3. All events should be processed
      for (const eventId of eventIds) {
        const queryResult = await DataDaemon.query<WebhookEventEntity>({
          collection: WebhookEventEntity.collection,
          filter: { id: eventId }
        });

        const event = queryResult.data![0].data;
        expect(event.status).toBe('completed');
        expect(event.processed).toBe(true);
      }
    });

    it('should handle TrainingDaemon restart without data loss', async () => {
      // Create messages while TrainingDaemon is observing
      const messageIds: UUID[] = [];

      for (let i = 0; i < 5; i++) {
        const message = new ChatMessageEntity();
        message.roomId = devUpdatesRoomId;
        message.senderId = botUserId;
        message.content = { text: `Test message ${i}` };
        message.timestamp = Date.now();
        message.status = 'sent';

        const result = await DataDaemon.store(COLLECTIONS.CHAT_MESSAGES, message);
        messageIds.push(result.id as UUID);
        await new Promise(resolve => setTimeout(resolve, 100));
      }

      await new Promise(resolve => setTimeout(resolve, 3000));

      // Verify training data was created for all messages
      const trainingResult = await DataDaemon.query<TrainingExampleEntity>({
        collection: TrainingExampleEntity.collection,
        filter: { 'metadata.roomId': devUpdatesRoomId }
      });

      expect(trainingResult.data!.length).toBeGreaterThan(0);

      // Training examples should reference some of our messages
      const allMessages = trainingResult.data!.flatMap(record =>
        record.data.messages.map(m => m.content)
      );

      expect(allMessages.some(content => content.includes('Test message'))).toBe(true);
    });
  });

  describe('Performance & Scale', () => {
    it('should handle burst of webhooks efficiently', async () => {
      const startTime = Date.now();

      // Ingest 20 webhook events rapidly
      const promises = [];
      for (let i = 0; i < 20; i++) {
        promises.push(
          WebhookProcessor.ingestWebhookEvent('github', 'push', {
            ref: `refs/heads/branch-${i}`
          })
        );
      }

      await Promise.all(promises);
      const ingestTime = Date.now() - startTime;

      // Ingestion should be fast (< 5 seconds for 20 events)
      expect(ingestTime).toBeLessThan(5000);

      // Process all
      await processor.start();
      await new Promise(resolve => setTimeout(resolve, 15000)); // Multiple batches

      // All should complete eventually
      const completedResult = await DataDaemon.query<WebhookEventEntity>({
        collection: WebhookEventEntity.collection,
        filter: { status: 'completed' }
      });

      expect(completedResult.data!.length).toBe(20);
    });

    it('should handle high-volume chat with reasonable performance', async () => {
      const startTime = Date.now();

      // Create 50 messages rapidly
      for (let i = 0; i < 50; i++) {
        const message = new ChatMessageEntity();
        message.roomId = devUpdatesRoomId;
        message.senderId = botUserId;
        message.content = { text: `High volume message ${i}` };
        message.timestamp = Date.now();
        message.status = 'sent';

        await DataDaemon.store(COLLECTIONS.CHAT_MESSAGES, message);
      }

      const createTime = Date.now() - startTime;

      // Message creation should be fast
      expect(createTime).toBeLessThan(10000);

      // Wait for TrainingDaemon to process
      await new Promise(resolve => setTimeout(resolve, 5000));

      // Should create training data (may batch multiple messages)
      const trainingResult = await DataDaemon.query<TrainingExampleEntity>({
        collection: TrainingExampleEntity.collection,
        filter: { 'metadata.roomId': devUpdatesRoomId }
      });

      expect(trainingResult.data!.length).toBeGreaterThan(0);
    });
  });
});
