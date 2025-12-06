/**
 * ChatMessage Entity - Decorated ChatMessageData for field extraction
 *
 * Uses field decorators to define storage requirements for the serde-style adapter system
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { UserType } from './UserEntity';

// Message-specific types moved here since domain files are deleted
export type MessageStatus = 'sending' | 'sent' | 'delivered' | 'read' | 'failed' | 'deleted';
export type MessagePriority = 'low' | 'normal' | 'high' | 'urgent';

/**
 * Media Types - Extensible media structure for multimodal messages
 */
export type MediaType = 'image' | 'audio' | 'video' | 'file' | 'document';

export interface MediaItem {
  // Core identification
  id?: string;  // Unique ID for this media item
  type: MediaType;

  // Content sources (at least one required)
  url?: string;        // URL to media (local file:// or remote https://)
  base64?: string;     // Base64-encoded data

  // Metadata
  mimeType?: string;   // e.g., 'image/png', 'audio/mp3'
  filename?: string;
  size?: number;       // Bytes

  // Accessibility & Description
  alt?: string;        // Alt text for images (accessibility)
  description?: string; // Human-readable description of media
  title?: string;      // Optional title

  // Dimensions (for images/videos)
  width?: number;
  height?: number;
  duration?: number;   // Duration in seconds (for audio/video)

  // Analysis & Processing
  analysisCacheKey?: string;  // Reference to cached AI analysis
  thumbnailUrl?: string;      // Thumbnail for videos/documents

  // Upload tracking
  uploadedAt?: number;  // Timestamp
  uploadedBy?: UUID;    // Who uploaded it
}

export interface MessageContent {
  text: string;
  media?: readonly MediaItem[];  // Renamed from 'attachments' for clarity
}

export interface EditHistoryEntry {
  editedAt: Date;
  previousText: string;
}

export interface DeliveryReceipt {
  userId: string;
  deliveredAt: Date;
}

export interface MessageMetadata {
  source: 'user' | 'system' | 'bot' | 'webhook';
  deviceType?: string;
  clientVersion?: string;
  editHistory?: readonly EditHistoryEntry[];
  deliveryReceipts?: readonly DeliveryReceipt[];
  resolved?: boolean;  // Mark message as resolved (no further AI responses needed)
  resolvedBy?: UUID;   // Who marked it resolved (moderator)
  resolvedAt?: number; // When it was marked resolved
  isSystemTest?: boolean;  // Mark as system test (precommit hook, integration test) - AIs should skip
  testType?: string;  // Type of test (e.g., 'precommit-hook', 'integration-test')

  // Phase 3B: Tool result storage in working memory
  toolResult?: boolean;  // Flag that this message is a tool result
  toolName?: string;  // Name of the tool executed
  parameters?: Record<string, unknown>;  // Tool parameters
  fullData?: unknown;  // Full tool result data (stored, not in RAG)
  success?: boolean;  // Whether tool execution succeeded
  error?: string;  // Error message if tool failed
  storedAt?: number;  // Timestamp when tool result was stored
}

export interface MessageReaction {
  emoji: string;
  userId: string;
  timestamp: Date;
}

export interface MessageThread {
  threadId: UUID;
  replyCount: number;
  lastReplyAt: Date;
}

// Constants and utility functions that were in the domain file
export const MESSAGE_STATUS = {
  SENDING: 'sending' as const,
  SENT: 'sent' as const,
  DELIVERED: 'delivered' as const,
  READ: 'read' as const,
  FAILED: 'failed' as const,
  DELETED: 'deleted' as const,
} as const;

// Data creation interface for new messages
export interface CreateMessageData {
  roomId: UUID;
  senderId: UUID;
  senderName: string;
  senderType: UserType; // Denormalized for performance (avoid extra DB lookup)
  content: MessageContent;
  priority?: MessagePriority;
  metadata?: Partial<MessageMetadata>;
  replyToId?: UUID;
}

// Simple content processing - no formatting complexity
export function processMessageFormatting(content: MessageContent): MessageContent {
  return {
    text: content.text,
    media: content.media
  };
}

import {
  TextField,
  DateField,
  EnumField,
  JsonField
} from '../decorators/FieldDecorators';
import { BaseEntity } from './BaseEntity';

/**
 * Decorated ChatMessage Entity - Storage-aware version of ChatMessageData
 *
 * The decorators define which fields get extracted to dedicated columns
 * vs stored as JSON blobs for optimal query performance
 */
export class ChatMessageEntity extends BaseEntity {
  // Single source of truth for collection name - used by both decorators and commands
  static readonly collection = 'chat_messages';

  @TextField({ index: true })
  roomId: UUID;

  @TextField({ index: true })
  senderId: UUID;

  @TextField({ description: true })
  senderName: string;

  @EnumField({ index: true }) // Index for efficient AI vs human filtering
  senderType: UserType;

  @JsonField()
  content: MessageContent;

  @EnumField()
  status: MessageStatus;

  @EnumField()
  priority: MessagePriority;

  @DateField({ index: true })
  timestamp: Date;

  @DateField({ nullable: true })
  editedAt?: Date;

  @JsonField()
  reactions: readonly MessageReaction[];

  @JsonField({ nullable: true })
  thread?: MessageThread;

  @TextField({ nullable: true })
  replyToId?: UUID;

  @JsonField({ nullable: true })
  metadata?: Partial<MessageMetadata>;

  constructor() {
    super(); // Initialize BaseEntity fields (id, createdAt, updatedAt, version)

    // Default values - id autogenerated by BaseEntity
    this.roomId = '' as UUID;
    this.senderId = '' as UUID;
    this.senderName = '';
    this.senderType = 'human'; // Default to human
    this.content = { text: '', media: [] };
    this.status = 'sending';
    this.priority = 'normal';
    this.timestamp = new Date();
    this.reactions = [];
  }

  /**
   * Implement BaseEntity abstract method
   */
  get collection(): string {
    return ChatMessageEntity.collection;
  }

  /**
   * Override BaseEntity pagination config - use timestamp for chat messages
   * Chat messages are sorted by timestamp DESC (newest first)
   * DataDaemon will use these defaults when opening query handles
   */
  static override getPaginationConfig(): {
    defaultSortField: string;
    defaultSortDirection: 'asc' | 'desc';
    defaultPageSize: number;
    cursorField: string;
  } {
    return {
      defaultSortField: 'timestamp',
      defaultSortDirection: 'desc' as const,
      defaultPageSize: 30,
      cursorField: 'timestamp'
    };
  }


  /**
   * Implement BaseEntity abstract method - validate message data
   */
  validate(): { success: boolean; error?: string } {
    // Required fields validation
    if (!this.roomId?.trim()) {
      return { success: false, error: 'Message roomId is required' };
    }

    if (!this.senderId?.trim()) {
      return { success: false, error: 'Message senderId is required' };
    }

    if (!this.senderName?.trim()) {
      return { success: false, error: 'Message senderName is required' };
    }

    if (!this.content) {
      return { success: false, error: 'Message content is required' };
    }

    if (!this.content.text?.trim() && (!this.content.media || this.content.media.length === 0)) {
      return { success: false, error: 'Message must have either text content or media' };
    }

    // Validate media items if present
    if (this.content.media && this.content.media.length > 0) {
      for (const mediaItem of this.content.media) {
        if (!mediaItem.url && !mediaItem.base64) {
          return { success: false, error: 'Media item must have either url or base64 data' };
        }
        if (!mediaItem.type) {
          return { success: false, error: 'Media item must have a type' };
        }
      }
    }

    // Enum validation
    const validStatuses: MessageStatus[] = ['sending', 'sent', 'delivered', 'read', 'failed', 'deleted'];
    if (!validStatuses.includes(this.status)) {
      return { success: false, error: `Message status must be one of: ${validStatuses.join(', ')}` };
    }

    const validPriorities: MessagePriority[] = ['low', 'normal', 'high', 'urgent'];
    if (!validPriorities.includes(this.priority)) {
      return { success: false, error: `Message priority must be one of: ${validPriorities.join(', ')}` };
    }

    // Date validation - serde-like graceful conversion
    if (!this.isValidDate(this.timestamp)) {
      return { success: false, error: 'Message timestamp must be a valid Date or ISO date string' };
    }

    return { success: true };
  }

}