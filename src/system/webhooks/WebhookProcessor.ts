/**
 * WebhookProcessor — Durable webhook event processing with event emission
 *
 * Architecture:
 * - Static ingestWebhookEvent() persists events immediately (zero data loss)
 * - Background polling loop processes pending events in FIFO order
 * - Successful processing emits events via Events system: webhook:{source}:{eventType}
 * - Failed processing retries with exponential backoff (max 5 attempts)
 * - Events survive server restarts (database-backed durability)
 *
 * Usage:
 *   // Receive webhook (HTTP handler, CLI, etc.)
 *   await WebhookProcessor.ingestWebhookEvent('github', 'pull_request', payload);
 *
 *   // Subscribe to processed events
 *   Events.subscribe('webhook:github:pull_request', handlePR);
 *
 *   // Start background processing
 *   const processor = new WebhookProcessor();
 *   await processor.start();
 */

import { Events } from '../core/shared/Events';
import { ORM } from '../../daemons/data-daemon/server/ORM';
import { WebhookEventEntity } from '../data/entities/WebhookEventEntity';
import type { WebhookSource, WebhookEventType, ProcessingStatus } from '../data/entities/WebhookEventEntity';
import { generateUUID } from '../core/types/CrossPlatformUUID';
import type { UUID } from '../core/types/CrossPlatformUUID';
import { Logger } from '../core/logging/Logger';

const log = Logger.create('WebhookProcessor', 'webhook');

/** Poll interval for background processing (ms) */
const POLL_INTERVAL_MS = 5000;

/** Maximum processing attempts before giving up */
const MAX_ATTEMPTS = 5;

/** Base retry delay for exponential backoff (ms) */
const BASE_RETRY_DELAY_MS = 10_000;

/** Maximum events to process per poll cycle */
const BATCH_SIZE = 20;

/** Supported webhook sources that can be processed */
const SUPPORTED_SOURCES = new Set<string>(['github', 'gitlab', 'ci', 'custom']);

// ============================================================================
// Ingestion result
// ============================================================================

export interface WebhookIngestResult {
  success: boolean;
  eventId?: UUID;
  error?: string;
}

// ============================================================================
// WebhookProcessor
// ============================================================================

export class WebhookProcessor {
  private pollTimer: ReturnType<typeof setInterval> | null = null;
  private processing = false;

  /**
   * Ingest a raw webhook event — persists immediately for durability.
   * This is a static method so it can be called without a running processor.
   */
  static async ingestWebhookEvent(
    source: string,
    eventType: string,
    payload: Record<string, unknown>,
  ): Promise<WebhookIngestResult> {
    if (!source || !eventType) {
      return { success: false, error: 'source and eventType are required' };
    }

    const entity = new WebhookEventEntity();
    entity.id = generateUUID();
    entity.source = source as WebhookSource;
    entity.eventType = eventType as WebhookEventType;
    entity.payload = payload;
    entity.status = 'pending';
    entity.attempts = 0;
    entity.receivedAt = Date.now();
    entity.processed = false;

    const validation = entity.validate();
    if (!validation.success) {
      return { success: false, error: validation.error };
    }

    try {
      await ORM.store(WebhookEventEntity.collection, entity, false, 'default');
      log.info(`Ingested webhook: ${source}:${eventType} → ${entity.id}`);
      return { success: true, eventId: entity.id };
    } catch (err) {
      log.error(`Failed to ingest webhook: ${err}`);
      return { success: false, error: String(err) };
    }
  }

  /**
   * Start background polling for pending webhook events.
   */
  async start(): Promise<void> {
    if (this.pollTimer) return; // Already running

    log.info('Starting webhook processor');

    // Process immediately on start (recover unprocessed events)
    await this.processPendingEvents();

    // Then poll periodically
    this.pollTimer = setInterval(async () => {
      if (this.processing) return; // Skip if previous cycle still running
      await this.processPendingEvents();
    }, POLL_INTERVAL_MS);
  }

  /**
   * Stop the background processor.
   */
  stop(): void {
    if (this.pollTimer) {
      clearInterval(this.pollTimer);
      this.pollTimer = null;
    }
    log.info('Stopped webhook processor');
  }

  // ── Processing loop ───────────────────────────────────────────────────

  private async processPendingEvents(): Promise<void> {
    this.processing = true;

    try {
      // Query pending events ordered by receivedAt (FIFO)
      const pendingResult = await ORM.query<WebhookEventEntity>({
        collection: WebhookEventEntity.collection,
        filter: { status: 'pending' },
        sort: [{ field: 'receivedAt', direction: 'asc' }],
        limit: BATCH_SIZE,
      }, 'default');

      // Also query failed events that are ready for retry
      const retryResult = await ORM.query<WebhookEventEntity>({
        collection: WebhookEventEntity.collection,
        filter: { status: 'failed' },
        sort: [{ field: 'nextRetryAt', direction: 'asc' }],
        limit: BATCH_SIZE,
      }, 'default');

      const pending = pendingResult.data ?? [];
      const retryable = (retryResult.data ?? []).filter(r => {
        const event = r.data;
        return event.attempts < MAX_ATTEMPTS &&
          (!event.nextRetryAt || event.nextRetryAt <= Date.now());
      });

      const events = [...pending, ...retryable];
      if (events.length === 0) return;

      log.debug(`Processing ${events.length} webhook events (${pending.length} pending, ${retryable.length} retry)`);

      for (const record of events) {
        await this.processEvent(record.data);
      }
    } catch (err) {
      log.error(`Webhook processing cycle failed: ${err}`);
    } finally {
      this.processing = false;
    }
  }

  private async processEvent(event: WebhookEventEntity): Promise<void> {
    // Mark as processing
    event.status = 'processing';
    event.attempts++;
    await ORM.store(WebhookEventEntity.collection, event, false, 'default');

    try {
      // Validate source is supported
      if (!SUPPORTED_SOURCES.has(event.source)) {
        throw new Error(`Unsupported webhook source: ${event.source}`);
      }

      // Construct event path: webhook:{source}:{eventType}
      const eventPath = `webhook:${event.source}:${event.eventType}`;

      // Emit through the universal Events system
      Events.emit(eventPath, {
        eventId: event.id,
        source: event.source,
        eventType: event.eventType,
        payload: event.payload,
        receivedAt: event.receivedAt,
      });

      // Mark as completed
      event.status = 'completed';
      event.processed = true;
      event.processedAt = Date.now();
      await ORM.store(WebhookEventEntity.collection, event, false, 'default');

      log.info(`Processed webhook ${event.id}: ${eventPath}`);

    } catch (err) {
      // Mark as failed with retry scheduling
      event.status = 'failed';
      event.lastError = String(err);
      event.nextRetryAt = Date.now() + BASE_RETRY_DELAY_MS * Math.pow(2, event.attempts - 1);

      if (event.attempts >= MAX_ATTEMPTS) {
        log.error(`Webhook ${event.id} permanently failed after ${MAX_ATTEMPTS} attempts: ${err}`);
      } else {
        log.warn(`Webhook ${event.id} failed (attempt ${event.attempts}/${MAX_ATTEMPTS}): ${err}`);
      }

      await ORM.store(WebhookEventEntity.collection, event, false, 'default');
    }
  }
}
