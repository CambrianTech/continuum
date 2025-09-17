/**
 * User Relationships - Foreign key relationships for BaseUser hierarchy
 *
 * Defines proper relationships between users, sessions, permissions, and other entities.
 * Works with DataDaemon for actual storage and querying.
 */

import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import type { BaseUser } from './BaseUser';
import type { BaseEntity } from '../../orm/shared/BaseORM';

// ============================================================================
// FOREIGN KEY RELATIONSHIPS
// ============================================================================

/**
 * User Session - One user can have many sessions
 */
export interface UserSession extends BaseEntity {
  readonly userId: UUID; // Foreign key to BaseUser
  readonly sessionToken: string;
  readonly deviceInfo: {
    readonly userAgent: string;
    readonly ipAddress: string;
    readonly platform: string;
  };
  readonly isActive: boolean;
  readonly startedAt: Date;
  readonly lastActivityAt: Date;
  readonly endedAt?: Date;
}

/**
 * User Permissions - Many-to-many through permissions table
 */
export interface UserPermission extends BaseEntity {
  readonly userId: UUID; // Foreign key to BaseUser
  readonly permission: string; // 'read_messages', 'send_messages', 'admin', etc.
  readonly resource: string; // '*', 'room:123', 'user:456', etc.
  readonly grantedBy: UUID; // Foreign key to BaseUser (who granted it)
  readonly grantedAt: Date;
  readonly expiresAt?: Date;
}

/**
 * Chat Room Participation - Many-to-many between users and rooms
 */
export interface RoomParticipation extends BaseEntity {
  readonly userId: UUID; // Foreign key to BaseUser
  readonly roomId: UUID; // Foreign key to ChatRoom
  readonly role: 'member' | 'moderator' | 'admin';
  readonly joinedAt: Date;
  readonly leftAt?: Date;
  readonly isActive: boolean;
  readonly messageCount: number;
  readonly lastMessageAt?: Date;
}

/**
 * Chat Room - Rooms that users can participate in
 */
export interface ChatRoom extends BaseEntity {
  readonly name: string;
  readonly description?: string;
  readonly isPrivate: boolean;
  readonly createdBy: UUID; // Foreign key to BaseUser
  readonly createdAt: Date;
  readonly maxParticipants: number;
  readonly isArchived: boolean;
}

/**
 * Chat Message - Messages sent in rooms
 */
export interface ChatMessage extends BaseEntity {
  readonly roomId: UUID; // Foreign key to ChatRoom
  readonly senderId: UUID; // Foreign key to BaseUser
  readonly content: string;
  readonly messageType: 'text' | 'image' | 'file' | 'system';
  readonly replyToId?: UUID; // Foreign key to ChatMessage (self-reference)
  readonly sentAt: Date;
  readonly editedAt?: Date;
  readonly isDeleted: boolean;
}

/**
 * User-to-User Relationships - Friends, blocking, etc.
 */
export interface UserRelationship extends BaseEntity {
  readonly fromUserId: UUID; // Foreign key to BaseUser (who initiated)
  readonly toUserId: UUID; // Foreign key to BaseUser (target)
  readonly relationshipType: 'friend' | 'blocked' | 'friend_request';
  readonly status: 'pending' | 'accepted' | 'declined' | 'active';
  readonly createdAt: Date;
  readonly updatedAt: Date;
}

/**
 * Message Reactions - Emoji reactions to messages
 */
export interface MessageReaction extends BaseEntity {
  readonly messageId: UUID; // Foreign key to ChatMessage
  readonly userId: UUID; // Foreign key to BaseUser
  readonly emoji: string; // Unicode emoji or custom emoji identifier
  readonly reactionType: 'unicode' | 'custom';
  readonly reactedAt: Date;
}

/**
 * Typing Indicators - Real-time typing status
 */
export interface TypingIndicator extends BaseEntity {
  readonly roomId: UUID; // Foreign key to ChatRoom
  readonly userId: UUID; // Foreign key to BaseUser
  readonly startedAt: Date;
  readonly expiresAt: Date; // Auto-expire after timeout (usually 10 seconds)
}

/**
 * Enhanced User Presence - Extended status beyond basic online/offline
 */
export interface UserPresence extends BaseEntity {
  readonly userId: UUID; // Foreign key to BaseUser
  readonly status: 'online' | 'away' | 'busy' | 'invisible' | 'offline';
  readonly customStatus?: string; // "Working on code", emoji status, etc.
  readonly lastSeenAt: Date;
  readonly currentActivity?: {
    readonly type: 'typing' | 'speaking' | 'idle';
    readonly location?: string; // Which room they're active in
    readonly startedAt: Date;
  };
}

/**
 * User Profile Extensions - Additional profile data beyond BaseUser
 */
export interface UserProfile extends BaseEntity {
  readonly userId: UUID; // Foreign key to BaseUser
  readonly avatarUrl?: string;
  readonly bio?: string;
  readonly location?: string;
  readonly timezone: string;
  readonly theme: 'light' | 'dark' | 'auto';
  readonly language: string;

  // Privacy settings
  readonly showOnlineStatus: boolean;
  readonly allowDirectMessages: boolean;
  readonly shareActivity: boolean;

  // Notification preferences
  readonly notificationSettings: {
    readonly mentions: boolean;
    readonly directMessages: boolean;
    readonly roomUpdates: boolean;
    readonly email: boolean;
    readonly push: boolean;
  };

  readonly updatedAt: Date;
}

/**
 * AI Model Configuration - For AI users
 */
export interface AIModelConfig {
  readonly provider: string; // 'openai', 'anthropic', 'deepseek', etc.
  readonly model: string; // 'gpt-4', 'claude-3-sonnet', etc.
  readonly contextWindow?: number;
  readonly maxTokens?: number;
  readonly temperature?: number;
  readonly systemPrompt?: string;
}

/**
 * Persona Configuration - For PersonaUser specific data
 */
export interface PersonaConfig {
  readonly prompt: string;
  readonly personality: {
    readonly traits: readonly string[];
    readonly style: string;
    readonly expertise: readonly string[];
  };
  readonly ragConfig?: {
    readonly enabled: boolean;
    readonly knowledgeBase: string;
    readonly retrievalStrategy: string;
  };
}

/**
 * Agent Portal Configuration - For AgentUser specific data
 */
export interface AgentPortalConfig {
  readonly portalType: string; // 'api', 'webhook', 'websocket', etc.
  readonly endpoint: string;
  readonly apiKey?: string;
  readonly config: Record<string, unknown>;
}

// ============================================================================
// EXTENDED USER DATA WITH RELATIONSHIPS
// ============================================================================

/**
 * Extended BaseUser data with relationship foreign keys
 */
export interface BaseUserDataWithRelationships {
  readonly userId: UUID;
  readonly sessionId: UUID;
  readonly displayName: string;
  readonly citizenType: 'human' | 'ai';
  readonly capabilities: readonly string[];
  readonly createdAt: Date;
  readonly lastActiveAt: string;
  readonly preferences: Record<string, unknown>;
  readonly isOnline: boolean;

  // Foreign key relationships (loaded separately or joined)
  readonly sessions?: readonly UserSession[];
  readonly permissions?: readonly UserPermission[];
  readonly roomParticipations?: readonly RoomParticipation[];
  readonly sentMessages?: readonly ChatMessage[];
}

/**
 * Extended AI User data with AI-specific relationships
 */
export interface AIUserDataWithRelationships extends BaseUserDataWithRelationships {
  readonly citizenType: 'ai';
  readonly aiType: 'agent' | 'persona';
  readonly modelConfig: AIModelConfig;
  readonly specialization?: string;
  readonly contextMemory?: readonly string[];
}

/**
 * Extended Human User data (humans don't need AI-specific fields)
 */
export interface HumanUserDataWithRelationships extends BaseUserDataWithRelationships {
  readonly citizenType: 'human';
  readonly authInfo?: {
    readonly lastLoginAt: string;
    readonly loginCount: number;
    readonly isEmailVerified: boolean;
  };
}

/**
 * Persona User data with persona-specific configuration
 */
export interface PersonaUserDataWithRelationships extends AIUserDataWithRelationships {
  readonly aiType: 'persona';
  readonly personaConfig: PersonaConfig;
}

/**
 * Agent User data with agent-specific configuration
 */
export interface AgentUserDataWithRelationships extends AIUserDataWithRelationships {
  readonly aiType: 'agent';
  readonly agentPortalConfig: AgentPortalConfig;
}