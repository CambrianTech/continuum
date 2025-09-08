/**
 * ChatMessage - Core domain object for chat messages
 * 
 * Immutable data class with factory methods and validation.
 * "typing like Rust - strict, explicit, and predictable"
 */

import { generateUUID, type UUID } from '../../system/core/types/CrossPlatformUUID';
import type { User } from '../user/User';
import type { ChatRoom } from './ChatRoom';

export interface ChatMessageData {
  readonly messageId: UUID;
  readonly roomId: string;
  readonly content: string;
  readonly senderId: UUID;
  readonly timestamp: string;
  readonly mentions: readonly UUID[];
  readonly category: 'chat' | 'system' | 'bot';
  readonly replyToId?: UUID;
  readonly editedAt?: string;
  readonly deletedAt?: string;
}

export class ChatMessage implements ChatMessageData {
  private constructor(private readonly data: ChatMessageData) {}

  get messageId(): UUID { return this.data.messageId; }
  get roomId(): string { return this.data.roomId; }
  get content(): string { return this.data.content; }
  get senderId(): UUID { return this.data.senderId; }
  get timestamp(): string { return this.data.timestamp; }
  get mentions(): readonly UUID[] { return this.data.mentions; }
  get category(): 'chat' | 'system' | 'bot' { return this.data.category; }
  get replyToId(): UUID | undefined { return this.data.replyToId; }
  get editedAt(): string | undefined { return this.data.editedAt; }
  get deletedAt(): string | undefined { return this.data.deletedAt; }

  /**
   * Create new chat message from user
   */
  static create(params: {
    readonly content: string;
    readonly user: User;
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
      timestamp: new Date().toISOString(),
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
      timestamp: new Date().toISOString(),
      mentions: [],
      category: 'system'
    });
  }

  /**
   * Reconstruct from stored data
   */
  static fromData(data: ChatMessageData): ChatMessage {
    return new ChatMessage(data);
  }

  /**
   * Get data for storage
   */
  toData(): ChatMessageData {
    return this.data;
  }

  /**
   * Edit message content
   */
  edit(newContent: string): ChatMessage {
    if (!newContent.trim()) {
      throw new Error('Message content cannot be empty');
    }

    return new ChatMessage({
      ...this.data,
      content: newContent.trim(),
      editedAt: new Date().toISOString()
    });
  }

  /**
   * Mark message as deleted
   */
  delete(): ChatMessage {
    return new ChatMessage({
      ...this.data,
      content: '[deleted]',
      deletedAt: new Date().toISOString()
    });
  }

  /**
   * Check if message is from user
   */
  isFromUser(user: User): boolean {
    return this.senderId === user.userId;
  }

  /**
   * Check if message mentions user
   */
  mentionsUser(user: User): boolean {
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