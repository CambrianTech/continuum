/**
 * ChatRoom - Core domain object for chat rooms
 * 
 * Immutable data class with factory methods and validation.
 * "typing like Rust - strict, explicit, and predictable"
 */

import { generateUUID, type UUID } from '../../system/core/types/CrossPlatformUUID';

export interface ChatRoomData {
  readonly roomId: UUID;
  readonly name: string;
  readonly description?: string;
  readonly roomType: 'public' | 'private' | 'direct' | 'system';
  readonly createdBy: UUID;
  readonly createdAt: string;
  readonly lastActivityAt: string;
  readonly memberIds: readonly UUID[];
  readonly isArchived: boolean;
  readonly settings: Record<string, unknown>;
}

export class ChatRoom implements ChatRoomData {
  private constructor(private readonly data: ChatRoomData) {}

  get roomId(): UUID { return this.data.roomId; }
  get name(): string { return this.data.name; }
  get description(): string | undefined { return this.data.description; }
  get roomType(): 'public' | 'private' | 'direct' | 'system' { return this.data.roomType; }
  get createdBy(): UUID { return this.data.createdBy; }
  get createdAt(): string { return this.data.createdAt; }
  get lastActivityAt(): string { return this.data.lastActivityAt; }
  get memberIds(): readonly UUID[] { return this.data.memberIds; }
  get isArchived(): boolean { return this.data.isArchived; }
  get settings(): Record<string, unknown> { return this.data.settings; }

  /**
   * Reconstruct from stored data
   */
  static fromData(data: ChatRoomData): ChatRoom {
    return new ChatRoom(data);
  }

  /**
   * Get data for storage
   */
  toData(): ChatRoomData {
    return this.data;
  }

  /**
   * Update room name
   */
  updateName(name: string): ChatRoom {
    if (!name.trim()) {
      throw new Error('Room name cannot be empty');
    }

    return new ChatRoom({
      ...this.data,
      name: name.trim()
    });
  }

  /**
   * Update description
   */
  updateDescription(description: string): ChatRoom {
    return new ChatRoom({
      ...this.data,
      description: description.trim() || undefined
    });
  }

  /**
   * Add member to room
   */
  addMember(userId: UUID): ChatRoom {
    if (this.memberIds.includes(userId)) {
      return this; // Already a member
    }

    return new ChatRoom({
      ...this.data,
      memberIds: [...this.memberIds, userId],
      lastActivityAt: new Date().toISOString()
    });
  }

  /**
   * Remove member from room
   */
  removeMember(userId: UUID): ChatRoom {
    const newMemberIds = this.memberIds.filter(id => id !== userId);
    
    if (newMemberIds.length === this.memberIds.length) {
      return this; // Member wasn't in room
    }

    return new ChatRoom({
      ...this.data,
      memberIds: newMemberIds,
      lastActivityAt: new Date().toISOString()
    });
  }

  /**
   * Archive room
   */
  archive(): ChatRoom {
    return new ChatRoom({
      ...this.data,
      isArchived: true,
      lastActivityAt: new Date().toISOString()
    });
  }

  /**
   * Unarchive room
   */
  unarchive(): ChatRoom {
    return new ChatRoom({
      ...this.data,
      isArchived: false,
      lastActivityAt: new Date().toISOString()
    });
  }

  /**
   * Update activity timestamp
   */
  updateActivity(): ChatRoom {
    return new ChatRoom({
      ...this.data,
      lastActivityAt: new Date().toISOString()
    });
  }

  /**
   * Check if user is member
   */
  hasMember(userId: UUID): boolean {
    return this.memberIds.includes(userId);
  }

  /**
   * Check if room is active (not archived)
   */
  isActive(): boolean {
    return !this.isArchived;
  }

  /**
   * Get member count
   */
  getMemberCount(): number {
    return this.memberIds.length;
  }

  toString(): string {
    return `ChatRoom(${this.roomId}, "${this.name}", ${this.memberIds.length} members)`;
  }
}