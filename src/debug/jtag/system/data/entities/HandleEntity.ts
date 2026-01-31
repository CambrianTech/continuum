/**
 * HandleEntity — Persistent async operation handle
 *
 * Every async operation in the system creates a HandleEntity.
 * The entity ID IS the handle. Resolvable by full UUID or shortId.
 *
 * Persists to SQLite via DataDaemon — survives restarts.
 * Workers query for pending handles on startup and resume processing.
 */

import { BaseEntity } from './BaseEntity';
import { COLLECTIONS } from '../../shared/Constants';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { HandleStatus } from '../../core/types/Handle';
import { DEFAULT_HANDLE_TTL_MS } from '../../core/types/Handle';
import { TextField, EnumField, DateField, JsonField, NumberField } from '../decorators/FieldDecorators';

export class HandleEntity extends BaseEntity {
  static readonly collection = COLLECTIONS.HANDLES;

  /** Operation type (e.g., 'social/feed', 'voice/synthesize', 'ai/inference') */
  @TextField({ index: true })
  type!: string;

  /** Current lifecycle status */
  @EnumField({ index: true })
  status!: HandleStatus;

  /** Original request parameters */
  @JsonField()
  params!: unknown;

  /** Result payload when complete */
  @JsonField({ nullable: true })
  result?: unknown;

  /** Error message when failed */
  @TextField({ nullable: true })
  error?: string;

  /** Who requested this operation */
  @TextField({ index: true })
  requestedBy!: UUID;

  /** When this handle expires */
  @DateField({ nullable: true })
  expiresAt?: Date;

  /** Retry count */
  @NumberField()
  retryCount!: number;

  constructor() {
    super();
    this.status = 'pending';
    this.retryCount = 0;
  }

  get collection(): string {
    return HandleEntity.collection;
  }

  validate(): { success: boolean; error?: string } {
    if (!this.type?.trim()) {
      return { success: false, error: 'Handle type is required' };
    }
    if (!this.requestedBy) {
      return { success: false, error: 'Handle requestedBy is required' };
    }
    const validStatuses: HandleStatus[] = ['pending', 'processing', 'complete', 'failed', 'expired', 'cancelled'];
    if (!validStatuses.includes(this.status)) {
      return { success: false, error: `Handle status must be one of: ${validStatuses.join(', ')}` };
    }
    return { success: true };
  }

  /** Check if this handle has reached a terminal state */
  get isTerminal(): boolean {
    return this.status === 'complete' || this.status === 'failed' || this.status === 'expired' || this.status === 'cancelled';
  }

  /** Check if this handle is still active (can be worked on) */
  get isActive(): boolean {
    return this.status === 'pending' || this.status === 'processing';
  }

  /** Check if this handle has expired based on its TTL */
  get isExpired(): boolean {
    if (!this.expiresAt) return false;
    return new Date() > this.expiresAt;
  }

  /** Mark as processing */
  markProcessing(): void {
    this.status = 'processing';
    this.updatedAt = new Date();
  }

  /** Mark as complete with result */
  markComplete(result: unknown): void {
    this.status = 'complete';
    this.result = result;
    this.updatedAt = new Date();
  }

  /** Mark as failed with error */
  markFailed(error: string): void {
    this.status = 'failed';
    this.error = error;
    this.retryCount++;
    this.updatedAt = new Date();
  }

  /** Mark as expired */
  markExpired(): void {
    this.status = 'expired';
    this.updatedAt = new Date();
  }

  /** Mark as cancelled */
  markCancelled(): void {
    this.status = 'cancelled';
    this.updatedAt = new Date();
  }

  /** Create a handle with standard defaults */
  static createHandle(
    type: string,
    params: unknown,
    requestedBy: UUID,
    ttlMs: number | null = DEFAULT_HANDLE_TTL_MS,
  ): HandleEntity {
    const entity = new HandleEntity();
    entity.type = type;
    entity.params = params;
    entity.requestedBy = requestedBy;
    if (ttlMs !== null) {
      entity.expiresAt = new Date(Date.now() + ttlMs);
    }
    return entity;
  }

  static getPaginationConfig() {
    return {
      defaultSortField: 'createdAt',
      defaultSortDirection: 'desc' as const,
      defaultPageSize: 50,
      cursorField: 'createdAt',
    };
  }
}
