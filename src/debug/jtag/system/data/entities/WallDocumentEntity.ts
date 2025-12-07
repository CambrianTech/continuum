/**
 * WallDocumentEntity - Room wall document metadata
 *
 * Stores metadata for collaborative markdown documents on room walls.
 * Actual file content is stored separately via file/save command.
 *
 * Two-layer storage:
 * - Database: This entity (metadata, stats, authorship)
 * - Filesystem: Markdown content via file/save/load commands
 */

import { BaseEntity } from './BaseEntity';
import { COLLECTIONS } from '../config/DatabaseConfig';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import { TextField, DateField, NumberField } from '../decorators/FieldDecorators';

/**
 * Protection levels for lease system (Phase 2)
 */
export type ProtectionLevel =
  | 'unrestricted'      // Anyone can write
  | 'peer_review'       // Requires peer AI approval
  | 'senior_review'     // Requires senior AI approval
  | 'human_review';     // Requires human approval

export class WallDocumentEntity extends BaseEntity {
  static readonly collection = COLLECTIONS.WALL_DOCUMENTS;

  // ============================================================================
  // SUMMARY FIELDS (included in data/list by default)
  // ============================================================================

  /**
   * Room this document belongs to
   */
  @TextField({ index: true, summary: true })
  roomId!: UUID;

  /**
   * Document name (e.g., "governance-framework.md")
   * This is the description field - used for toString representation
   */
  @TextField({
    index: true,
    summary: true,
    description: true,
    maxLength: 256
  })
  name!: string;

  /**
   * Original document creator
   */
  @TextField({ index: true, summary: true })
  createdBy!: UUID;

  /**
   * Last user to modify document
   */
  @TextField({ index: true, summary: true })
  lastModifiedBy!: UUID;

  /**
   * When document was last modified
   */
  @DateField({ index: true, summary: true })
  lastModifiedAt!: Date;

  /**
   * Total lines in document (cached for quick stats)
   */
  @NumberField({ summary: true })
  lineCount!: number;

  /**
   * Total bytes in document (cached for quick stats)
   */
  @NumberField({ summary: true })
  byteCount!: number;

  // ============================================================================
  // DETAIL FIELDS (NOT in summary - require explicit data/read)
  // ============================================================================

  /**
   * Absolute path to markdown file on filesystem
   * Format: .continuum/shared/rooms/{roomId}/{name}
   */
  @TextField()
  filePath!: string;

  /**
   * Git commit hash from last write (if git commands used)
   */
  @TextField({ nullable: true })
  lastCommitHash?: string;

  /**
   * Active lease ID if document is currently leased (Phase 2)
   */
  @TextField({ nullable: true, index: true })
  currentLeaseId?: UUID;

  /**
   * Protection level for lease system (Phase 2)
   * Determines who can acquire leases
   */
  @TextField({ nullable: true })
  protectionLevel?: ProtectionLevel;

  // ============================================================================
  // CONSTRUCTOR
  // ============================================================================

  constructor() {
    super();
    this.lastModifiedAt = new Date();
    this.lineCount = 0;
    this.byteCount = 0;
  }

  // ============================================================================
  // BASE ENTITY IMPLEMENTATION
  // ============================================================================

  /**
   * Implement BaseEntity abstract method
   */
  get collection(): string {
    return WallDocumentEntity.collection;
  }

  /**
   * Implement BaseEntity abstract method - validate document metadata
   */
  validate(): { success: boolean; error?: string } {
    // Required fields
    if (!this.roomId) {
      return { success: false, error: 'Wall document roomId is required' };
    }
    if (!this.name?.trim()) {
      return { success: false, error: 'Wall document name is required' };
    }
    if (!this.filePath?.trim()) {
      return { success: false, error: 'Wall document filePath is required' };
    }
    if (!this.createdBy) {
      return { success: false, error: 'Wall document createdBy is required' };
    }
    if (!this.lastModifiedBy) {
      return { success: false, error: 'Wall document lastModifiedBy is required' };
    }

    // Validate name format
    if (!this.name.endsWith('.md')) {
      return {
        success: false,
        error: 'Wall document name must end with .md'
      };
    }

    // Validate name doesn't contain path traversal
    if (this.name.includes('..') || this.name.includes('/') || this.name.includes('\\')) {
      return {
        success: false,
        error: 'Wall document name must not contain path separators or parent directory references'
      };
    }

    // Validate protection level if set
    if (this.protectionLevel) {
      const validLevels: ProtectionLevel[] = [
        'unrestricted',
        'peer_review',
        'senior_review',
        'human_review'
      ];
      if (!validLevels.includes(this.protectionLevel)) {
        return {
          success: false,
          error: `Wall document protectionLevel must be one of: ${validLevels.join(', ')}`
        };
      }
    }

    return { success: true };
  }

  // ============================================================================
  // UTILITY METHODS
  // ============================================================================

  /**
   * Check if document is currently leased (Phase 2)
   */
  isLeased(): boolean {
    return !!this.currentLeaseId;
  }

  /**
   * Get human-readable size
   */
  getHumanReadableSize(): string {
    if (this.byteCount < 1024) {
      return `${this.byteCount} bytes`;
    } else if (this.byteCount < 1024 * 1024) {
      return `${(this.byteCount / 1024).toFixed(1)} KB`;
    } else {
      return `${(this.byteCount / (1024 * 1024)).toFixed(1)} MB`;
    }
  }

  /**
   * Get summary stats for display
   */
  getStats(): string {
    return `${this.lineCount} lines, ${this.getHumanReadableSize()}`;
  }

  /**
   * Pagination config - show recently modified first
   */
  static getPaginationConfig(): {
    defaultSortField: string;
    defaultSortDirection: 'asc' | 'desc';
    defaultPageSize: number;
    cursorField: string;
  } {
    return {
      defaultSortField: 'lastModifiedAt',
      defaultSortDirection: 'desc',  // Newest changes first
      defaultPageSize: 50,
      cursorField: 'lastModifiedAt'
    };
  }
}
