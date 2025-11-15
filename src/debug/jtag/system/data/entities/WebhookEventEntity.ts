/**
 * WebhookEvent Entity - Durable queue for webhook events
 *
 * Persists webhook events (GitHub, CI/CD, etc.) to ensure zero data loss.
 * Events survive server restarts and are processed asynchronously.
 *
 * ARCHITECTURE:
 * - Webhook receiver writes event to database immediately
 * - Background processor reads unprocessed events
 * - Processing failures → retry with exponential backoff
 * - Successful processing → mark as processed
 *
 * GUARANTEES:
 * - At-least-once delivery (events never lost)
 * - Idempotent processing (safe to retry)
 * - Ordered processing within same source (GitHub PR events stay ordered)
 */

import { BaseEntity } from './BaseEntity';
import { TextField, NumberField, JsonField, BooleanField } from '../decorators/FieldDecorators';

export type WebhookSource = 'github' | 'gitlab' | 'ci' | 'custom';
export type WebhookEventType = 'pull_request' | 'issue' | 'push' | 'release' | 'ci_result' | 'custom';
export type ProcessingStatus = 'pending' | 'processing' | 'completed' | 'failed';

export class WebhookEventEntity extends BaseEntity {
  static readonly collection = 'webhook_events';

  get collection(): string {
    return WebhookEventEntity.collection;
  }

  /** Source of webhook (github, gitlab, ci, etc.) */
  @TextField()
  source!: WebhookSource;

  /** Event type (pull_request, issue, push, etc.) */
  @TextField()
  eventType!: WebhookEventType;

  /** Full webhook payload (JSON) */
  @JsonField()
  payload!: Record<string, unknown>;

  /** Processing status */
  @TextField()
  status!: ProcessingStatus;

  /** Number of processing attempts */
  @NumberField()
  attempts!: number;

  /** Last error message (if failed) */
  @TextField()
  lastError?: string;

  /** When event was received */
  @NumberField()
  receivedAt!: number;

  /** When event was processed (if completed) */
  @NumberField()
  processedAt?: number;

  /** Next retry time (if failed, for exponential backoff) */
  @NumberField()
  nextRetryAt?: number;

  /** Whether event has been processed successfully */
  @BooleanField()
  processed!: boolean;

  /** Target room ID (where to post message) */
  @TextField()
  targetRoomId?: string;

  /** Result of processing (message ID, error details, etc.) */
  @JsonField()
  processingResult?: Record<string, unknown>;

  validate(): { success: boolean; error?: string } {
    if (!this.id) {
      return { success: false, error: 'Missing required field: id' };
    }

    if (!this.source) {
      return { success: false, error: 'Missing required field: source' };
    }

    if (!this.eventType) {
      return { success: false, error: 'Missing required field: eventType' };
    }

    if (!this.payload || typeof this.payload !== 'object') {
      return { success: false, error: 'Missing or invalid field: payload' };
    }

    if (!this.status || !['pending', 'processing', 'completed', 'failed'].includes(this.status)) {
      return { success: false, error: 'Invalid status' };
    }

    if (typeof this.attempts !== 'number' || this.attempts < 0) {
      return { success: false, error: 'Invalid attempts count' };
    }

    if (typeof this.receivedAt !== 'number' || this.receivedAt < 0) {
      return { success: false, error: 'Invalid receivedAt timestamp' };
    }

    if (typeof this.processed !== 'boolean') {
      return { success: false, error: 'Invalid processed flag' };
    }

    return { success: true };
  }
}
