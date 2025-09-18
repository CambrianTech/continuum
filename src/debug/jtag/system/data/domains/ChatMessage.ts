/**
 * ChatMessage Domain - Rich messaging with attachments and reactions
 * 
 * Professional chat system with Discord-like features
 * Supports text, attachments, reactions, threads, and formatting
 */

import type {
  MessageId,
  RoomId,
  UserId,
  ISOString,
  DataResult,
  DataError,
  BaseEntity
} from './CoreTypes';
import { generateUUID, type UUID } from '../../core/types/CrossPlatformUUID';
import { COLLECTIONS } from '../core/FieldMapping';

/**
 * Message Content Types
 */
export type MessageContentType = 'text' | 'image' | 'file' | 'code' | 'system' | 'embed';

/**
 * Message Status
 */
export type MessageStatus = 'sending' | 'sent' | 'delivered' | 'read' | 'failed' | 'deleted';

/**
 * Message Status Constants
 */
export const MESSAGE_STATUS = {
  SENDING: 'sending' as MessageStatus,
  SENT: 'sent' as MessageStatus,
  DELIVERED: 'delivered' as MessageStatus,
  READ: 'read' as MessageStatus,
  FAILED: 'failed' as MessageStatus,
  DELETED: 'deleted' as MessageStatus
} as const;

/**
 * Message Priority
 */
export type MessagePriority = 'low' | 'normal' | 'high' | 'urgent';

/**
 * Message Attachment
 */
export interface MessageAttachment {
  readonly id: string;
  readonly type: 'image' | 'file' | 'audio' | 'video';
  readonly filename: string;
  readonly size: number;
  readonly mimeType: string;
  readonly url: string;
  readonly thumbnailUrl?: string;
  readonly metadata?: Record<string, unknown>;
}

/**
 * Message Formatting
 */
export interface MessageFormatting {
  readonly markdown: boolean;
  readonly mentions: readonly UserId[];
  readonly hashtags: readonly string[];
  readonly links: readonly string[];
  readonly codeBlocks: readonly {
    readonly language?: string;
    readonly content: string;
  }[];
}

/**
 * Message Content
 */
export interface MessageContent {
  readonly text: string;
  readonly attachments: readonly MessageAttachment[];
  readonly formatting: MessageFormatting;
  readonly embeds?: readonly MessageEmbed[];
}

/**
 * Message Embed (rich content preview)
 */
export interface MessageEmbed {
  readonly type: 'link' | 'image' | 'video' | 'rich';
  readonly title?: string;
  readonly description?: string;
  readonly url?: string;
  readonly imageUrl?: string;
  readonly author?: string;
  readonly timestamp?: ISOString;
}

/**
 * Message Reaction
 */
export interface MessageReaction {
  readonly emoji: string;
  readonly count: number;
  readonly users: readonly UserId[];
  readonly addedAt: ISOString;
}

/**
 * Message Thread Info
 */
export interface MessageThread {
  readonly parentMessageId: MessageId;
  readonly replyCount: number;
  readonly lastReplyAt: ISOString;
  readonly participants: readonly UserId[];
}

/**
 * Message Metadata - Audit and context
 */
export interface MessageMetadata {
  readonly source: 'user' | 'system' | 'bot' | 'webhook';
  readonly deviceType?: 'web' | 'mobile' | 'desktop';
  readonly clientVersion?: string;
  readonly editHistory?: readonly {
    readonly editedAt: ISOString;
    readonly previousContent: string;
  }[];
  readonly deliveryReceipts?: readonly {
    readonly userId: UserId;
    readonly deliveredAt: ISOString;
    readonly readAt?: ISOString;
  }[];
}

/**
 * Chat Message Data - Pure data interface extending BaseEntity
 * Used by adapters for storage and retrieval from database
 */
export interface ChatMessageData extends BaseEntity {
  readonly messageId: MessageId;
  readonly roomId: RoomId;
  readonly senderId: UserId;
  readonly senderName: string;        // Cached for performance
  readonly content: MessageContent;
  readonly status: MessageStatus;
  readonly priority: MessagePriority;
  readonly timestamp: ISOString;
  readonly editedAt?: ISOString;
  readonly reactions: readonly MessageReaction[];
  readonly thread?: MessageThread;
  readonly replyToId?: MessageId;     // Direct reply (not thread)
  readonly metadata: MessageMetadata;
}

// ChatMessageData is a pure interface - no registry needed

/**
 * Message Creation Data
 */
export interface CreateMessageData {
  readonly roomId: RoomId;
  readonly senderId: UserId;
  readonly content: {
    readonly text: string;
    readonly attachments?: readonly MessageAttachment[];
    readonly formatting?: Partial<MessageFormatting>;
  };
  readonly priority?: MessagePriority;
  readonly mentions?: readonly UserId[];
  readonly replyToId?: MessageId;
  readonly metadata?: Partial<MessageMetadata>;
}

/**
 * Message Update Data (for editing)
 */
export interface UpdateMessageData {
  readonly content?: {
    readonly text?: string;
    readonly attachments?: readonly MessageAttachment[];
    readonly formatting?: Partial<MessageFormatting>;
  };
  readonly status?: MessageStatus;
}

/**
 * Message Query Filters
 */
export interface MessageQueryFilters {
  readonly roomId?: RoomId;
  readonly senderId?: UserId;
  readonly status?: MessageStatus;
  readonly priority?: MessagePriority;
  readonly contentType?: MessageContentType;
  readonly hasAttachments?: boolean;
  readonly hasReactions?: boolean;
  readonly isThread?: boolean;
  readonly mentionsUser?: UserId;
  readonly createdAfter?: ISOString;
  readonly createdBefore?: ISOString;
  readonly search?: string;         // Full-text search
}

/**
 * Default Message Formatting
 */
export const DEFAULT_MESSAGE_FORMATTING: MessageFormatting = {
  markdown: false,
  mentions: [],
  hashtags: [],
  links: [],
  codeBlocks: []
};

/**
 * Default Message Metadata
 */
export const DEFAULT_MESSAGE_METADATA: MessageMetadata = {
  source: 'user'
};

/**
 * Message Validation
 */
export function validateMessageData(data: CreateMessageData): DataResult<void, DataError> {
  if (!data.content.text || data.content.text.trim().length === 0) {
    return { success: false, error: { 
      type: 'VALIDATION_ERROR', 
      message: 'Message content cannot be empty',
      code: 'EMPTY_MESSAGE_CONTENT'
    }};
  }

  if (data.content.text.length > 4000) {
    return { success: false, error: { 
      type: 'VALIDATION_ERROR', 
      message: 'Message content cannot exceed 4000 characters',
      code: 'MESSAGE_TOO_LONG'
    }};
  }

  const attachmentCount = data.content.attachments?.length || 0;
  if (attachmentCount > 10) {
    return { success: false, error: { 
      type: 'VALIDATION_ERROR', 
      message: 'Cannot attach more than 10 files to a message',
      code: 'TOO_MANY_ATTACHMENTS'
    }};
  }

  return { success: true, data: undefined };
}

/**
 * Message Helper Functions
 */
export function extractMentions(text: string): UserId[] {
  const mentionPattern = /@(\w+)/g;
  const matches = text.match(mentionPattern);
  return matches ? matches.map(match => match.slice(1) as UserId) : [];
}

export function extractHashtags(text: string): string[] {
  const hashtagPattern = /#(\w+)/g;
  const matches = text.match(hashtagPattern);
  return matches ? matches.map(match => match.slice(1)) : [];
}

export function extractLinks(text: string): string[] {
  const linkPattern = /https?:\/\/[^\s]+/g;
  return text.match(linkPattern) || [];
}

export function extractCodeBlocks(text: string): MessageFormatting['codeBlocks'] {
  const codeBlockPattern = /```(\w+)?\n([\s\S]*?)```/g;
  const codeBlocks: { language?: string; content: string }[] = [];
  let match;

  while ((match = codeBlockPattern.exec(text)) !== null) {
    codeBlocks.push({
      language: match[1] || undefined,
      content: match[2]
    });
  }

  return codeBlocks;
}

export function processMessageFormatting(text: string): MessageFormatting {
  return {
    markdown: text.includes('**') || text.includes('*') || text.includes('`'),
    mentions: extractMentions(text),
    hashtags: extractHashtags(text),
    links: extractLinks(text),
    codeBlocks: extractCodeBlocks(text)
  };
}

export function isMessageEdited(message: ChatMessageData): boolean {
  return message.editedAt !== undefined;
}

export function getMessageDisplayTime(message: ChatMessageData): ISOString {
  return message.editedAt || message.timestamp;
}

export function canUserEditMessage(message: ChatMessageData, userId: UserId): boolean {
  // Users can edit their own messages within 24 hours
  const editWindow = 24 * 60 * 60 * 1000; // 24 hours in milliseconds
  const messageAge = Date.now() - new Date(message.timestamp).getTime();

  return message.senderId === userId &&
         messageAge < editWindow &&
         message.status !== 'deleted';
}

export function getMessageSummary(message: ChatMessageData): string {
  const content = message.content.text;
  const maxLength = 100;

  if (content.length <= maxLength) {
    return content;
  }

  return content.substring(0, maxLength - 3) + '...';
}