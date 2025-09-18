/**
 * User Entity Mappings - ORM metadata for existing domain objects
 *
 * Maps existing BaseUser hierarchy and UserRelationships to proper ORM entities
 * with foreign keys, indexes, and constraints. Works with existing domain classes.
 */

import type { EntityMetadata } from '../shared/BaseORM';

// ============================================================================
// CORE USER ENTITY METADATA (BaseUser → HumanUser/AIUser → AgentUser/PersonaUser)
// ============================================================================

/**
 * BaseUser Entity Metadata - Single table inheritance for all user types
 */
export const BaseUserEntityMetadata: EntityMetadata = {
    tableName: 'users',
    primaryKey: 'id',
    columns: {
        // BaseUserData fields
        id: { type: 'uuid', nullable: false },
        userId: { type: 'uuid', nullable: false, unique: true }, // Alias for id
        sessionId: { type: 'uuid', nullable: false },
        displayName: { type: 'string', nullable: false, length: 255 },
        citizenType: { type: 'string', nullable: false, length: 50 },
        capabilities: { type: 'json', nullable: false, default: "'[]'" },
        createdAt: { type: 'date', nullable: false },
        lastActiveAt: { type: 'date', nullable: false },
        preferences: { type: 'json', nullable: false, default: "'{}'" },
        isOnline: { type: 'boolean', nullable: false, default: false },

        // HumanUser fields (nullable for AI users)
        email: { type: 'string', nullable: true, length: 255, unique: true },
        avatarUrl: { type: 'string', nullable: true, length: 500 },
        isAuthenticated: { type: 'boolean', nullable: true, default: false },
        authInfo: { type: 'json', nullable: true },
        bio: { type: 'string', nullable: true, length: 1000 },
        location: { type: 'string', nullable: true, length: 100 },
        timezone: { type: 'string', nullable: true, length: 50, default: "'UTC'" },
        showOnlineStatus: { type: 'boolean', nullable: true, default: true },
        allowDirectMessages: { type: 'boolean', nullable: true, default: true },
        shareActivity: { type: 'boolean', nullable: true, default: true },
        totalMessages: { type: 'number', nullable: true, default: 0 },
        joinedRooms: { type: 'number', nullable: true, default: 0 },

        // AIUser fields (nullable for human users)
        aiType: { type: 'string', nullable: true, length: 50 },
        modelConfig: { type: 'json', nullable: true },
        specialization: { type: 'string', nullable: true, length: 255 },
        contextMemory: { type: 'json', nullable: true, default: "'[]'" },

        // AgentUser fields (nullable for human/persona users)
        agentPortalConfig: { type: 'json', nullable: true },
        toolAccess: { type: 'json', nullable: true },
        automationLevel: { type: 'number', nullable: true },
        maxConcurrentTasks: { type: 'number', nullable: true },

        // PersonaUser fields (nullable for human/agent users)
        personaConfig: { type: 'json', nullable: true },
        personaStyle: { type: 'string', nullable: true, length: 100 },
        contextualMemory: { type: 'json', nullable: true },
        adaptivePersonality: { type: 'boolean', nullable: true },
        emotionalIntelligence: { type: 'number', nullable: true },
        conversationalDepth: { type: 'number', nullable: true },

        updatedAt: { type: 'date', nullable: false }
    },
    relationships: {
        sessions: {
            type: 'oneToMany',
            targetEntity: 'user_sessions',
            foreignKey: 'userId'
        },
        permissions: {
            type: 'oneToMany',
            targetEntity: 'user_permissions',
            foreignKey: 'userId'
        },
        roomParticipations: {
            type: 'oneToMany',
            targetEntity: 'room_participations',
            foreignKey: 'userId'
        },
        sentMessages: {
            type: 'oneToMany',
            targetEntity: 'messages',
            foreignKey: 'senderId'
        },
        reactions: {
            type: 'oneToMany',
            targetEntity: 'message_reactions',
            foreignKey: 'userId'
        },
        relationshipsFrom: {
            type: 'oneToMany',
            targetEntity: 'user_relationships',
            foreignKey: 'fromUserId'
        },
        relationshipsTo: {
            type: 'oneToMany',
            targetEntity: 'user_relationships',
            foreignKey: 'toUserId'
        },
        presence: {
            type: 'oneToOne',
            targetEntity: 'user_presence',
            foreignKey: 'userId'
        },
        profile: {
            type: 'oneToOne',
            targetEntity: 'user_profiles',
            foreignKey: 'userId'
        }
    },
    indexes: [
        { name: 'idx_users_session_id', columns: ['sessionId'] },
        { name: 'idx_users_citizen_type', columns: ['citizenType'] },
        { name: 'idx_users_ai_type', columns: ['aiType'] },
        { name: 'idx_users_online', columns: ['isOnline', 'lastActiveAt'] },
        { name: 'idx_users_display_name', columns: ['displayName'] },
        { name: 'idx_users_email', columns: ['email'], unique: true }
    ],
    constraints: [
        {
            type: 'check',
            columns: ['citizenType'],
            condition: "citizenType IN ('human', 'ai', 'system')"
        },
        {
            type: 'check',
            columns: ['aiType'],
            condition: "aiType IS NULL OR aiType IN ('agent', 'persona')"
        }
    ]
};

// ============================================================================
// RELATIONSHIP ENTITY METADATA
// ============================================================================

/**
 * UserSession Entity Metadata
 */
export const UserSessionEntityMetadata: EntityMetadata = {
    tableName: 'user_sessions',
    primaryKey: 'id',
    columns: {
        id: { type: 'uuid', nullable: false },
        userId: { type: 'uuid', nullable: false },
        sessionToken: { type: 'string', nullable: false, length: 255, unique: true },
        deviceInfo: { type: 'json', nullable: false },
        isActive: { type: 'boolean', nullable: false, default: true },
        startedAt: { type: 'date', nullable: false },
        lastActivityAt: { type: 'date', nullable: false },
        endedAt: { type: 'date', nullable: true },
        createdAt: { type: 'date', nullable: false },
        updatedAt: { type: 'date', nullable: false }
    },
    relationships: {
        user: {
            type: 'manyToOne',
            targetEntity: 'users',
            foreignKey: 'userId'
        }
    },
    indexes: [
        { name: 'idx_user_sessions_user_id', columns: ['userId'] },
        { name: 'idx_user_sessions_token', columns: ['sessionToken'], unique: true },
        { name: 'idx_user_sessions_active', columns: ['isActive', 'lastActivityAt'] }
    ],
    constraints: [
        {
            type: 'foreignKey',
            columns: ['userId'],
            references: { table: 'users', columns: ['id'] }
        }
    ]
};

/**
 * UserPermission Entity Metadata
 */
export const UserPermissionEntityMetadata: EntityMetadata = {
    tableName: 'user_permissions',
    primaryKey: 'id',
    columns: {
        id: { type: 'uuid', nullable: false },
        userId: { type: 'uuid', nullable: false },
        permission: { type: 'string', nullable: false, length: 100 },
        resource: { type: 'string', nullable: false, length: 255, default: "'*'" },
        grantedBy: { type: 'uuid', nullable: false },
        grantedAt: { type: 'date', nullable: false },
        expiresAt: { type: 'date', nullable: true },
        createdAt: { type: 'date', nullable: false },
        updatedAt: { type: 'date', nullable: false }
    },
    relationships: {
        user: {
            type: 'manyToOne',
            targetEntity: 'users',
            foreignKey: 'userId'
        },
        grantedByUser: {
            type: 'manyToOne',
            targetEntity: 'users',
            foreignKey: 'grantedBy'
        }
    },
    indexes: [
        { name: 'idx_user_permissions_user_id', columns: ['userId'] },
        { name: 'idx_user_permissions_permission', columns: ['permission', 'resource'] },
        { name: 'idx_user_permissions_expires', columns: ['expiresAt'] }
    ],
    constraints: [
        {
            type: 'foreignKey',
            columns: ['userId'],
            references: { table: 'users', columns: ['id'] }
        },
        {
            type: 'foreignKey',
            columns: ['grantedBy'],
            references: { table: 'users', columns: ['id'] }
        },
        {
            type: 'unique',
            columns: ['userId', 'permission', 'resource']
        }
    ]
};

/**
 * UserRelationship Entity Metadata (Friends, blocking, etc.)
 */
export const UserRelationshipEntityMetadata: EntityMetadata = {
    tableName: 'user_relationships',
    primaryKey: 'id',
    columns: {
        id: { type: 'uuid', nullable: false },
        fromUserId: { type: 'uuid', nullable: false },
        toUserId: { type: 'uuid', nullable: false },
        relationshipType: { type: 'string', nullable: false, length: 50 },
        status: { type: 'string', nullable: false, length: 50, default: "'pending'" },
        createdAt: { type: 'date', nullable: false },
        updatedAt: { type: 'date', nullable: false }
    },
    relationships: {
        fromUser: {
            type: 'manyToOne',
            targetEntity: 'users',
            foreignKey: 'fromUserId'
        },
        toUser: {
            type: 'manyToOne',
            targetEntity: 'users',
            foreignKey: 'toUserId'
        }
    },
    indexes: [
        { name: 'idx_user_rel_from_user', columns: ['fromUserId'] },
        { name: 'idx_user_rel_to_user', columns: ['toUserId'] },
        { name: 'idx_user_rel_type_status', columns: ['relationshipType', 'status'] }
    ],
    constraints: [
        {
            type: 'foreignKey',
            columns: ['fromUserId'],
            references: { table: 'users', columns: ['id'] }
        },
        {
            type: 'foreignKey',
            columns: ['toUserId'],
            references: { table: 'users', columns: ['id'] }
        },
        {
            type: 'unique',
            columns: ['fromUserId', 'toUserId', 'relationshipType']
        },
        {
            type: 'check',
            columns: ['fromUserId', 'toUserId'],
            condition: 'fromUserId != toUserId'
        },
        {
            type: 'check',
            columns: ['relationshipType'],
            condition: "relationshipType IN ('friend', 'blocked', 'friend_request')"
        },
        {
            type: 'check',
            columns: ['status'],
            condition: "status IN ('pending', 'accepted', 'declined', 'active')"
        }
    ]
};

/**
 * ChatRoom Entity Metadata
 */
export const ChatRoomEntityMetadata: EntityMetadata = {
    tableName: 'chat_rooms',
    primaryKey: 'id',
    columns: {
        id: { type: 'uuid', nullable: false },
        name: { type: 'string', nullable: false, length: 255 },
        description: { type: 'string', nullable: true, length: 1000 },
        isPrivate: { type: 'boolean', nullable: false, default: false },
        createdBy: { type: 'uuid', nullable: false },
        maxParticipants: { type: 'number', nullable: false, default: 100 },
        isArchived: { type: 'boolean', nullable: false, default: false },
        createdAt: { type: 'date', nullable: false },
        updatedAt: { type: 'date', nullable: false }
    },
    relationships: {
        creator: {
            type: 'manyToOne',
            targetEntity: 'users',
            foreignKey: 'createdBy'
        },
        participations: {
            type: 'oneToMany',
            targetEntity: 'room_participations',
            foreignKey: 'roomId'
        },
        messages: {
            type: 'oneToMany',
            targetEntity: 'chat_messages',
            foreignKey: 'roomId'
        }
    },
    indexes: [
        { name: 'idx_chat_rooms_name', columns: ['name'] },
        { name: 'idx_chat_rooms_creator', columns: ['createdBy'] },
        { name: 'idx_chat_rooms_archived', columns: ['isArchived'] }
    ],
    constraints: [
        {
            type: 'foreignKey',
            columns: ['createdBy'],
            references: { table: 'users', columns: ['id'] }
        }
    ]
};

/**
 * RoomParticipation Entity Metadata
 */
export const RoomParticipationEntityMetadata: EntityMetadata = {
    tableName: 'room_participations',
    primaryKey: 'id',
    columns: {
        id: { type: 'uuid', nullable: false },
        userId: { type: 'uuid', nullable: false },
        roomId: { type: 'uuid', nullable: false },
        role: { type: 'string', nullable: false, length: 50, default: "'member'" },
        joinedAt: { type: 'date', nullable: false },
        leftAt: { type: 'date', nullable: true },
        isActive: { type: 'boolean', nullable: false, default: true },
        messageCount: { type: 'number', nullable: false, default: 0 },
        lastMessageAt: { type: 'date', nullable: true },
        createdAt: { type: 'date', nullable: false },
        updatedAt: { type: 'date', nullable: false }
    },
    relationships: {
        user: {
            type: 'manyToOne',
            targetEntity: 'users',
            foreignKey: 'userId'
        },
        room: {
            type: 'manyToOne',
            targetEntity: 'chat_rooms',
            foreignKey: 'roomId'
        }
    },
    indexes: [
        { name: 'idx_room_participation_user', columns: ['userId', 'isActive'] },
        { name: 'idx_room_participation_room', columns: ['roomId', 'isActive'] },
        { name: 'idx_room_participation_role', columns: ['role'] }
    ],
    constraints: [
        {
            type: 'foreignKey',
            columns: ['userId'],
            references: { table: 'users', columns: ['id'] }
        },
        {
            type: 'foreignKey',
            columns: ['roomId'],
            references: { table: 'chat_rooms', columns: ['id'] }
        },
        {
            type: 'unique',
            columns: ['userId', 'roomId']
        },
        {
            type: 'check',
            columns: ['role'],
            condition: "role IN ('member', 'moderator', 'admin')"
        }
    ]
};

/**
 * ChatMessage Entity Metadata
 */
export const ChatMessageEntityMetadata: EntityMetadata = {
    tableName: 'chat_messages',
    primaryKey: 'id',
    columns: {
        id: { type: 'uuid', nullable: false },
        roomId: { type: 'uuid', nullable: false },
        senderId: { type: 'uuid', nullable: false },
        content: { type: 'string', nullable: false, length: 4000 },
        messageType: { type: 'string', nullable: false, length: 50, default: "'text'" },
        replyToId: { type: 'uuid', nullable: true },
        sentAt: { type: 'date', nullable: false },
        editedAt: { type: 'date', nullable: true },
        isDeleted: { type: 'boolean', nullable: false, default: false },
        createdAt: { type: 'date', nullable: false },
        updatedAt: { type: 'date', nullable: false }
    },
    relationships: {
        room: {
            type: 'manyToOne',
            targetEntity: 'chat_rooms',
            foreignKey: 'roomId'
        },
        sender: {
            type: 'manyToOne',
            targetEntity: 'users',
            foreignKey: 'senderId'
        },
        replyTo: {
            type: 'manyToOne',
            targetEntity: 'chat_messages',
            foreignKey: 'replyToId'
        },
        replies: {
            type: 'oneToMany',
            targetEntity: 'chat_messages',
            foreignKey: 'replyToId'
        },
        reactions: {
            type: 'oneToMany',
            targetEntity: 'message_reactions',
            foreignKey: 'messageId'
        }
    },
    indexes: [
        { name: 'idx_chat_messages_room_sent', columns: ['roomId', 'sentAt'] },
        { name: 'idx_chat_messages_sender', columns: ['senderId'] },
        { name: 'idx_chat_messages_reply_to', columns: ['replyToId'] },
        { name: 'idx_chat_messages_deleted', columns: ['isDeleted'] }
    ],
    constraints: [
        {
            type: 'foreignKey',
            columns: ['roomId'],
            references: { table: 'chat_rooms', columns: ['id'] }
        },
        {
            type: 'foreignKey',
            columns: ['senderId'],
            references: { table: 'users', columns: ['id'] }
        },
        {
            type: 'foreignKey',
            columns: ['replyToId'],
            references: { table: 'chat_messages', columns: ['id'] }
        },
        {
            type: 'check',
            columns: ['messageType'],
            condition: "messageType IN ('text', 'image', 'file', 'system')"
        }
    ]
};

/**
 * MessageReaction Entity Metadata
 */
export const MessageReactionEntityMetadata: EntityMetadata = {
    tableName: 'message_reactions',
    primaryKey: 'id',
    columns: {
        id: { type: 'uuid', nullable: false },
        messageId: { type: 'uuid', nullable: false },
        userId: { type: 'uuid', nullable: false },
        emoji: { type: 'string', nullable: false, length: 100 },
        reactionType: { type: 'string', nullable: false, length: 50, default: "'unicode'" },
        reactedAt: { type: 'date', nullable: false },
        createdAt: { type: 'date', nullable: false },
        updatedAt: { type: 'date', nullable: false }
    },
    relationships: {
        message: {
            type: 'manyToOne',
            targetEntity: 'chat_messages',
            foreignKey: 'messageId'
        },
        user: {
            type: 'manyToOne',
            targetEntity: 'users',
            foreignKey: 'userId'
        }
    },
    indexes: [
        { name: 'idx_message_reactions_message', columns: ['messageId'] },
        { name: 'idx_message_reactions_user', columns: ['userId'] },
        { name: 'idx_message_reactions_emoji', columns: ['emoji'] }
    ],
    constraints: [
        {
            type: 'foreignKey',
            columns: ['messageId'],
            references: { table: 'chat_messages', columns: ['id'] }
        },
        {
            type: 'foreignKey',
            columns: ['userId'],
            references: { table: 'users', columns: ['id'] }
        },
        {
            type: 'unique',
            columns: ['messageId', 'userId', 'emoji']
        },
        {
            type: 'check',
            columns: ['reactionType'],
            condition: "reactionType IN ('unicode', 'custom')"
        }
    ]
};

/**
 * TypingIndicator Entity Metadata
 */
export const TypingIndicatorEntityMetadata: EntityMetadata = {
    tableName: 'typing_indicators',
    primaryKey: 'id',
    columns: {
        id: { type: 'uuid', nullable: false },
        roomId: { type: 'uuid', nullable: false },
        userId: { type: 'uuid', nullable: false },
        startedAt: { type: 'date', nullable: false },
        expiresAt: { type: 'date', nullable: false },
        createdAt: { type: 'date', nullable: false },
        updatedAt: { type: 'date', nullable: false }
    },
    relationships: {
        room: {
            type: 'manyToOne',
            targetEntity: 'chat_rooms',
            foreignKey: 'roomId'
        },
        user: {
            type: 'manyToOne',
            targetEntity: 'users',
            foreignKey: 'userId'
        }
    },
    indexes: [
        { name: 'idx_typing_indicators_room_expires', columns: ['roomId', 'expiresAt'] },
        { name: 'idx_typing_indicators_expires', columns: ['expiresAt'] }
    ],
    constraints: [
        {
            type: 'foreignKey',
            columns: ['roomId'],
            references: { table: 'chat_rooms', columns: ['id'] }
        },
        {
            type: 'foreignKey',
            columns: ['userId'],
            references: { table: 'users', columns: ['id'] }
        },
        {
            type: 'unique',
            columns: ['roomId', 'userId']
        }
    ]
};

/**
 * UserPresence Entity Metadata
 */
export const UserPresenceEntityMetadata: EntityMetadata = {
    tableName: 'user_presence',
    primaryKey: 'id',
    columns: {
        id: { type: 'uuid', nullable: false },
        userId: { type: 'uuid', nullable: false, unique: true },
        status: { type: 'string', nullable: false, length: 50, default: "'offline'" },
        customStatus: { type: 'string', nullable: true, length: 200 },
        lastSeenAt: { type: 'date', nullable: false },
        currentActivity: { type: 'json', nullable: true },
        createdAt: { type: 'date', nullable: false },
        updatedAt: { type: 'date', nullable: false }
    },
    relationships: {
        user: {
            type: 'oneToOne',
            targetEntity: 'users',
            foreignKey: 'userId'
        }
    },
    indexes: [
        { name: 'idx_user_presence_user', columns: ['userId'], unique: true },
        { name: 'idx_user_presence_status', columns: ['status', 'lastSeenAt'] }
    ],
    constraints: [
        {
            type: 'foreignKey',
            columns: ['userId'],
            references: { table: 'users', columns: ['id'] }
        },
        {
            type: 'check',
            columns: ['status'],
            condition: "status IN ('online', 'away', 'busy', 'invisible', 'offline')"
        }
    ]
};

/**
 * UserProfile Entity Metadata
 */
export const UserProfileEntityMetadata: EntityMetadata = {
    tableName: 'user_profiles',
    primaryKey: 'id',
    columns: {
        id: { type: 'uuid', nullable: false },
        userId: { type: 'uuid', nullable: false, unique: true },
        avatarUrl: { type: 'string', nullable: true, length: 500 },
        bio: { type: 'string', nullable: true, length: 1000 },
        location: { type: 'string', nullable: true, length: 100 },
        timezone: { type: 'string', nullable: false, default: "'UTC'" },
        theme: { type: 'string', nullable: false, default: "'dark'" },
        language: { type: 'string', nullable: false, default: "'en'" },
        showOnlineStatus: { type: 'boolean', nullable: false, default: true },
        allowDirectMessages: { type: 'boolean', nullable: false, default: true },
        shareActivity: { type: 'boolean', nullable: false, default: true },
        notificationSettings: { type: 'json', nullable: false, default: "'{}'" },
        createdAt: { type: 'date', nullable: false },
        updatedAt: { type: 'date', nullable: false }
    },
    relationships: {
        user: {
            type: 'oneToOne',
            targetEntity: 'users',
            foreignKey: 'userId'
        }
    },
    indexes: [
        { name: 'idx_user_profiles_user', columns: ['userId'], unique: true },
        { name: 'idx_user_profiles_timezone', columns: ['timezone'] }
    ],
    constraints: [
        {
            type: 'foreignKey',
            columns: ['userId'],
            references: { table: 'users', columns: ['id'] }
        },
        {
            type: 'check',
            columns: ['theme'],
            condition: "theme IN ('light', 'dark', 'auto')"
        }
    ]
};