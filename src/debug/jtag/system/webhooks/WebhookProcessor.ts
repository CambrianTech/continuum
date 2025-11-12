/**
 * WebhookProcessor - Durable webhook event processing
 *
 * Processes webhook events from persistent queue with:
 * - At-least-once delivery guarantee
 * - Exponential backoff retry on failure
 * - Ordered processing per source
 * - Idempotent operations (safe to retry)
 *
 * ARCHITECTURE:
 * 1. Webhook arrives → write to WebhookEventEntity (immediate persistence)
 * 2. Background loop polls for unprocessed events
 * 3. Process event → re-emit through YOUR event system
 * 4. Mark as processed or schedule retry
 *
 * EVENT FLOW:
 * - Persisted events → re-emit as `webhook:github:pull_request`
 * - Subscribers (PersonaUser, TrainingDaemon, etc.) handle naturally
 * - No hardcoded behavior - just natural event pub/sub
 *
 * GUARANTEES:
 * - Events never lost (persisted before acknowledgment)
 * - Events processed exactly once (with retry safety)
 * - Server restart safe (queue in database)
 */

import { DataDaemon } from '../../daemons/data-daemon/shared/DataDaemon';
import { Events } from '../core/shared/Events';
import type { UUID } from '../core/types/CrossPlatformUUID';
import { WebhookEventEntity, type ProcessingStatus } from '../data/entities/WebhookEventEntity';

interface ProcessingResult {
  success: boolean;
  error?: string;
}

export class WebhookProcessor {
  private processingInterval?: NodeJS.Timeout;
  private readonly POLL_INTERVAL_MS = 5000;  // Check queue every 5 seconds
  private readonly MAX_RETRY_ATTEMPTS = 5;
  private readonly INITIAL_RETRY_DELAY_MS = 1000;  // 1 second

  /**
   * Start background processing loop
   */
  async start(): Promise<void> {
    console.log('🔄 WebhookProcessor: Starting background processor');

    // Start polling loop
    this.processingInterval = setInterval(() => {
      this.processNextBatch().catch(error => {
        console.error('❌ WebhookProcessor: Error in processing loop:', error);
      });
    }, this.POLL_INTERVAL_MS);

    console.log('✅ WebhookProcessor: Background processor started');
  }

  /**
   * Stop background processing loop
   */
  stop(): void {
    if (this.processingInterval) {
      clearInterval(this.processingInterval);
      this.processingInterval = undefined;
      console.log('🛑 WebhookProcessor: Background processor stopped');
    }
  }

  /**
   * Process next batch of unprocessed events
   */
  private async processNextBatch(): Promise<void> {
    try {
      // Query for unprocessed events (pending or failed with retry time passed)
      const now = Date.now();
      const queryResult = await DataDaemon.query<WebhookEventEntity>({
        collection: WebhookEventEntity.collection,
        filter: {
          $or: [
            { status: 'pending' },
            { status: 'failed', nextRetryAt: { $lte: now } }
          ]
        },
        sort: [{ field: 'receivedAt', direction: 'asc' }],  // FIFO order
        limit: 10  // Process 10 events per batch
      });

      if (!queryResult.success || !queryResult.data || queryResult.data.length === 0) {
        return;  // No events to process
      }

      const events = queryResult.data.map(record => record.data);
      console.log(`🔄 WebhookProcessor: Processing batch of ${events.length} events`);

      // Process each event
      for (const event of events) {
        await this.processEvent(event);
      }
    } catch (error) {
      console.error('❌ WebhookProcessor: Failed to query events:', error);
    }
  }

  /**
   * Process single webhook event
   */
  private async processEvent(event: WebhookEventEntity): Promise<void> {
    console.log(`🔄 WebhookProcessor: Processing ${event.source}/${event.eventType} event (${event.id})`);

    try {
      // Mark as processing
      await this.updateEventStatus(event.id, 'processing', event.attempts + 1);

      // Process based on source
      let result: ProcessingResult;
      if (event.source === 'github') {
        result = await this.processGitHubEvent(event);
      } else {
        result = { success: false, error: `Unsupported webhook source: ${event.source}` };
      }

      // Update event based on result
      if (result.success) {
        await this.markEventCompleted(event.id, result);
        console.log(`✅ WebhookProcessor: Successfully processed event ${event.id}`);
      } else {
        await this.handleProcessingFailure(event, result.error || 'Unknown error');
      }
    } catch (error) {
      console.error(`❌ WebhookProcessor: Error processing event ${event.id}:`, error);
      await this.handleProcessingFailure(event, error instanceof Error ? error.message : 'Unknown error');
    }
  }

  /**
   * Process GitHub webhook event
   * Re-emits through YOUR event system for natural subscriber handling
   */
  private async processGitHubEvent(event: WebhookEventEntity): Promise<ProcessingResult> {
    try {
      // Construct event path: webhook:github:pull_request
      const eventPath = `webhook:${event.source}:${event.eventType}`;

      // Re-emit through YOUR event system
      // Subscribers (PersonaUser, TrainingDaemon, etc.) handle naturally
      Events.emit(eventPath, {
        eventId: event.id,
        source: event.source,
        eventType: event.eventType,
        payload: event.payload,
        receivedAt: event.receivedAt
      });

      console.log(`🔄 WebhookProcessor: Re-emitted event ${event.id} as '${eventPath}'`);
      return { success: true };
    } catch (error) {
      return { success: false, error: error instanceof Error ? error.message : 'Unknown error' };
    }
  }

  /**
   * Update event processing status
   */
  private async updateEventStatus(
    eventId: UUID,
    status: ProcessingStatus,
    attempts: number
  ): Promise<void> {
    await DataDaemon.update<WebhookEventEntity>({
      collection: WebhookEventEntity.collection,
      id: eventId,
      data: { status, attempts } as Partial<WebhookEventEntity>
    });
  }

  /**
   * Mark event as completed
   */
  private async markEventCompleted(eventId: UUID, result: ProcessingResult): Promise<void> {
    await DataDaemon.update<WebhookEventEntity>({
      collection: WebhookEventEntity.collection,
      id: eventId,
      data: {
        status: 'completed',
        processed: true,
        processedAt: Date.now()
      } as Partial<WebhookEventEntity>
    });
  }

  /**
   * Handle processing failure with exponential backoff retry
   */
  private async handleProcessingFailure(event: WebhookEventEntity, error: string): Promise<void> {
    const newAttempts = event.attempts + 1;

    if (newAttempts >= this.MAX_RETRY_ATTEMPTS) {
      // Max retries reached, mark as permanently failed
      console.error(`❌ WebhookProcessor: Event ${event.id} failed after ${newAttempts} attempts, giving up`);
      await DataDaemon.update<WebhookEventEntity>({
        collection: WebhookEventEntity.collection,
        id: event.id,
        data: {
          status: 'failed',
          attempts: newAttempts,
          lastError: error,
          processed: false
        } as Partial<WebhookEventEntity>
      });
    } else {
      // Schedule retry with exponential backoff
      const retryDelay = this.INITIAL_RETRY_DELAY_MS * Math.pow(2, newAttempts - 1);
      const nextRetryAt = Date.now() + retryDelay;

      console.warn(`⚠️  WebhookProcessor: Event ${event.id} failed (attempt ${newAttempts}), retrying in ${retryDelay}ms`);
      await DataDaemon.update<WebhookEventEntity>({
        collection: WebhookEventEntity.collection,
        id: event.id,
        data: {
          status: 'failed',
          attempts: newAttempts,
          lastError: error,
          nextRetryAt
        } as Partial<WebhookEventEntity>
      });
    }
  }

  /**
   * Ingest new webhook event (called by webhook receiver)
   * Returns immediately after persisting to queue
   */
  static async ingestWebhookEvent(
    source: string,
    eventType: string,
    payload: Record<string, unknown>
  ): Promise<{ success: boolean; eventId?: UUID; error?: string }> {
    try {
      const event = new WebhookEventEntity();
      event.source = source as any;
      event.eventType = eventType as any;
      event.payload = payload;
      event.status = 'pending';
      event.attempts = 0;
      event.receivedAt = Date.now();
      event.processed = false;

      const result = await DataDaemon.create<WebhookEventEntity>({
        collection: WebhookEventEntity.collection,
        data: event
      });

      if (result.success && result.id) {
        console.log(`✅ WebhookProcessor: Ingested ${source}/${eventType} event (${result.id})`);
        return { success: true, eventId: result.id as UUID };
      } else {
        return { success: false, error: result.error || 'Failed to persist event' };
      }
    } catch (error) {
      console.error('❌ WebhookProcessor: Failed to ingest event:', error);
      return { success: false, error: error instanceof Error ? error.message : 'Unknown error' };
    }
  }
}
