/**
 * ChatRoomData Domain - Chat room/channel management
 *
 * Professional chat rooms with Discord-like features
 * Supports public/private rooms, permissions, and member management
 */

import type {
  BaseEntity,
  RoomId,
  UserId,
  ISOString,
  DataResult,
  DataError
} from './CoreTypes';

/**
 * Room Type Classifications
 */
export type RoomType = 'public' | 'private' | 'direct' | 'group' | 'channel';

/**
 * Room Status
 */
export type RoomStatus = 'active' | 'archived' | 'disabled';

/**
 * Room Privacy Settings
 */
export interface RoomPrivacy {
  readonly isPublic: boolean;
  readonly requiresInvite: boolean;
  readonly allowGuestAccess: boolean;
  readonly searchable: boolean;
}

/**
 * Room Member Role
 */
export type MemberRole = 'owner' | 'admin' | 'moderator' | 'member' | 'guest';

/**
 * Room Member Information
 */
export interface RoomMember {
  readonly userId: UserId;
  readonly displayName: string;
  readonly role: MemberRole;
  readonly joinedAt: ISOString;
  readonly lastActiveAt: ISOString;
  readonly isOnline: boolean;
}

/**
 * Room Statistics
 */
export interface RoomStats {
  readonly memberCount: number;
  readonly messageCount: number;
  readonly lastMessageAt?: ISOString;
  readonly createdAt: ISOString;
  readonly lastActivityAt: ISOString;
}

/**
 * Room Settings
 */
export interface RoomSettings {
  readonly allowReactions: boolean;
  readonly allowThreads: boolean;
  readonly allowFileSharing: boolean;
  readonly messageRetentionDays?: number;
  readonly slowMode?: number; // seconds between messages
}

/**
 * Chat Room Data - Pure data interface extending BaseEntity
 * Used by adapters for storage and retrieval from database
 */
export interface ChatRoomData extends BaseEntity {
  readonly roomId: RoomId;
  readonly name: string;
  readonly displayName: string;
  readonly description?: string;
  readonly topic?: string;
  readonly type: RoomType;
  readonly status: RoomStatus;
  readonly privacy: RoomPrivacy;
  readonly settings: RoomSettings;
  readonly stats: RoomStats;
  readonly ownerId: UserId;
  readonly members: readonly RoomMember[];
  readonly tags: readonly string[];
}

/**
 * Room Creation Data
 */
export interface CreateRoomData {
  readonly name: string;
  readonly displayName?: string;
  readonly description?: string;
  readonly topic?: string;
  readonly type: RoomType;
  readonly privacy?: Partial<RoomPrivacy>;
  readonly settings?: Partial<RoomSettings>;
  readonly ownerId: UserId;
  readonly initialMembers?: readonly UserId[];
  readonly tags?: readonly string[];
}

/**
 * Room Update Data
 */
export interface UpdateRoomData {
  readonly name?: string;
  readonly displayName?: string;
  readonly description?: string;
  readonly topic?: string;
  readonly privacy?: Partial<RoomPrivacy>;
  readonly settings?: Partial<RoomSettings>;
  readonly status?: RoomStatus;
  readonly tags?: readonly string[];
}

/**
 * Room Query Filters
 */
export interface RoomQueryFilters {
  readonly type?: RoomType;
  readonly status?: RoomStatus;
  readonly isPublic?: boolean;
  readonly ownerId?: UserId;
  readonly memberUserId?: UserId;
  readonly hasTag?: string;
  readonly nameSearch?: string;
  readonly createdAfter?: ISOString;
  readonly lastActiveAfter?: ISOString;
}

/**
 * Default Room Privacy Settings
 */
export const DEFAULT_ROOM_PRIVACY: RoomPrivacy = {
  isPublic: true,
  requiresInvite: false,
  allowGuestAccess: true,
  searchable: true
};

/**
 * Default Room Settings
 */
export const DEFAULT_ROOM_SETTINGS: RoomSettings = {
  allowReactions: true,
  allowThreads: true,
  allowFileSharing: true,
  messageRetentionDays: 365
};

/**
 * Default Privacy by Room Type
 */
export const DEFAULT_PRIVACY_BY_TYPE: Record<RoomType, RoomPrivacy> = {
  public: DEFAULT_ROOM_PRIVACY,
  private: {
    ...DEFAULT_ROOM_PRIVACY,
    isPublic: false,
    requiresInvite: true,
    searchable: false
  },
  direct: {
    ...DEFAULT_ROOM_PRIVACY,
    isPublic: false,
    requiresInvite: true,
    allowGuestAccess: false,
    searchable: false
  },
  group: {
    ...DEFAULT_ROOM_PRIVACY,
    isPublic: false,
    requiresInvite: true,
    searchable: false
  },
  channel: DEFAULT_ROOM_PRIVACY
};

/**
 * Room Validation
 */
export function validateRoomData(data: CreateRoomData): DataResult<void, DataError> {
  if (!data.name || data.name.trim().length === 0) {
    return { success: false, error: {
      type: 'VALIDATION_ERROR',
      message: 'Room name is required',
      code: 'INVALID_ROOM_NAME'
    }};
  }

  if (data.name.length > 50) {
    return { success: false, error: {
      type: 'VALIDATION_ERROR',
      message: 'Room name cannot exceed 50 characters',
      code: 'ROOM_NAME_TOO_LONG'
    }};
  }

  // Validate room name format (alphanumeric, hyphens, underscores)
  const namePattern = /^[a-zA-Z0-9_-]+$/;
  if (!namePattern.test(data.name)) {
    return { success: false, error: {
      type: 'VALIDATION_ERROR',
      message: 'Room name can only contain letters, numbers, hyphens, and underscores',
      code: 'INVALID_ROOM_NAME_FORMAT'
    }};
  }

  const validTypes: RoomType[] = ['public', 'private', 'direct', 'group', 'channel'];
  if (!validTypes.includes(data.type)) {
    return { success: false, error: {
      type: 'VALIDATION_ERROR',
      message: `Invalid room type: ${data.type}`,
      code: 'INVALID_ROOM_TYPE'
    }};
  }

  return { success: true, data: undefined };
}

/**
 * Room Helper Functions
 */
export function getRoomDisplayName(room: ChatRoomData): string {
  return room.displayName || room.name;
}

export function isUserRoomMember(room: ChatRoomData, userId: UserId): boolean {
  return room.members.some(member => member.userId === userId);
}

export function getUserRoomRole(room: ChatRoomData, userId: UserId): MemberRole | null {
  const member = room.members.find(member => member.userId === userId);
  return member?.role || null;
}

export function canUserAccessRoom(room: ChatRoomData, userId: UserId): boolean {
  // Public rooms are accessible to everyone
  if (room.privacy.isPublic) {
    return true;
  }

  // Private rooms require membership
  return isUserRoomMember(room, userId);
}

export function canUserModerateRoom(room: ChatRoomData, userId: UserId): boolean {
  const role = getUserRoomRole(room, userId);
  return role === 'owner' || role === 'admin' || role === 'moderator';
}

export function getRoomSummary(room: ChatRoomData): string {
  const memberText = room.stats.memberCount === 1 ? 'member' : 'members';
  return `${room.stats.memberCount} ${memberText}`;
}

export function createRoomStats(memberCount: number = 0): RoomStats {
  const now = new Date().toISOString() as ISOString;
  return {
    memberCount,
    messageCount: 0,
    createdAt: now,
    lastActivityAt: now
  };
}