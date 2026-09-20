/**
 * CollaborativeOperationEntity - Base class for append-only operation logs
 *
 * This pattern enables real-time collaboration across any domain:
 * - Drawing canvas (strokes)
 * - CAD (shape operations)
 * - Games (moves)
 * - Document editing (insertions, deletions)
 * - Room design (furniture placement)
 *
 * Key properties:
 * - Append-only: Operations are never modified, only added
 * - Ordered: Timestamp provides total ordering
 * - Replayable: State is derived by replaying all operations
 * - Conflict-free: No merge conflicts, eventual consistency
 *
 * STORAGE ARCHITECTURE:
 * - **Metadata** (DB): operation log entries with indexes for querying
 * - **Content** (per-activity): large data stored in activity-specific storage
 *
 * The TMeta generic is for SMALL metadata that goes in DB (tool, color, bounds).
 * Large content (points arrays, media) should use contentRef to external storage.
 *
 * @example
 * // Define domain-specific metadata (small, queryable)
 * interface ChessMoveMeta {
 *   piece: 'pawn' | 'rook' | 'knight' | 'bishop' | 'queen' | 'king';
 *   notation: string;  // "e2-e4"
 * }
 *
 * // Create domain-specific entity
 * class ChessMoveEntity extends CollaborativeOperationEntity<ChessMoveMeta> {
 *   static collection = 'chess_moves';
 * }
 */

import { BaseEntity } from './BaseEntity';
import { TextField, DateField, JsonField } from '../decorators/FieldDecorators';
import type { UUID } from '../../core/types/CrossPlatformUUID';

/**
 * Base interface for operation metadata (small, stored in DB)
 * Subclasses define their own metadata shape
 */
export interface OperationMeta {
  [key: string]: unknown;
}

/**
 * Undo/redo tracking metadata
 */
export interface UndoRedoMeta {
  /** If operation was undone, when */
  undoneAt?: Date;
  /** If operation was undone, by whom */
  undoneBy?: UUID;
  /** If this operation undoes another, reference it */
  undoesOperationId?: UUID;
  /** Sequence number within activity (for ordering) */
  sequenceNumber?: number;
}

/**
 * Content storage location types
 */
export type ContentStorageType = 'inline' | 'file' | 'blob';

/**
 * Content reference for external storage
 */
export interface ContentRef {
  /** Storage type */
  type: ContentStorageType;
  /** Path or key to content (relative to activity storage) */
  path: string;
  /** Content size in bytes (for quota tracking) */
  sizeBytes?: number;
  /** Content MIME type */
  mimeType?: string;
}

/**
 * Abstract base class for collaborative operations
 *
 * STORAGE PATTERN:
 * - Common fields (activityId, creatorId, timestamp, opType) indexed in DB
 * - Small metadata (TMeta) stored in DB for querying
 * - Large content uses contentRef to per-activity storage
 *
 * @typeParam TMeta - Shape of small, queryable metadata (NOT large content)
 */
export abstract class CollaborativeOperationEntity<TMeta extends OperationMeta = OperationMeta> extends BaseEntity {

  // ─────────────────────────────────────────────────────────────────
  // INDEXED FIELDS (fast queries)
  // ─────────────────────────────────────────────────────────────────

  /**
   * Activity instance this operation belongs to
   * e.g., a specific canvas, game session, or document
   */
  @TextField({ index: true, summary: true })
  activityId!: UUID;

  /**
   * User who created this operation
   */
  @TextField({ index: true, summary: true })
  creatorId!: UUID;

  /**
   * Display name of creator (denormalized for efficiency)
   */
  @TextField({ summary: true })
  creatorName: string = 'Unknown';

  /**
   * When the operation was created
   * Used for ordering and replay
   */
  @DateField({ index: true, summary: true })
  timestamp!: Date;

  /**
   * Domain-specific operation type
   * e.g., 'stroke', 'move', 'addShape', 'delete'
   */
  @TextField({ index: true, summary: true })
  opType!: string;

  // ─────────────────────────────────────────────────────────────────
  // METADATA (small, stored in DB)
  // ─────────────────────────────────────────────────────────────────

  /**
   * Domain-specific operation metadata (SMALL data only!)
   * For canvas: { tool, color, size, pointCount, bounds }
   * For games: { piece, notation, capturedPiece }
   *
   * Large content (points arrays, images) should NOT go here.
   * Use contentRef instead.
   */
  @JsonField()
  meta!: TMeta;

  /**
   * Undo/redo tracking
   */
  @JsonField({ nullable: true })
  undoRedo?: UndoRedoMeta;

  // ─────────────────────────────────────────────────────────────────
  // CONTENT STORAGE (for large data)
  // ─────────────────────────────────────────────────────────────────

  /**
   * Reference to external content storage
   * Used for large data that shouldn't bloat the DB
   *
   * Content stored at: .continuum/activities/{activityId}/content/{operationId}.{ext}
   */
  @JsonField({ nullable: true })
  contentRef?: ContentRef;

  // ─────────────────────────────────────────────────────────────────
  // ACCESSORS
  // ─────────────────────────────────────────────────────────────────

  /**
   * Whether this operation has been undone
   */
  get isUndone(): boolean {
    return !!this.undoRedo?.undoneAt;
  }

  /**
   * Whether this operation has external content
   */
  get hasExternalContent(): boolean {
    return !!this.contentRef;
  }

  /**
   * Get content storage path (for subclass content loading)
   */
  getContentPath(basePath: string): string | null {
    if (!this.contentRef) return null;
    return `${basePath}/activities/${this.activityId}/content/${this.contentRef.path}`;
  }

  // ─────────────────────────────────────────────────────────────────
  // VALIDATION
  // ─────────────────────────────────────────────────────────────────

  /**
   * Validate the operation before saving
   * Subclasses should call super.validate() then add domain-specific checks
   */
  validate(): { success: boolean; error?: string } {
    if (!this.activityId) {
      return { success: false, error: 'activityId is required' };
    }
    if (!this.creatorId) {
      return { success: false, error: 'creatorId is required' };
    }
    if (!this.timestamp) {
      return { success: false, error: 'timestamp is required' };
    }
    if (!this.opType) {
      return { success: false, error: 'opType is required' };
    }
    return { success: true };
  }

  /**
   * Create a summary for logging/debugging
   */
  toSummary(): string {
    return `${this.opType} by ${this.creatorName} at ${this.timestamp.toISOString()}`;
  }
}

/**
 * Type helper for extracting operation metadata type from entity
 */
export type ExtractOperationMeta<T> = T extends CollaborativeOperationEntity<infer M> ? M : never;

// Backwards compatibility alias
export type OperationData = OperationMeta;
export type ExtractOperationData<T> = ExtractOperationMeta<T>;

/**
 * Standard operation types that many domains use
 */
export const STANDARD_OP_TYPES = {
  CREATE: 'create',
  UPDATE: 'update',
  DELETE: 'delete',
  MOVE: 'move',
  UNDO: 'undo',
  REDO: 'redo',
  CLEAR: 'clear',
} as const;

/**
 * Event names for collaborative operations
 * Pattern: {domain}:operation:{action}
 */
export const createOperationEventNames = (domain: string) => ({
  ADDED: `${domain}:operation:added`,
  UNDONE: `${domain}:operation:undone`,
  REDONE: `${domain}:operation:redone`,
  CLEARED: `${domain}:operation:cleared`,
} as const);
