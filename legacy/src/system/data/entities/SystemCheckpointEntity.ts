/**
 * SystemCheckpoint Entity - Cold start recovery for daemons
 *
 * Persists daemon processing state so they can resume from last checkpoint
 * after system restart or crash. Prevents data loss and duplicate processing.
 *
 * USAGE:
 * - Daemon saves checkpoint periodically (every N records or time interval)
 * - On startup, daemon loads last checkpoint and backfills missed data
 * - Checkpoint includes timestamp + optional ID for exact resume point
 *
 * EXAMPLES:
 * - TrainingDaemon: last processed chat message timestamp
 * - WebhookProcessor: inherently recovers (queries pending webhooks)
 * - PersonaUser: last serviced inbox timestamp
 */

import { BaseEntity } from './BaseEntity';
import { TextField, NumberField, JsonField } from '../decorators/FieldDecorators';

export class SystemCheckpointEntity extends BaseEntity {
  static readonly collection = 'system_checkpoints';

  get collection(): string {
    return SystemCheckpointEntity.collection;
  }

  /** Daemon name (training-daemon, webhook-processor, etc.) */
  @TextField()
  daemonName!: string;

  /** Last processed timestamp (milliseconds since epoch) */
  @NumberField()
  lastProcessedTimestamp!: number;

  /** Last processed entity ID (optional, for exact resume point) */
  @TextField()
  lastProcessedId?: string;

  /** Total items processed (lifetime counter) */
  @NumberField()
  totalProcessed!: number;

  /** Checkpoint creation timestamp */
  @NumberField()
  checkpointAt!: number;

  /** Daemon-specific metadata (flexible JSON) */
  @JsonField()
  metadata!: Record<string, unknown>;

  validate(): { success: boolean; error?: string } {
    if (!this.id) {
      return { success: false, error: 'Missing required field: id' };
    }

    if (!this.daemonName || typeof this.daemonName !== 'string') {
      return { success: false, error: 'Invalid daemonName' };
    }

    if (typeof this.lastProcessedTimestamp !== 'number' || this.lastProcessedTimestamp < 0) {
      return { success: false, error: 'Invalid lastProcessedTimestamp' };
    }

    if (typeof this.totalProcessed !== 'number' || this.totalProcessed < 0) {
      return { success: false, error: 'Invalid totalProcessed count' };
    }

    if (typeof this.checkpointAt !== 'number' || this.checkpointAt < 0) {
      return { success: false, error: 'Invalid checkpointAt timestamp' };
    }

    if (!this.metadata || typeof this.metadata !== 'object') {
      return { success: false, error: 'Invalid metadata' };
    }

    return { success: true };
  }
}
