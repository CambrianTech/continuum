/**
 * Handle — Universal async operation reference
 *
 * A Handle is a persistent reference to any async operation in the system.
 * It can be resolved by either its full UUID or its 6-char short form.
 *
 * Design:
 *   - Short or long form: both resolve to the same operation
 *   - Persistent: survives restarts (backed by SQLite)
 *   - Universal: used by social, voice, inference, coding agents, proposals, etc.
 *   - Status-tracked: pending → processing → complete | failed | expired
 *
 * Usage:
 *   const handle = await Handles.create('social/feed', { sort: 'hot' }, requesterId);
 *   console.log(handle.shortId);  // "#a1b2c3"
 *
 *   // Later (even after restart):
 *   const result = await Handles.resolve('#a1b2c3');
 *   const result = await Handles.resolve('550e8400-e29b-41d4-a716-446655440000');
 */

import type { UUID } from './CrossPlatformUUID';
import type { ShortId } from './CrossPlatformUUID';

/**
 * HandleStatus lifecycle:
 *   pending → processing → complete
 *                        → failed
 *                        → expired (TTL exceeded)
 *   pending → cancelled (caller cancelled before processing)
 */
export type HandleStatus =
  | 'pending'      // Created, waiting for worker to pick up
  | 'processing'   // Worker is actively fulfilling
  | 'complete'     // Result available
  | 'failed'       // Operation failed (error stored)
  | 'expired'      // TTL exceeded before completion
  | 'cancelled';   // Cancelled by caller

/**
 * HandleRef — accepts short or long form for resolution.
 * Examples: "#a1b2c3", "a1b2c3", "550e8400-e29b-41d4-a716-446655440000"
 */
export type HandleRef = UUID | ShortId | string;

/**
 * HandleRecord — the full persisted state of a handle.
 * This is the shape stored in the database and returned by the service.
 */
export interface HandleRecord {
  /** Full UUID (the canonical identifier) */
  readonly id: UUID;

  /** Short form (last 6 hex chars) for human reference */
  readonly shortId: ShortId;

  /** Operation type (e.g., 'social/feed', 'voice/synthesize', 'ai/inference') */
  readonly type: string;

  /** Current lifecycle status */
  status: HandleStatus;

  /** Original request parameters (JSON-serializable) */
  readonly params: unknown;

  /** Result payload when status=complete (JSON-serializable) */
  result?: unknown;

  /** Error message when status=failed */
  error?: string;

  /** Who requested this operation */
  readonly requestedBy: UUID;

  /** When the handle was created */
  readonly createdAt: Date;

  /** When the status last changed */
  updatedAt: Date;

  /** When this handle expires (null = never) */
  expiresAt?: Date;

  /** How many times the worker retried this operation */
  retryCount: number;
}

/**
 * HandleCreateOptions — parameters for creating a new handle
 */
export interface HandleCreateOptions {
  /** Operation type (e.g., 'social/feed') */
  type: string;

  /** Request parameters */
  params: unknown;

  /** Who is requesting */
  requestedBy: UUID;

  /** TTL in milliseconds (null = never expires). Default: 5 minutes */
  ttlMs?: number | null;
}

/**
 * Default TTL for handles (5 minutes)
 */
export const DEFAULT_HANDLE_TTL_MS = 5 * 60 * 1000;
