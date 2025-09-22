/**
 * ChatMessage Entity - Decorated ChatMessageData for field extraction
 *
 * Uses field decorators to define storage requirements for the serde-style adapter system
 */

import type {
  MessageStatus,
  MessagePriority,
  MessageContent,
  MessageMetadata
} from '../domains/ChatMessage';
import type { MessageId, RoomId, UserId, ISOString } from '../domains/CoreTypes';
import {
  PrimaryField,
  TextField,
  DateField,
  EnumField,
  JsonField
} from '../decorators/FieldDecorators';
import { BaseEntityClass } from '../core/BaseEntityClass';

/**
 * Decorated ChatMessage Entity - Storage-aware version of ChatMessageData
 *
 * The decorators define which fields get extracted to dedicated columns
 * vs stored as JSON blobs for optimal query performance
 */
export class ChatMessageEntity extends BaseEntityClass {
  // Single source of truth for collection name - used by both decorators and commands
  static readonly collection = 'ChatMessage';

  @PrimaryField()
  messageId: MessageId;

  @TextField({ index: true })
  roomId: RoomId;

  @TextField({ index: true })
  senderId: UserId;

  @TextField()
  senderName: string;

  @JsonField()
  content: MessageContent;

  @EnumField()
  status: MessageStatus;

  @EnumField()
  priority: MessagePriority;

  @DateField({ index: true })
  timestamp: ISOString;

  @DateField({ nullable: true })
  editedAt?: ISOString;

  @JsonField()
  reactions: readonly any[];

  @JsonField({ nullable: true })
  thread?: any;

  @TextField({ nullable: true })
  replyToId?: MessageId;

  @JsonField()
  metadata: MessageMetadata;

  // BaseEntity inherited fields - initialized by super()
  declare id: string;
  declare createdAt: ISOString;
  declare updatedAt: ISOString;
  declare version: number;

  constructor() {
    super(); // Initialize BaseEntity fields

    // Default values
    this.messageId = '' as MessageId;
    this.roomId = '' as RoomId;
    this.senderId = '' as UserId;
    this.senderName = '';
    this.content = { text: '', attachments: [], formatting: { markdown: false, mentions: [], hashtags: [], links: [], codeBlocks: [] } };
    this.status = 'sending';
    this.priority = 'normal';
    this.timestamp = new Date().toISOString() as ISOString;
    this.reactions = [];
    this.metadata = { source: 'user' };
  }

}