/**
 * WebhookProcessor Implementation Tests
 *
 * Tests the WebhookProcessor class in isolation using mocked ORM.
 * The existing webhook-processor.test.ts requires a running server (integration).
 * These tests validate the implementation logic without database dependencies.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { WebhookProcessor } from '../../system/webhooks/WebhookProcessor';
import { Events } from '../../system/core/shared/Events';

// Mock ORM to avoid database dependency
vi.mock('../../daemons/data-daemon/server/ORM', () => {
  const storedEvents = new Map<string, any>();

  return {
    ORM: {
      store: vi.fn(async (_collection: string, entity: any) => {
        storedEvents.set(entity.id, entity);
        return { success: true };
      }),
      query: vi.fn(async ({ filter }: any) => {
        const results = Array.from(storedEvents.values())
          .filter(e => {
            if (filter.status && e.status !== filter.status) return false;
            return true;
          })
          .map(data => ({ data }));
        return { success: true, data: results };
      }),
      delete: vi.fn(async () => ({ success: true })),
      // Expose for test assertions
      _storedEvents: storedEvents,
    },
  };
});

describe('WebhookProcessor Implementation', () => {
  let processor: WebhookProcessor;
  let eventEmitSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(async () => {
    // Clear mock storage
    const { ORM } = await import('../../daemons/data-daemon/server/ORM');
    (ORM as any)._storedEvents.clear();

    processor = new WebhookProcessor();
    eventEmitSpy = vi.spyOn(Events, 'emit');
  });

  afterEach(() => {
    processor.stop();
    eventEmitSpy.mockRestore();
    vi.clearAllMocks();
  });

  describe('Static ingestWebhookEvent', () => {
    it('should persist webhook event with correct fields', async () => {
      const payload = { action: 'opened', pull_request: { number: 42 } };

      const result = await WebhookProcessor.ingestWebhookEvent(
        'github',
        'pull_request',
        payload,
      );

      expect(result.success).toBe(true);
      expect(result.eventId).toBeDefined();

      // Verify ORM.store was called
      const { ORM } = await import('../../daemons/data-daemon/server/ORM');
      expect(ORM.store).toHaveBeenCalledWith(
        'webhook_events',
        expect.objectContaining({
          source: 'github',
          eventType: 'pull_request',
          payload,
          status: 'pending',
          attempts: 0,
          processed: false,
        }),
        'default',
      );
    });

    it('should reject empty source', async () => {
      const result = await WebhookProcessor.ingestWebhookEvent('', 'push', {});
      expect(result.success).toBe(false);
      expect(result.error).toContain('required');
    });

    it('should reject empty eventType', async () => {
      const result = await WebhookProcessor.ingestWebhookEvent('github', '', {});
      expect(result.success).toBe(false);
      expect(result.error).toContain('required');
    });

    it('should assign unique IDs to each event', async () => {
      const r1 = await WebhookProcessor.ingestWebhookEvent('github', 'push', { ref: 'main' });
      const r2 = await WebhookProcessor.ingestWebhookEvent('github', 'push', { ref: 'dev' });

      expect(r1.success).toBe(true);
      expect(r2.success).toBe(true);
      expect(r1.eventId).not.toBe(r2.eventId);
    });
  });

  describe('Event Path Construction', () => {
    it('should construct webhook:{source}:{eventType} paths', async () => {
      await WebhookProcessor.ingestWebhookEvent('github', 'pull_request', { test: true });

      // Start processor and wait for processing
      await processor.start();
      await new Promise(resolve => setTimeout(resolve, 100));

      // Verify correct event path
      const webhookEmit = eventEmitSpy.mock.calls.find(
        call => call[0] === 'webhook:github:pull_request',
      );
      expect(webhookEmit).toBeDefined();
    });

    it('should include event metadata in emission payload', async () => {
      const payload = { action: 'opened', number: 99 };
      const result = await WebhookProcessor.ingestWebhookEvent('github', 'issue', payload);

      await processor.start();
      await new Promise(resolve => setTimeout(resolve, 100));

      const emitted = eventEmitSpy.mock.calls.find(
        call => call[0] === 'webhook:github:issue',
      );
      expect(emitted).toBeDefined();
      expect(emitted![1]).toMatchObject({
        eventId: result.eventId,
        source: 'github',
        eventType: 'issue',
        payload,
      });
    });
  });

  describe('Processing Status', () => {
    it('should mark events as completed after processing', async () => {
      await WebhookProcessor.ingestWebhookEvent('github', 'push', { ref: 'main' });

      await processor.start();
      await new Promise(resolve => setTimeout(resolve, 100));

      // Check that ORM.store was called with completed status
      const { ORM } = await import('../../daemons/data-daemon/server/ORM');
      const storeCalls = (ORM.store as any).mock.calls;
      const completedCall = storeCalls.find(
        (call: any) => call[1].status === 'completed' && call[1].processed === true,
      );
      expect(completedCall).toBeDefined();
    });

    it('should mark unsupported sources as failed', async () => {
      // Directly store an event with unsupported source
      await WebhookProcessor.ingestWebhookEvent('unsupported-source' as any, 'test' as any, {});

      await processor.start();
      await new Promise(resolve => setTimeout(resolve, 100));

      const { ORM } = await import('../../daemons/data-daemon/server/ORM');
      const storeCalls = (ORM.store as any).mock.calls;
      const failedCall = storeCalls.find(
        (call: any) => call[1].status === 'failed',
      );
      expect(failedCall).toBeDefined();
      expect(failedCall[1].lastError).toContain('Unsupported');
    });
  });

  describe('Lifecycle', () => {
    it('should start and stop cleanly', async () => {
      await processor.start();
      // Starting again should be a no-op
      await processor.start();

      processor.stop();
      // Stopping again should be safe
      processor.stop();
    });
  });
});
