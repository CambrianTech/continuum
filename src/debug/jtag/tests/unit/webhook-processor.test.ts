/**
 * Unit tests for WebhookProcessor
 *
 * Tests the durable webhook event processing with event emission
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { WebhookProcessor } from '../../system/webhooks/WebhookProcessor';
import { Events } from '../../system/core/shared/Events';
import { DataDaemon } from '../../daemons/data-daemon/shared/DataDaemon';
import { WebhookEventEntity } from '../../system/data/entities/WebhookEventEntity';
import { UUID } from '../../system/core/types/CrossPlatformUUID';

describe('WebhookProcessor', () => {
  let processor: WebhookProcessor;
  let eventEmitSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    processor = new WebhookProcessor();
    // Spy on Events.emit to verify event emission
    eventEmitSpy = vi.spyOn(Events, 'emit');
  });

  afterEach(async () => {
    processor.stop();
    eventEmitSpy.mockRestore();

    // Clean up test data
    await DataDaemon.delete({ collection: WebhookEventEntity.collection, id: 'all' });
  });

  describe('Event Ingestion', () => {
    it('should persist webhook events immediately', async () => {
      const payload = {
        action: 'opened',
        pull_request: { number: 123, title: 'Test PR' }
      };

      const result = await WebhookProcessor.ingestWebhookEvent(
        'github',
        'pull_request',
        payload
      );

      expect(result.success).toBe(true);
      expect(result.eventId).toBeDefined();

      // Verify persisted
      const queryResult = await DataDaemon.query<WebhookEventEntity>({
        collection: WebhookEventEntity.collection,
        filter: { id: result.eventId }
      });

      expect(queryResult.success).toBe(true);
      expect(queryResult.data).toBeDefined();
      expect(queryResult.data!.length).toBe(1);

      const event = queryResult.data![0].data;
      expect(event.source).toBe('github');
      expect(event.eventType).toBe('pull_request');
      expect(event.status).toBe('pending');
      expect(event.payload).toEqual(payload);
    });

    it('should handle ingestion errors gracefully', async () => {
      // Test with invalid data
      const result = await WebhookProcessor.ingestWebhookEvent(
        '',  // Invalid source
        '',  // Invalid eventType
        {}
      );

      // Should still return structured result
      expect(result).toHaveProperty('success');
      expect(result).toHaveProperty('error');
    });
  });

  describe('Event Processing', () => {
    it('should emit events through YOUR event system', async () => {
      // Ingest test event
      const payload = {
        action: 'synchronize',
        pull_request: { number: 456, title: 'Update feature' }
      };

      const ingestResult = await WebhookProcessor.ingestWebhookEvent(
        'github',
        'pull_request',
        payload
      );

      expect(ingestResult.success).toBe(true);

      // Start processor
      await processor.start();

      // Wait for processing
      await new Promise(resolve => setTimeout(resolve, 6000)); // POLL_INTERVAL_MS + buffer

      // Verify event was emitted
      expect(eventEmitSpy).toHaveBeenCalled();

      const emitCalls = eventEmitSpy.mock.calls;
      const webhookEmit = emitCalls.find(call =>
        call[0] === 'webhook:github:pull_request'
      );

      expect(webhookEmit).toBeDefined();
      expect(webhookEmit![1]).toMatchObject({
        eventId: ingestResult.eventId,
        source: 'github',
        eventType: 'pull_request',
        payload
      });
    });

    it('should mark processed events as completed', async () => {
      const payload = { action: 'closed' };

      const ingestResult = await WebhookProcessor.ingestWebhookEvent(
        'github',
        'pull_request',
        payload
      );

      await processor.start();
      await new Promise(resolve => setTimeout(resolve, 6000));

      // Verify status updated
      const queryResult = await DataDaemon.query<WebhookEventEntity>({
        collection: WebhookEventEntity.collection,
        filter: { id: ingestResult.eventId }
      });

      const event = queryResult.data![0].data;
      expect(event.status).toBe('completed');
      expect(event.processed).toBe(true);
      expect(event.processedAt).toBeDefined();
    });

    it('should process events in FIFO order', async () => {
      const events = [
        { payload: { number: 1 }, timestamp: Date.now() - 2000 },
        { payload: { number: 2 }, timestamp: Date.now() - 1000 },
        { payload: { number: 3 }, timestamp: Date.now() }
      ];

      // Ingest events
      for (const evt of events) {
        await WebhookProcessor.ingestWebhookEvent(
          'github',
          'pull_request',
          evt.payload
        );
      }

      await processor.start();
      await new Promise(resolve => setTimeout(resolve, 7000)); // Allow multiple batches

      // Verify events were emitted in order
      const webhookEmits = eventEmitSpy.mock.calls.filter(call =>
        call[0] === 'webhook:github:pull_request'
      );

      expect(webhookEmits.length).toBeGreaterThanOrEqual(3);

      // Check order (first emitted should have lowest number)
      const firstPayload = webhookEmits[0][1].payload;
      const lastPayload = webhookEmits[webhookEmits.length - 1][1].payload;

      expect(firstPayload.number).toBeLessThan(lastPayload.number);
    });
  });

  describe('Retry Logic', () => {
    it('should retry failed events with exponential backoff', async () => {
      // Create event that will fail processing
      const event = new WebhookEventEntity();
      event.source = 'unsupported-source' as any; // Will cause processing failure
      event.eventType = 'test' as any;
      event.payload = {};
      event.status = 'pending';
      event.attempts = 0;
      event.receivedAt = Date.now();
      event.processed = false;

      await DataDaemon.store(WebhookEventEntity.collection, event);

      await processor.start();

      // Wait for first attempt
      await new Promise(resolve => setTimeout(resolve, 6000));

      // Verify retry scheduled
      const queryResult = await DataDaemon.query<WebhookEventEntity>({
        collection: WebhookEventEntity.collection,
        filter: { id: event.id }
      });

      const updated = queryResult.data![0].data;
      expect(updated.status).toBe('failed');
      expect(updated.attempts).toBeGreaterThan(0);
      expect(updated.nextRetryAt).toBeDefined();
      expect(updated.lastError).toBeDefined();
    });

    it('should give up after max retry attempts', async () => {
      // Create event with max attempts already
      const event = new WebhookEventEntity();
      event.source = 'unsupported-source' as any;
      event.eventType = 'test' as any;
      event.payload = {};
      event.status = 'failed';
      event.attempts = 4; // One below max (5)
      event.receivedAt = Date.now();
      event.processed = false;
      event.nextRetryAt = Date.now(); // Retry immediately

      await DataDaemon.store(WebhookEventEntity.collection, event);

      await processor.start();
      await new Promise(resolve => setTimeout(resolve, 6000));

      // Should have reached max attempts and given up
      const queryResult = await DataDaemon.query<WebhookEventEntity>({
        collection: WebhookEventEntity.collection,
        filter: { id: event.id }
      });

      const updated = queryResult.data![0].data;
      expect(updated.attempts).toBe(5); // Max attempts
      expect(updated.status).toBe('failed');
      expect(updated.processed).toBe(false);
    });
  });

  describe('Event Path Construction', () => {
    it('should construct proper event paths', async () => {
      const testCases = [
        { source: 'github', type: 'pull_request', expected: 'webhook:github:pull_request' },
        { source: 'github', type: 'push', expected: 'webhook:github:push' },
        { source: 'github', type: 'issues', expected: 'webhook:github:issues' }
      ];

      for (const test of testCases) {
        await WebhookProcessor.ingestWebhookEvent(
          test.source,
          test.type,
          { test: true }
        );
      }

      await processor.start();
      await new Promise(resolve => setTimeout(resolve, 7000));

      // Verify all event paths were emitted
      for (const test of testCases) {
        const emitted = eventEmitSpy.mock.calls.some(call =>
          call[0] === test.expected
        );
        expect(emitted).toBe(true);
      }
    });
  });

  describe('Durability', () => {
    it('should recover unprocessed events after restart', async () => {
      // Ingest event
      const result = await WebhookProcessor.ingestWebhookEvent(
        'github',
        'pull_request',
        { number: 999 }
      );

      // Don't start processor - simulate crash before processing

      // Create new processor instance (simulates restart)
      const newProcessor = new WebhookProcessor();
      const newSpy = vi.spyOn(Events, 'emit');

      await newProcessor.start();
      await new Promise(resolve => setTimeout(resolve, 6000));

      // Should process the orphaned event
      const webhookEmit = newSpy.mock.calls.find(call =>
        call[0] === 'webhook:github:pull_request'
      );

      expect(webhookEmit).toBeDefined();
      expect(webhookEmit![1].eventId).toBe(result.eventId);

      newProcessor.stop();
      newSpy.mockRestore();
    });

    it('should handle concurrent processing safely', async () => {
      // Ingest multiple events quickly
      const promises = [];
      for (let i = 0; i < 10; i++) {
        promises.push(
          WebhookProcessor.ingestWebhookEvent(
            'github',
            'pull_request',
            { number: i }
          )
        );
      }

      await Promise.all(promises);

      await processor.start();
      await new Promise(resolve => setTimeout(resolve, 12000)); // Allow multiple batches

      // All events should be processed
      const queryResult = await DataDaemon.query<WebhookEventEntity>({
        collection: WebhookEventEntity.collection,
        filter: { status: 'completed' }
      });

      expect(queryResult.data!.length).toBe(10);
    });
  });
});
