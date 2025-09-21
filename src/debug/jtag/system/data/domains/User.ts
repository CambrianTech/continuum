/**
 * User Domain - Citizens, Personas, and Identity Management
 * 
 * Handles all user types: humans, AI agents, personas
 * Professional typing with capabilities and permissions
 */

import type { 
  BaseEntity, 
  UserId, 
  CitizenId, 
  PersonaId, 
  SessionId,
  ISOString,
  DataResult,
  DataError
} from './CoreTypes';

/**
 * User Capabilities - What a user can do in the system
 */
export interface UserCapabilities {
  readonly canSendMessages: boolean;
  readonly canReceiveMessages: boolean;
  readonly canCreateRooms: boolean;
  readonly canInviteOthers: boolean;
  readonly canModerate: boolean;
  readonly autoResponds: boolean;        // For AI users
  readonly providesContext: boolean;     // For AI assistants
  readonly canTrain: boolean;           // For persona development
  readonly canAccessPersonas: boolean;  // For persona management
}

/**
 * User Preferences - UI and behavior settings
 */
export interface UserPreferences {
  readonly theme: 'light' | 'dark' | 'auto';
  readonly language: string;
  readonly timezone: string;
  readonly notifications: {
    readonly mentions: boolean;
    readonly directMessages: boolean;
    readonly roomUpdates: boolean;
  };
  readonly privacy: {
    readonly showOnlineStatus: boolean;
    readonly allowDirectMessages: boolean;
    readonly shareActivity: boolean;
  };
}

/**
 * User Status - Current activity state
 */
export type UserStatus = 'online' | 'offline' | 'away' | 'busy' | 'invisible';

/**
 * User Type - Core identity classification
 */
export type UserType = 'human' | 'ai' | 'persona' | 'system';

/**
 * User Profile - Public information
 */
export interface UserProfile {
  readonly displayName: string;
  readonly avatar?: string;
  readonly bio?: string;
  readonly location?: string;
  readonly website?: string;
  readonly joinedAt: ISOString;
}

/**
 * User Entity - Core user/citizen data
 */
export interface UserData extends BaseEntity {
  readonly userId: UserId;
  readonly citizenId?: CitizenId;        // Links to citizen system
  readonly personaId?: PersonaId;        // Links to persona/LoRA system
  readonly type: UserType;
  readonly profile: UserProfile;
  readonly capabilities: UserCapabilities;
  readonly preferences: UserPreferences;
  readonly status: UserStatus;
  readonly lastActiveAt: ISOString;
  readonly sessionsActive: readonly SessionId[];

  // Index signature to make compatible with RecordData
  readonly [key: string]: unknown;
}

/**
 * User Creation Data - What's needed to create a user
 */
export interface CreateUserData {
  readonly displayName: string;
  readonly type: UserType;
  readonly capabilities?: Partial<UserCapabilities>;
  readonly preferences?: Partial<UserPreferences>;
  readonly profile?: Partial<Omit<UserProfile, 'displayName' | 'joinedAt'>>;
}

/**
 * User Update Data - What can be updated
 */
export interface UpdateUserData {
  readonly profile?: Partial<UserProfile>;
  readonly capabilities?: Partial<UserCapabilities>;
  readonly preferences?: Partial<UserPreferences>;
  readonly status?: UserStatus;
}

/**
 * User Query Filters - Type-safe filtering
 */
export interface UserQueryFilters {
  readonly type?: UserType;
  readonly status?: UserStatus;
  readonly citizenId?: CitizenId;
  readonly personaId?: PersonaId;
  readonly activeSince?: ISOString;
  readonly hasCapability?: keyof UserCapabilities;
}

/**
 * Default Capabilities by User Type
 */
export const DEFAULT_USER_CAPABILITIES: Record<UserType, UserCapabilities> = {
  human: {
    canSendMessages: true,
    canReceiveMessages: true,
    canCreateRooms: true,
    canInviteOthers: true,
    canModerate: false,
    autoResponds: false,
    providesContext: false,
    canTrain: false,
    canAccessPersonas: true
  },
  ai: {
    canSendMessages: true,
    canReceiveMessages: true,
    canCreateRooms: false,
    canInviteOthers: false,
    canModerate: false,
    autoResponds: true,
    providesContext: true,
    canTrain: false,
    canAccessPersonas: true
  },
  persona: {
    canSendMessages: true,
    canReceiveMessages: true,
    canCreateRooms: false,
    canInviteOthers: false,
    canModerate: false,
    autoResponds: true,
    providesContext: true,
    canTrain: true,
    canAccessPersonas: false
  },
  system: {
    canSendMessages: true,
    canReceiveMessages: false,
    canCreateRooms: true,
    canInviteOthers: true,
    canModerate: true,
    autoResponds: false,
    providesContext: false,
    canTrain: false,
    canAccessPersonas: true
  }
};

/**
 * Default User Preferences
 */
export const DEFAULT_USER_PREFERENCES: UserPreferences = {
  theme: 'auto',
  language: 'en',
  timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
  notifications: {
    mentions: true,
    directMessages: true,
    roomUpdates: false
  },
  privacy: {
    showOnlineStatus: true,
    allowDirectMessages: true,
    shareActivity: true
  }
};

/**
 * User Validation
 */
export function validateUserData(data: CreateUserData): DataResult<void, DataError> {
  if (!data.displayName || data.displayName.trim().length === 0) {
    return { success: false, error: { 
      type: 'VALIDATION_ERROR', 
      message: 'Display name is required',
      code: 'INVALID_DISPLAY_NAME'
    }};
  }

  if (data.displayName.length > 100) {
    return { success: false, error: { 
      type: 'VALIDATION_ERROR', 
      message: 'Display name cannot exceed 100 characters',
      code: 'DISPLAY_NAME_TOO_LONG'
    }};
  }

  const validTypes: UserType[] = ['human', 'ai', 'persona', 'system'];
  if (!validTypes.includes(data.type)) {
    return { success: false, error: { 
      type: 'VALIDATION_ERROR', 
      message: `Invalid user type: ${data.type}`,
      code: 'INVALID_USER_TYPE'
    }};
  }

  return { success: true, data: undefined };
}

/**
 * User Helper Functions
 */
export function mergeUserCapabilities(base: UserCapabilities, override?: Partial<UserCapabilities>): UserCapabilities {
  return { ...base, ...override };
}

export function mergeUserPreferences(base: UserPreferences, override?: Partial<UserPreferences>): UserPreferences {
  return {
    ...base,
    ...override,
    notifications: { ...base.notifications, ...override?.notifications },
    privacy: { ...base.privacy, ...override?.privacy }
  };
}

export function isUserOnline(user: UserData): boolean {
  return user.status === 'online' && user.sessionsActive.length > 0;
}

export function canUserPerformAction(user: UserData, action: keyof UserCapabilities): boolean {
  return user.capabilities[action];
}

export function getUserDisplayInfo(user: UserData): { name: string; type: string; status: string } {
  return {
    name: user.profile.displayName,
    type: user.type,
    status: user.status
  };
}