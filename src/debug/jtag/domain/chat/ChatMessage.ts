/**
 * ChatMessage - Core domain object for chat messages
 * 
 * Immutable data class with factory methods and validation.
 * "typing like Rust - strict, explicit, and predictable"
 */

import { generateUUID, type UUID } from '../../system/core/types/CrossPlatformUUID';
import type { BaseUser } from '../user/BaseUser';
import type { ChatRoom } from './ChatRoom';

export class ChatMessage {
  readonly messageId: UUID;
  readonly roomId: string;
  readonly content: string;
  readonly senderId: UUID;
  readonly createdAt: Date;
  readonly mentions: readonly UUID[];
  readonly category: 'chat' | 'system' | 'bot';
  readonly replyToId?: UUID;
  readonly editedAt?: Date;
  readonly deletedAt?: Date;

  constructor(data: {
    messageId: UUID;
    roomId: string;
    content: string;
    senderId: UUID;
    createdAt: Date;
    mentions: readonly UUID[];
    category: 'chat' | 'system' | 'bot';
    replyToId?: UUID;
    editedAt?: Date;
    deletedAt?: Date;
  }) {
    this.messageId = data.messageId;
    this.roomId = data.roomId;
    this.content = data.content;
    this.senderId = data.senderId;
    this.createdAt = data.createdAt;
    this.mentions = data.mentions;
    this.category = data.category;
    this.replyToId = data.replyToId;
    this.editedAt = data.editedAt;
    this.deletedAt = data.deletedAt;
  }

  get timestamp(): Date { return this.createdAt; } // Alias for compatibility

  /**
   * Create new chat message from user
   */
  static create(params: {
    readonly content: string;
    readonly user: BaseUser;
    readonly room: ChatRoom;
    readonly mentions?: readonly UUID[];
    readonly replyToId?: UUID;
  }): ChatMessage {
    if (!params.content.trim()) {
      throw new Error('Message content cannot be empty');
    }

    return new ChatMessage({
      messageId: generateUUID(),
      roomId: params.room.roomId,
      content: params.content.trim(),
      senderId: params.user.userId,
      createdAt: new Date(),
      mentions: params.mentions || [],
      category: 'chat',
      replyToId: params.replyToId
    });
  }
  
  /**
   * Create system message
   */
  static createSystem(params: {
    readonly content: string;
    readonly room: ChatRoom;
  }): ChatMessage {
    if (!params.content.trim()) {
      throw new Error('System message content cannot be empty');
    }

    return new ChatMessage({
      messageId: generateUUID(),
      roomId: params.room.roomId,
      content: params.content.trim(),
      senderId: generateUUID(), // System messages get unique sender ID
      createdAt: new Date(),
      mentions: [],
      category: 'system'
    });
  }

  /**
   * Reconstruct from stored data (handles string to Date conversion)
   */
  static fromData(record: { id: string; createdAt: string; content: { text: string }; senderId: string; roomId: string; mentions?: string[]; editedAt?: string; deletedAt?: string }): ChatMessage {
    return new ChatMessage({
      messageId: record.id as UUID,
      roomId: record.roomId,
      content: record.content.text,
      senderId: record.senderId as UUID,
      createdAt: new Date(record.createdAt),
      mentions: (record.mentions || []) as readonly UUID[],
      category: 'chat',
      editedAt: record.editedAt ? new Date(record.editedAt) : undefined,
      deletedAt: record.deletedAt ? new Date(record.deletedAt) : undefined,
    });
  }

  /**
   * Get data for storage (converts Dates to ISO strings)
   */
  toData(): { id: string; createdAt: string; content: { text: string }; senderId: string; roomId: string; mentions: string[]; editedAt?: string; deletedAt?: string } {
    return {
      id: this.messageId,
      createdAt: this.createdAt.toISOString(),
      content: { text: this.content },
      senderId: this.senderId,
      roomId: this.roomId,
      mentions: [...this.mentions],
      editedAt: this.editedAt?.toISOString(),
      deletedAt: this.deletedAt?.toISOString(),
    };
  }

  /**
   * Edit message content
   */
  edit(newContent: string): ChatMessage {
    if (!newContent.trim()) {
      throw new Error('Message content cannot be empty');
    }

    return new ChatMessage({
      messageId: this.messageId,
      roomId: this.roomId,
      content: newContent.trim(),
      senderId: this.senderId,
      createdAt: this.createdAt,
      mentions: this.mentions,
      category: this.category,
      replyToId: this.replyToId,
      editedAt: new Date(),
      deletedAt: this.deletedAt
    });
  }

  /**
   * Mark message as deleted
   */
  delete(): ChatMessage {
    return new ChatMessage({
      messageId: this.messageId,
      roomId: this.roomId,
      content: '[deleted]',
      senderId: this.senderId,
      createdAt: this.createdAt,
      mentions: this.mentions,
      category: this.category,
      replyToId: this.replyToId,
      editedAt: this.editedAt,
      deletedAt: new Date()
    });
  }

  /**
   * Check if message is from user
   */
  isFromUser(user: BaseUser): boolean {
    return this.senderId === user.userId;
  }

  /**
   * Check if message mentions user
   */
  mentionsUser(user: BaseUser): boolean {
    return this.mentions.includes(user.userId);
  }

  /**
   * Check if message is deleted
   */
  isDeleted(): boolean {
    return !!this.deletedAt;
  }

  /**
   * Check if message is edited
   */
  isEdited(): boolean {
    return !!this.editedAt;
  }

  /**
   * Get display text for UI
   */
  getDisplayContent(): string {
    if (this.isDeleted()) {
      return '[deleted]';
    }
    return this.content;
  }

  toString(): string {
    return `ChatMessage(${this.messageId}, room=${this.roomId}, content="${this.content.substring(0, 50)}...")`;
  }
}