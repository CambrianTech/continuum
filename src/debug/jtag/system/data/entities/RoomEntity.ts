/**
 * Room Entity - Decorated ChatRoomData for field extraction
 *
 * Uses field decorators to define storage requirements for the serde-style adapter system
 * Replaces manual field mappings with elegant decorator-based schema definition
 */

import type {
  RoomType,
  RoomStatus,
  RoomPrivacy,
  RoomSettings,
  RoomStats,
  RoomMember
} from '../domains/ChatRoom';
import type { RoomId, UserId, ISOString } from '../domains/CoreTypes';
import {
  PrimaryField,
  TextField,
  DateField,
  EnumField,
  JsonField,
  ForeignKeyField
} from '../decorators/FieldDecorators';

/**
 * Decorated Room Entity - Storage-aware version of ChatRoomData
 *
 * The decorators define which fields get extracted to dedicated columns
 * vs stored as JSON blobs for optimal query performance
 */
export class RoomEntity {
  // Single source of truth for collection name - used by both decorators and commands
  static readonly collection = 'Room';

  @PrimaryField()
  roomId: RoomId;

  @TextField({ index: true })
  name: string;

  @TextField()
  displayName: string;

  @TextField({ nullable: true })
  description?: string;

  @TextField({ nullable: true })
  topic?: string;

  @EnumField({ index: true })
  type: RoomType;

  @EnumField({ index: true })
  status: RoomStatus;

  @ForeignKeyField({ references: 'users.userId', index: true })
  ownerId: UserId;

  @DateField({ index: true })
  lastMessageAt?: ISOString;

  // Complex objects stored as JSON blobs
  @JsonField()
  privacy: RoomPrivacy;

  @JsonField()
  settings: RoomSettings;

  @JsonField()
  stats: RoomStats;

  @JsonField()
  members: readonly RoomMember[];

  @JsonField()
  tags: readonly string[];

  // BaseEntity inherited fields
  id: string;
  createdAt: ISOString;
  updatedAt: ISOString;
  version: number;

  constructor() {
    // Default values
    this.roomId = '' as RoomId;
    this.name = '';
    this.displayName = '';
    this.type = 'public';
    this.status = 'active';
    this.ownerId = '' as UserId;
    this.privacy = { isPublic: true, requiresInvite: false, allowGuestAccess: false, searchable: true };
    this.settings = { allowThreads: true, allowReactions: true, allowFileSharing: true, messageRetentionDays: 365, slowMode: 0 };
    this.stats = { memberCount: 0, messageCount: 0, createdAt: new Date().toISOString() as ISOString, lastActivityAt: new Date().toISOString() as ISOString };
    this.members = [];
    this.tags = [];

    // BaseEntity fields
    this.id = '';
    this.createdAt = new Date().toISOString() as ISOString;
    this.updatedAt = new Date().toISOString() as ISOString;
    this.version = 1;
  }
}