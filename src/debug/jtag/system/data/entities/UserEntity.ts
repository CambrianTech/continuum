/**
 * User Entity - Decorated UserData for field extraction
 *
 * Uses field decorators to define storage requirements for the serde-style adapter system
 * Replaces manual field mappings with elegant decorator-based schema definition
 */

import type {
  UserType,
  UserStatus,
  UserProfile,
  UserCapabilities,
  UserPreferences
} from '../domains/User';
import type { UserId, CitizenId, PersonaId, SessionId, ISOString } from '../domains/CoreTypes';
import {
  PrimaryField,
  TextField,
  DateField,
  EnumField,
  JsonField,
  ForeignKeyField
} from '../decorators/FieldDecorators';

/**
 * Decorated User Entity - Storage-aware version of UserData
 *
 * The decorators define which fields get extracted to dedicated columns
 * vs stored as JSON blobs for optimal query performance
 */
export class UserEntity {
  // Single source of truth for collection name - used by both decorators and commands
  static readonly collection = 'User';

  @PrimaryField()
  userId: UserId;

  @ForeignKeyField({ references: 'citizens.citizenId', nullable: true })
  citizenId?: CitizenId;

  @ForeignKeyField({ references: 'personas.personaId', nullable: true })
  personaId?: PersonaId;

  @EnumField({ index: true })
  type: UserType;

  @TextField({ index: true })
  displayName: string;

  @EnumField({ index: true })
  status: UserStatus;

  @DateField({ index: true })
  lastActiveAt: ISOString;

  // Complex objects stored as JSON blobs
  @JsonField()
  profile: UserProfile;

  @JsonField()
  capabilities: UserCapabilities;

  @JsonField()
  preferences: UserPreferences;

  @JsonField()
  sessionsActive: readonly SessionId[];

  // BaseEntity inherited fields
  id: string;
  createdAt: ISOString;
  updatedAt: ISOString;
  version: number;

  // Index signature for UserData compatibility
  [key: string]: unknown;

  constructor() {
    // Default values - will be set by storage layer
    this.userId = '' as UserId;
    this.type = 'human';
    this.displayName = '';
    this.status = 'offline';
    this.lastActiveAt = new Date().toISOString() as ISOString;
    this.profile = { displayName: '', avatar: '', bio: '', location: '', joinedAt: new Date().toISOString() as ISOString };
    this.capabilities = { canSendMessages: true, canReceiveMessages: true, canCreateRooms: false, canInviteOthers: false, canModerate: false, autoResponds: false, providesContext: false, canTrain: false, canAccessPersonas: false };
    this.preferences = { theme: 'auto', language: 'en', timezone: 'UTC', notifications: { mentions: true, directMessages: true, roomUpdates: false }, privacy: { showOnlineStatus: true, allowDirectMessages: true, shareActivity: false } };
    this.sessionsActive = [];

    // BaseEntity fields
    this.id = '';
    this.createdAt = new Date().toISOString() as ISOString;
    this.updatedAt = new Date().toISOString() as ISOString;
    this.version = 1;
  }
}