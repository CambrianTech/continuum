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
import { DataDaemon, type DataOperationContext } from '../../data-daemon/shared/DataDaemon';
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
 * UserDaemon - Complete User Management System with Clean Static Interface
 *
 * Following the proven DataDaemon.store() pattern with:
 * - Auto-context injection
 * - Strict TypeScript typing
 * - Integration with authentication, chat, and state management
 */
export class UserDaemon extends DaemonBase {
    // Static interface for clean daemon access
    private static sharedInstance: UserDaemon | null = null;
    private static context: DataOperationContext | null = null;

    /**
     * Initialize UserDaemon static interface - called by system during startup
     */
    static initialize(instance: UserDaemon, context: DataOperationContext): void {
        UserDaemon.sharedInstance = instance;
        UserDaemon.context = context;
    }

    static isInitialized(): boolean {
        return UserDaemon.sharedInstance !== null && UserDaemon.context !== null;
    }

    // ========================================================================
    // CLEAN STATIC USER OPERATIONS - Following DataDaemon.store() pattern
    // ========================================================================

    /**
     * Create Human user - type-safe, no generics
     */
    static async createHuman(
        displayName: string,
        sessionId: UUID
    ): Promise<StorageResult<HumanUser>> {
        UserDaemon.ensureInitialized();
        return UserDaemon.sharedInstance!.createHuman(displayName, sessionId, UserDaemon.context!);
    }

    /**
     * Create Agent user - type-safe, no generics
     */
    static async createAgent(
        displayName: string,
        sessionId: UUID,
        agentPortalConfig: AgentPortalConfig,
        modelConfig: AIModelConfig
    ): Promise<StorageResult<AgentUser>> {
        UserDaemon.ensureInitialized();
        return UserDaemon.sharedInstance!.createAgent(
            displayName,
            sessionId,
            agentPortalConfig,
            modelConfig,
            UserDaemon.context!
        );
    }

    /**
     * Create Persona user - type-safe, no generics
     */
    static async createPersona(
        displayName: string,
        sessionId: UUID,
        personaConfig: PersonaConfig,
        modelConfig: AIModelConfig
    ): Promise<StorageResult<PersonaUser>> {
        UserDaemon.ensureInitialized();
        return UserDaemon.sharedInstance!.createPersona(
            displayName,
            sessionId,
            personaConfig,
            modelConfig,
            UserDaemon.context!
        );
    }

    /**
     * Create user with automatic polymorphic instantiation
     * Returns BaseUser - caller must cast if they need specific type
     * Rust-like: explicit about what we're returning
     */
    static async create(userData: {
        displayName: string;
        sessionId: UUID;
        citizenType: UserCitizenType;
        aiType?: 'agent' | 'persona';
        agentPortalConfig?: AgentPortalConfig;
        modelConfig?: AIModelConfig;
        personaConfig?: PersonaConfig;
    }): Promise<StorageResult<BaseUser>> {
        if (userData.citizenType === 'human') {
            return UserDaemon.createHuman(userData.displayName, userData.sessionId);
        } else if (userData.citizenType === 'ai' && userData.aiType === 'agent') {
            if (!userData.agentPortalConfig || !userData.modelConfig) {
                return {
                    success: false,
                    error: 'Agent users require agentPortalConfig and modelConfig'
                };
            }
            return UserDaemon.createAgent(
                userData.displayName,
                userData.sessionId,
                userData.agentPortalConfig,
                userData.modelConfig
            );
        } else if (userData.citizenType === 'ai' && userData.aiType === 'persona') {
            if (!userData.personaConfig || !userData.modelConfig) {
                return {
                    success: false,
                    error: 'Persona users require personaConfig and modelConfig'
                };
            }
            return UserDaemon.createPersona(
                userData.displayName,
                userData.sessionId,
                userData.personaConfig,
                userData.modelConfig
            );
        }

        return {
            success: false,
            error: `Unsupported user type: ${userData.citizenType}${userData.aiType ? '/' + userData.aiType : ''}`
        };
    }

    /**
     * Find user by ID - critical for authentication and chat
     */
    static async findById(userId: UUID): Promise<StorageResult<BaseUser | null>> {
        UserDaemon.ensureInitialized();
        return UserDaemon.sharedInstance!.findUserById(userId, UserDaemon.context!);
    }

    /**
     * Query users with type-safe filters - for chat participant lists
     * Returns BaseUser[] - caller can cast if needed
     * Rust-like: explicit return type, no unsafe generics
     */
    static async query(filters: {
        citizenType?: UserCitizenType;
        aiType?: 'agent' | 'persona';
        sessionId?: UUID;
        isActive?: boolean;
        limit?: number;
    } = {}): Promise<StorageResult<BaseUser[]>> {
        UserDaemon.ensureInitialized();

        if (filters.citizenType) {
            const result = await UserDaemon.sharedInstance!.findUsersByType(
                filters.citizenType,
                UserDaemon.context!,
                filters.aiType
            );
            return result;
        }

        if (filters.isActive) {
            const result = await UserDaemon.sharedInstance!.findActiveUsers(UserDaemon.context!);
            return result;
        }

        // For sessionId filtering, use DataDaemon directly
        if (filters.sessionId) {
            const queryResult = await DataDaemon.query<{userId: UUID; sessionId: UUID}>({
                collection: 'users',
                filters: { sessionId: filters.sessionId },
                limit: filters.limit
            });

            if (!queryResult.success || !queryResult.data) {
                return { success: false, error: queryResult.error };
            }

            // Convert each found user using the findById method to get proper domain objects
            const users: BaseUser[] = [];
            for (const record of queryResult.data) {
                const userResult = await UserDaemon.findById(record.data.userId);
                if (userResult.success && userResult.data) {
                    users.push(userResult.data);
                }
            }

            return {
                success: true,
                data: users
            };
        }

        // Default: return active users
        const result = await UserDaemon.sharedInstance!.findActiveUsers(UserDaemon.context!);
        return result;
    }

    /**
     * Authenticate user by session - critical for chat and state management
     */
    static async authenticateBySession(sessionId: UUID): Promise<StorageResult<BaseUser | null>> {
        const result = await UserDaemon.query({ sessionId, limit: 1 });

        if (!result.success) {
            return { success: false, error: result.error };
        }

        return {
            success: true,
            data: result.data?.[0] ?? null
        };
    }

    /**
     * Get or create session user - handles both human and AI users for chat
     */
    static async getOrCreateSessionUser(
        sessionId: UUID,
        userType: UserCitizenType = 'human',
        displayName?: string
    ): Promise<StorageResult<BaseUser>> {
        // First try to find existing user for session
        const existing = await UserDaemon.authenticateBySession(sessionId);

        if (existing.success && existing.data) {
            return existing as StorageResult<BaseUser>;
        }

        // Create new user for session
        const userData = {
            displayName: displayName ?? `${userType}-${sessionId.substring(0, 8)}`,
            sessionId,
            citizenType: userType
        };

        if (userType === 'ai') {
            // Default to agent type for AI users
            return UserDaemon.create({
                ...userData,
                aiType: 'agent' as const,
                agentPortalConfig: {
                    portalType: 'api',
                    endpoint: 'https://api.anthropic.com/v1',
                    config: {}
                },
                modelConfig: {
                    provider: 'anthropic',
                    model: 'claude-3-sonnet',
                    temperature: 0.7,
                    maxTokens: 4096
                }
            });
        }

        return UserDaemon.create(userData);
    }

    /**
     * Get user presence - for chat online/offline status
     */
    static async getPresence(userId: UUID): Promise<StorageResult<UserPresence | null>> {
        UserDaemon.ensureInitialized();
        return UserDaemon.sharedInstance!.getUserPresence(userId, UserDaemon.context!);
    }

    /**
     * Update user presence - for chat status integration
     */
    static async updatePresence(
        userId: UUID,
        status: UserPresence['status'],
        customStatus?: string
    ): Promise<StorageResult<UserPresence>> {
        UserDaemon.ensureInitialized();
        return UserDaemon.sharedInstance!.updateUserPresence(userId, status, UserDaemon.context!, customStatus);
    }

    /**
     * Get online users - for chat participant lists
     */
    static async getOnlineUsers(): Promise<StorageResult<UserPresence[]>> {
        UserDaemon.ensureInitialized();
        return UserDaemon.sharedInstance!.getOnlineUsers(UserDaemon.context!);
    }

    /**
     * Set typing indicator - for chat real-time features
     */
    static async setTyping(userId: UUID, roomId: UUID): Promise<StorageResult<TypingIndicator>> {
        UserDaemon.ensureInitialized();
        return UserDaemon.sharedInstance!.setTyping(userId, roomId, UserDaemon.context!);
    }

    /**
     * Clear typing indicator - for chat real-time features
     */
    static async clearTyping(userId: UUID, roomId: UUID): Promise<StorageResult<void>> {
        UserDaemon.ensureInitialized();
        return UserDaemon.sharedInstance!.clearTyping(userId, roomId, UserDaemon.context!);
    }

    /**
     * Get typing users in room - for chat real-time features
     */
    static async getTypingUsers(roomId: UUID): Promise<StorageResult<TypingIndicator[]>> {
        UserDaemon.ensureInitialized();
        return UserDaemon.sharedInstance!.getTypingUsers(roomId, UserDaemon.context!);
    }

    /**
     * Add message reaction - for chat interactions
     */
    static async addReaction(userId: UUID, messageId: UUID, emoji: string): Promise<StorageResult<MessageReaction>> {
        UserDaemon.ensureInitialized();
        return UserDaemon.sharedInstance!.addReaction(userId, messageId, emoji, UserDaemon.context!);
    }

    /**
     * Remove message reaction - for chat interactions
     */
    static async removeReaction(userId: UUID, messageId: UUID, emoji: string): Promise<StorageResult<void>> {
        UserDaemon.ensureInitialized();
        return UserDaemon.sharedInstance!.removeReaction(userId, messageId, emoji, UserDaemon.context!);
    }

    /**
     * Get message reactions - for chat display
     */
    static async getMessageReactions(messageId: UUID): Promise<StorageResult<MessageReaction[]>> {
        UserDaemon.ensureInitialized();
        return UserDaemon.sharedInstance!.getMessageReactions(messageId, UserDaemon.context!);
    }

    private static ensureInitialized(): void {
        if (!UserDaemon.sharedInstance || !UserDaemon.context) {
            throw new Error('UserDaemon not initialized - system must call UserDaemon.initialize() first');
        }
    }
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