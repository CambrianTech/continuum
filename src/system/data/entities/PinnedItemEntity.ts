/**
 * PinnedItemEntity - Post-it note style reminders for rooms
 *
 * Pins are lightweight highlights/reminders that can reference:
 * - Chat messages
 * - Memories
 * - Tasks
 * - Or contain their own content
 *
 * Interoperable with MemoryEntity and TaskEntity:
 * - Memory → Pin: Highlight important insight for room visibility
 * - Task → Pin: Reminder about current work
 * - Pin → Memory: Promote important pin to persistent memory
 * - Pin → Task: Convert reminder into actionable work
 */

import { BaseEntity } from './BaseEntity';
import { COLLECTIONS } from '../config/DatabaseConfig';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import { TextField, DateField, JsonField } from '../decorators/FieldDecorators';

/**
 * What type of content this pin references
 */
export type PinType =
  | 'message'      // References a chat message
  | 'memory'       // References a memory
  | 'task'         // References a task
  | 'standalone';  // Self-contained content

/**
 * Optional category for organizing pins
 */
export type PinCategory =
  | 'decision'     // Important decision
  | 'blocker'      // Something blocking progress
  | 'resource'     // Useful resource/link
  | 'reminder'     // General reminder
  | 'question';    // Open question

export class PinnedItemEntity extends BaseEntity {
  static readonly collection = COLLECTIONS.PINNED_ITEMS;

  // What room this pin belongs to
  @TextField({ index: true })
  roomId!: UUID;

  // Who pinned it
  @TextField({ index: true })
  pinnedBy!: UUID;

  @DateField({ index: true })
  pinnedAt!: Date;

  // What this pin references (if anything)
  @TextField({ index: true })
  pinType!: PinType;

  // Reference to original content (message/memory/task ID)
  @TextField({ index: true, nullable: true })
  referencedId?: UUID;

  // Pin content - short and long form
  @TextField()
  title!: string;                    // Self-explanatory title (required)

  @TextField({ nullable: true })
  shortDescription?: string;          // Brief description (optional)

  @TextField({ nullable: true })
  longDescription?: string;           // Detailed description (optional)

  // Bullets for content (tree depth = 1)
  @JsonField({ nullable: true })
  bullets?: string[];                 // Simple flat list of bullet points

  // Optional categorization
  @TextField({ nullable: true })
  category?: PinCategory;

  // Optional note about why this was pinned
  @TextField({ nullable: true })
  note?: string;

  // Expiration (optional)
  @DateField({ nullable: true })
  expiresAt?: Date;                   // Auto-unpin after this date

  // Ordering
  @TextField({ nullable: true })
  orderKey?: string;                  // For manual ordering within room

  constructor() {
    super();
    this.pinnedAt = new Date();
    this.pinType = 'standalone';
  }

  /**
   * Implement BaseEntity abstract method
   */
  get collection(): string {
    return PinnedItemEntity.collection;
  }

  /**
   * Implement BaseEntity abstract method - validate pin data
   */
  validate(): { success: boolean; error?: string } {
    // Required fields
    if (!this.roomId) {
      return { success: false, error: 'Pin roomId is required' };
    }
    if (!this.pinnedBy) {
      return { success: false, error: 'Pin pinnedBy is required' };
    }
    if (!this.title?.trim()) {
      return { success: false, error: 'Pin title is required' };
    }
    if (!this.pinType) {
      return { success: false, error: 'Pin pinType is required' };
    }

    // If referencing something, must have referencedId
    if (this.pinType !== 'standalone' && !this.referencedId) {
      return {
        success: false,
        error: `Pin of type '${this.pinType}' must have referencedId`
      };
    }

    // Validate pinType enum
    const validTypes: PinType[] = ['message', 'memory', 'task', 'standalone'];
    if (!validTypes.includes(this.pinType)) {
      return {
        success: false,
        error: `Pin pinType must be one of: ${validTypes.join(', ')}`
      };
    }

    // Validate category if present
    if (this.category) {
      const validCategories: PinCategory[] = ['decision', 'blocker', 'resource', 'reminder', 'question'];
      if (!validCategories.includes(this.category)) {
        return {
          success: false,
          error: `Pin category must be one of: ${validCategories.join(', ')}`
        };
      }
    }

    return { success: true };
  }

  /**
   * Check if pin is expired
   */
  isExpired(): boolean {
    if (!this.expiresAt) return false;
    return this.expiresAt.getTime() < Date.now();
  }

  /**
   * Get display content for UI
   */
  getDisplayContent(): string {
    let content = this.title;

    if (this.shortDescription) {
      content += `\n${this.shortDescription}`;
    }

    if (this.bullets && this.bullets.length > 0) {
      content += '\n\n';
      for (const bullet of this.bullets) {
        content += `• ${bullet}\n`;
      }
    }

    if (this.note) {
      content += `\nNote: ${this.note}`;
    }

    return content;
  }

  /**
   * Convert to Memory for long-term storage
   * Useful when a pin becomes permanently important
   */
  toMemoryData(): {
    content: string;
    scope: 'personal' | 'task' | 'project' | 'team' | 'global';
    tags: string[];
    metadata: Record<string, unknown>;
  } {
    return {
      content: this.getDisplayContent(),
      scope: 'task', // Pins are typically task-scoped
      tags: [
        'pin',
        this.pinType,
        ...(this.category ? [this.category] : [])
      ],
      metadata: {
        originalPinId: this.id,
        roomId: this.roomId,
        pinnedAt: this.pinnedAt.toISOString(),
        pinnedBy: this.pinnedBy,
        referencedId: this.referencedId
      }
    };
  }

  /**
   * Convert to Task for actionable work
   * Useful when a reminder becomes a concrete task
   */
  toTaskData(): {
    description: string;
    domain: 'chat' | 'code' | 'game' | 'academy' | 'analysis' | 'self';
    contextId: UUID;
    metadata: Record<string, unknown>;
  } {
    return {
      description: this.getDisplayContent(),
      domain: 'chat', // Pins come from chat rooms
      contextId: this.roomId, // Use room as context
      metadata: {
        originalPinId: this.id,
        pinnedAt: this.pinnedAt.toISOString(),
        pinnedBy: this.pinnedBy,
        referencedId: this.referencedId
      }
    };
  }

  /**
   * Pagination config - show newest pins first
   */
  static getPaginationConfig(): {
    defaultSortField: string;
    defaultSortDirection: 'asc' | 'desc';
    defaultPageSize: number;
    cursorField: string;
  } {
    return {
      defaultSortField: 'pinnedAt',
      defaultSortDirection: 'desc', // Newest pins first
      defaultPageSize: 50,
      cursorField: 'pinnedAt'
    };
  }
}
