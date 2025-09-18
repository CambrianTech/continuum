/**
 * UserDaemon - Professional User Management with ORM Integration
 *
 * Orchestrates the existing BaseUser hierarchy, UserRepository, and DataDaemon
 * with the new ORM layer to provide complete user relationship management.
 *
 * Architecture Integration:
 * ✅ Works WITH existing BaseUser → HumanUser/AIUser → AgentUser/PersonaUser
 * ✅ Extends existing UserRepository (doesn't replace it)
 * ✅ Leverages existing DataDaemon infrastructure
 * ✅ Adds professional ORM with real foreign keys and relationships
 * ✅ Maintains backward compatibility with existing domain objects
 */

import { DaemonBase } from '../../command-daemon/shared/DaemonBase';
import type { JTAGContext, JTAGMessage } from '../../../system/core/types/JTAGTypes';
import { createPayload } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import type { BaseResponsePayload } from '../../../system/core/types/ResponseTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { generateUUID } from '../../../system/core/types/CrossPlatformUUID';

// Existing architecture imports
import type { DataDaemon, DataOperationContext } from '../../data-daemon/shared/DataDaemon';
import type { StorageResult } from '../../data-daemon/shared/DataStorageAdapter';
import { UserRepository, HumanUserRepository, AgentUserRepository, PersonaUserRepository } from '../../../domain/user/UserRepository';
import type { BaseUser, UserCitizenType } from '../../../domain/user/BaseUser';
import type { HumanUser } from '../../../domain/user/HumanUser';
import type { AgentUser } from '../../../domain/user/AgentUser';
import type { PersonaUser } from '../../../domain/user/PersonaUser';

// ORM imports
import { ORM, EntityRepository } from '../../../orm/shared/BaseORM';
import {
    BaseUserEntityMetadata,
    UserSessionEntityMetadata,
    UserPermissionEntityMetadata,
    UserRelationshipEntityMetadata,
    ChatRoomEntityMetadata,
    RoomParticipationEntityMetadata,
    ChatMessageEntityMetadata,
    MessageReactionEntityMetadata,
    TypingIndicatorEntityMetadata,
    UserPresenceEntityMetadata,
    UserProfileEntityMetadata
} from '../../../orm/entities/UserEntityMappings';

// Relationship imports
import type {
    UserSession,
    UserRelationship,
    MessageReaction,
    TypingIndicator,
    UserPresence,
    UserProfile,
    PersonaConfig,
    AgentPortalConfig,
    AIModelConfig
} from '../../../domain/user/UserRelationships';

/**
 * Specialized Entity Repositories
 */
export class UserSessionRepository extends EntityRepository<UserSession> {
    constructor(dataDaemon: DataDaemon) {
        super(dataDaemon, UserSessionEntityMetadata);
    }

    protected generateId(): UUID {
        return generateUUID();
    }

    async findActiveByUserId(userId: UUID, context: DataOperationContext): Promise<StorageResult<UserSession[]>> {
        const query = this.createQuery()
            .where('userId', '=', userId)
            .where('isActive', '=', true)
            .orderBy('lastActivityAt', 'desc');

        return await this.query(query, context);
    }
}

export class UserRelationshipRepository extends EntityRepository<UserRelationship> {
    constructor(dataDaemon: DataDaemon) {
        super(dataDaemon, UserRelationshipEntityMetadata);
    }

    protected generateId(): UUID {
        return generateUUID();
    }

    async findFriends(userId: UUID, context: DataOperationContext): Promise<StorageResult<UserRelationship[]>> {
        const query = this.createQuery()
            .where('fromUserId', '=', userId)
            .where('relationshipType', '=', 'friend')
            .where('status', '=', 'active');

        return await this.query(query, context);
    }

    async findBlocked(userId: UUID, context: DataOperationContext): Promise<StorageResult<UserRelationship[]>> {
        const query = this.createQuery()
            .where('fromUserId', '=', userId)
            .where('relationshipType', '=', 'blocked')
            .where('status', '=', 'active');

        return await this.query(query, context);
    }

    async findPendingFriendRequests(userId: UUID, context: DataOperationContext): Promise<StorageResult<UserRelationship[]>> {
        const query = this.createQuery()
            .where('toUserId', '=', userId)
            .where('relationshipType', '=', 'friend_request')
            .where('status', '=', 'pending');

        return await this.query(query, context);
    }
}

export class MessageReactionRepository extends EntityRepository<MessageReaction> {
    constructor(dataDaemon: DataDaemon) {
        super(dataDaemon, MessageReactionEntityMetadata);
    }

    protected generateId(): UUID {
        return generateUUID();
    }

    async findByMessageId(messageId: UUID, context: DataOperationContext): Promise<StorageResult<MessageReaction[]>> {
        const query = this.createQuery()
            .where('messageId', '=', messageId)
            .orderBy('reactedAt', 'asc');

        return await this.query(query, context);
    }

    async findByUserAndMessage(userId: UUID, messageId: UUID, context: DataOperationContext): Promise<StorageResult<MessageReaction | null>> {
        const query = this.createQuery()
            .where('userId', '=', userId)
            .where('messageId', '=', messageId)
            .limit(1);

        const result = await this.query(query, context);
        if (!result.success || !result.data || result.data.length === 0) {
            return { success: true, data: null };
        }

        return { success: true, data: result.data[0] };
    }
}

export class TypingIndicatorRepository extends EntityRepository<TypingIndicator> {
    constructor(dataDaemon: DataDaemon) {
        super(dataDaemon, TypingIndicatorEntityMetadata);
    }

    protected generateId(): UUID {
        return generateUUID();
    }

    async findActiveInRoom(roomId: UUID, context: DataOperationContext): Promise<StorageResult<TypingIndicator[]>> {
        const now = new Date().toISOString();
        const query = this.createQuery()
            .where('roomId', '=', roomId)
            .where('expiresAt', '>', now);

        return await this.query(query, context);
    }

    async cleanupExpired(_context: DataOperationContext): Promise<StorageResult<number>> {
        // This would need a custom delete query - for now return count 0
        return { success: true, data: 0 };
    }
}

export class UserPresenceRepository extends EntityRepository<UserPresence> {
    constructor(dataDaemon: DataDaemon) {
        super(dataDaemon, UserPresenceEntityMetadata);
    }

    protected generateId(): UUID {
        return generateUUID();
    }

    async findByUserId(userId: UUID, context: DataOperationContext): Promise<StorageResult<UserPresence | null>> {
        const query = this.createQuery()
            .where('userId', '=', userId)
            .limit(1);

        const result = await this.query(query, context);
        if (!result.success || !result.data || result.data.length === 0) {
            return { success: true, data: null };
        }

        return { success: true, data: result.data[0] };
    }

    async findOnlineUsers(context: DataOperationContext): Promise<StorageResult<UserPresence[]>> {
        const query = this.createQuery()
            .where('status', '!=', 'offline')
            .where('status', '!=', 'invisible')
            .orderBy('lastSeenAt', 'desc');

        return await this.query(query, context);
    }
}

/**
 * UserDaemon - Complete User Management System
 */
export class UserDaemon extends DaemonBase {
    public readonly subpath = 'user';

    // Existing repositories (leveraged)
    private userRepository: UserRepository;
    private humanRepository: HumanUserRepository;
    private agentRepository: AgentUserRepository;
    private personaRepository: PersonaUserRepository;

    // ORM system
    private orm: ORM;

    // Specialized relationship repositories
    protected userSessionRepo: UserSessionRepository;
    protected userRelationshipRepo: UserRelationshipRepository;
    protected messageReactionRepo: MessageReactionRepository;
    protected typingIndicatorRepo: TypingIndicatorRepository;
    protected userPresenceRepo: UserPresenceRepository;
    protected userProfileRepo: EntityRepository<UserProfile>;

    constructor(context: JTAGContext, router: JTAGRouter, private dataDaemon: DataDaemon) {
        super('user-daemon', context, router);

        // Initialize existing repositories
        this.userRepository = new UserRepository(dataDaemon);
        this.humanRepository = new HumanUserRepository(dataDaemon);
        this.agentRepository = new AgentUserRepository(dataDaemon);
        this.personaRepository = new PersonaUserRepository(dataDaemon);

        // Initialize ORM
        this.orm = new ORM(dataDaemon);
        this.registerAllEntities();

        // Initialize specialized repositories
        this.userSessionRepo = new UserSessionRepository(dataDaemon);
        this.userRelationshipRepo = new UserRelationshipRepository(dataDaemon);
        this.messageReactionRepo = new MessageReactionRepository(dataDaemon);
        this.typingIndicatorRepo = new TypingIndicatorRepository(dataDaemon);
        this.userPresenceRepo = new UserPresenceRepository(dataDaemon);
        this.userProfileRepo = new (class extends EntityRepository<UserProfile> {
            protected generateId(): UUID { return generateUUID(); }
        })(dataDaemon, UserProfileEntityMetadata);
    }

    private registerAllEntities(): void {
        // Register all entity metadata
        this.orm.registerEntity('BaseUser', BaseUserEntityMetadata);
        this.orm.registerEntity('UserSession', UserSessionEntityMetadata);
        this.orm.registerEntity('UserPermission', UserPermissionEntityMetadata);
        this.orm.registerEntity('UserRelationship', UserRelationshipEntityMetadata);
        this.orm.registerEntity('ChatRoom', ChatRoomEntityMetadata);
        this.orm.registerEntity('RoomParticipation', RoomParticipationEntityMetadata);
        this.orm.registerEntity('ChatMessage', ChatMessageEntityMetadata);
        this.orm.registerEntity('MessageReaction', MessageReactionEntityMetadata);
        this.orm.registerEntity('TypingIndicator', TypingIndicatorEntityMetadata);
        this.orm.registerEntity('UserPresence', UserPresenceEntityMetadata);
        this.orm.registerEntity('UserProfile', UserProfileEntityMetadata);
    }

    /**
     * Initialize daemon functionality
     */
    protected async initialize(): Promise<void> {
        // Initialize ORM schema if needed
        // Could add schema generation/migration logic here
        await Promise.resolve();
    }

    /**
     * Handle incoming messages
     */
    async handleMessage(message: JTAGMessage): Promise<BaseResponsePayload> {
        // Handle user-related messages
        // This would route to specific user operations based on message type
        return createPayload(this.context, message.payload.sessionId, {
            success: false,
            timestamp: new Date().toISOString(), // Keep as string for response payload
            error: `UserDaemon message handling not implemented for: ${message.endpoint ?? 'unknown'}`
        });
    }

    // ========================================================================
    // USER CREATION (Leverages existing repositories)
    // ========================================================================

    async createHuman(
        displayName: string,
        sessionId: UUID,
        context: DataOperationContext
    ): Promise<StorageResult<HumanUser>> {
        return await this.humanRepository.createHuman(displayName, sessionId, context);
    }

    async createAgent(
        displayName: string,
        sessionId: UUID,
        agentPortalConfig: AgentPortalConfig,
        modelConfig: AIModelConfig,
        context: DataOperationContext
    ): Promise<StorageResult<AgentUser>> {
        return await this.agentRepository.createAgent(displayName, sessionId, agentPortalConfig, modelConfig, context);
    }

    async createPersona(
        displayName: string,
        sessionId: UUID,
        personaConfig: PersonaConfig,
        modelConfig: AIModelConfig,
        context: DataOperationContext
    ): Promise<StorageResult<PersonaUser>> {
        return await this.personaRepository.createPersona(displayName, sessionId, personaConfig, modelConfig, context);
    }

    // ========================================================================
    // USER QUERIES (Leverages existing repositories)
    // ========================================================================

    async findUserById(userId: UUID, context: DataOperationContext): Promise<StorageResult<BaseUser | null>> {
        return await this.userRepository.findById(userId, context);
    }

    async findUsersByType(citizenType: UserCitizenType, context: DataOperationContext, aiType?: 'agent' | 'persona'): Promise<StorageResult<BaseUser[]>> {
        return await this.userRepository.findByType(citizenType, context, aiType);
    }

    async findActiveUsers(context: DataOperationContext): Promise<StorageResult<BaseUser[]>> {
        return await this.userRepository.findActiveUsers(context);
    }

    // ========================================================================
    // USER RELATIONSHIPS (New ORM functionality)
    // ========================================================================

    async sendFriendRequest(fromUserId: UUID, toUserId: UUID, context: DataOperationContext): Promise<StorageResult<UserRelationship>> {
        return await this.userRelationshipRepo.create({
            fromUserId,
            toUserId,
            relationshipType: 'friend_request',
            status: 'pending'
        }, context);
    }

    async acceptFriendRequest(userId: UUID, fromUserId: UUID, context: DataOperationContext): Promise<StorageResult<UserRelationship>> {
        // Find the pending friend request
        const query = this.userRelationshipRepo.createQuery()
            .where('fromUserId', '=', fromUserId)
            .where('toUserId', '=', userId)
            .where('relationshipType', '=', 'friend_request')
            .where('status', '=', 'pending')
            .limit(1);

        const requestResult = await this.userRelationshipRepo.query(query, context);
        if (!requestResult.success || !requestResult.data || requestResult.data.length === 0) {
            return {
                success: false,
                error: 'Friend request not found'
            };
        }

        const request = requestResult.data[0];

        // Update to accepted status and change type to friend
        return await this.userRelationshipRepo.update(request.id, {
            relationshipType: 'friend',
            status: 'active'
        }, context);
    }

    async blockUser(fromUserId: UUID, toUserId: UUID, context: DataOperationContext): Promise<StorageResult<UserRelationship>> {
        return await this.userRelationshipRepo.create({
            fromUserId,
            toUserId,
            relationshipType: 'blocked',
            status: 'active'
        }, context);
    }

    async getFriends(userId: UUID, context: DataOperationContext): Promise<StorageResult<UserRelationship[]>> {
        return await this.userRelationshipRepo.findFriends(userId, context);
    }

    async getBlocked(userId: UUID, context: DataOperationContext): Promise<StorageResult<UserRelationship[]>> {
        return await this.userRelationshipRepo.findBlocked(userId, context);
    }

    async getPendingFriendRequests(userId: UUID, context: DataOperationContext): Promise<StorageResult<UserRelationship[]>> {
        return await this.userRelationshipRepo.findPendingFriendRequests(userId, context);
    }

    // ========================================================================
    // MESSAGE REACTIONS
    // ========================================================================

    async addReaction(userId: UUID, messageId: UUID, emoji: string, context: DataOperationContext): Promise<StorageResult<MessageReaction>> {
        return await this.messageReactionRepo.create({
            userId,
            messageId,
            emoji,
            reactionType: 'unicode',
            reactedAt: new Date()
        }, context);
    }

    async removeReaction(userId: UUID, messageId: UUID, emoji: string, context: DataOperationContext): Promise<StorageResult<void>> {
        const existingResult = await this.messageReactionRepo.findByUserAndMessage(userId, messageId, context);
        if (!existingResult.success || !existingResult.data) {
            return {
                success: false,
                error: 'Reaction not found'
            };
        }

        return await this.messageReactionRepo.delete(existingResult.data.id, context);
    }

    async getMessageReactions(messageId: UUID, context: DataOperationContext): Promise<StorageResult<MessageReaction[]>> {
        return await this.messageReactionRepo.findByMessageId(messageId, context);
    }

    // ========================================================================
    // TYPING INDICATORS
    // ========================================================================

    async setTyping(userId: UUID, roomId: UUID, context: DataOperationContext): Promise<StorageResult<TypingIndicator>> {
        const expiresAt = new Date(Date.now() + 10000); // 10 seconds

        return await this.typingIndicatorRepo.create({
            userId,
            roomId,
            startedAt: new Date(),
            expiresAt
        }, context);
    }

    async clearTyping(userId: UUID, roomId: UUID, context: DataOperationContext): Promise<StorageResult<void>> {
        // Find and delete the typing indicator
        const query = this.typingIndicatorRepo.createQuery()
            .where('userId', '=', userId)
            .where('roomId', '=', roomId)
            .limit(1);

        const result = await this.typingIndicatorRepo.query(query, context);
        if (result.success && result.data && result.data.length > 0) {
            return await this.typingIndicatorRepo.delete(result.data[0].id, context);
        }

        return { success: true, data: undefined };
    }

    async getTypingUsers(roomId: UUID, context: DataOperationContext): Promise<StorageResult<TypingIndicator[]>> {
        return await this.typingIndicatorRepo.findActiveInRoom(roomId, context);
    }

    // ========================================================================
    // USER PRESENCE
    // ========================================================================

    async updateUserPresence(
        userId: UUID,
        status: UserPresence['status'],
        context: DataOperationContext,
        customStatus?: string
    ): Promise<StorageResult<UserPresence>> {
        const existingResult = await this.userPresenceRepo.findByUserId(userId, context);

        const presenceData = {
            userId,
            status,
            customStatus,
            lastSeenAt: new Date(),
            currentActivity: undefined
        };

        if (existingResult.success && existingResult.data) {
            // Update existing presence
            return await this.userPresenceRepo.update(existingResult.data.id, presenceData, context);
        } else {
            // Create new presence
            return await this.userPresenceRepo.create(presenceData, context);
        }
    }

    async getUserPresence(userId: UUID, context: DataOperationContext): Promise<StorageResult<UserPresence | null>> {
        return await this.userPresenceRepo.findByUserId(userId, context);
    }

    async getOnlineUsers(context: DataOperationContext): Promise<StorageResult<UserPresence[]>> {
        return await this.userPresenceRepo.findOnlineUsers(context);
    }

    // ========================================================================
    // SCHEMA MANAGEMENT
    // ========================================================================

    async generateDatabaseSchema(context: DataOperationContext): Promise<StorageResult<string[]>> {
        return await this.orm.generateSchema(context);
    }

    // ========================================================================
    // EXISTING REPOSITORY ACCESS (Backward compatibility)
    // ========================================================================

    getUserRepository(): UserRepository {
        return this.userRepository;
    }

    getHumanRepository(): HumanUserRepository {
        return this.humanRepository;
    }

    getAgentRepository(): AgentUserRepository {
        return this.agentRepository;
    }

    getPersonaRepository(): PersonaUserRepository {
        return this.personaRepository;
    }

    getORM(): ORM {
        return this.orm;
    }
}