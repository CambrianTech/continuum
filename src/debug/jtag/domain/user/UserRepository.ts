/**
 * User Repository - Adapter-agnostic ORM for BaseUser hierarchy
 *
 * Works with ANY DataDaemon adapter: SQL (PostgreSQL, SQLite), NoSQL (MongoDB),
 * File (JSON), Memory, Network, etc. The DataDaemon handles adapter selection,
 * this repository provides domain-specific operations.
 *
 * Adapter Compatibility:
 * - SQL: Relationships via foreign keys, JOINs, transactions
 * - NoSQL: Embedded documents, references, eventual consistency
 * - JSON: File-based storage with manual relationship resolution
 * - Memory: In-memory maps with immediate consistency
 */

import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import { DataDaemon, type DataOperationContext } from '../../daemons/data-daemon/shared/DataDaemon';
import type { StorageResult, StorageQuery } from '../../daemons/data-daemon/shared/DataStorageAdapter';
import { BaseUser, type UserCitizenType } from './BaseUser';
import { HumanUser, type HumanUserData } from './HumanUser';
import { AgentUser, type AgentUserData } from './AgentUser';
import { PersonaUser, type PersonaUserData } from './PersonaUser';
import { SystemUser, type SystemUserData } from './SystemUser';
import type { AIUserData } from './AIUser';
import { AdapterAwareQueryBuilder, type UniversalQueryOptions, type AdapterType } from './AdapterAwareQueryBuilder';
import type {
  BaseUserDataWithRelationships,
  HumanUserDataWithRelationships,
  AIUserDataWithRelationships,
  PersonaUserDataWithRelationships,
  AgentUserDataWithRelationships,
  UserSession,
  UserPermission,
  RoomParticipation,
  ChatRoom,
  ChatMessage,
  PersonaConfig,
  AIModelConfig
} from './UserRelationships';
import type { AgentSpecialization } from './AgentUser';

/**
 * Base Repository for User operations
 */
export class UserRepository {
  private adapterType: AdapterType = 'unknown';

  constructor(private dataDaemon: DataDaemon) {
    // Detect adapter type from DataDaemon configuration
    this.adapterType = this.detectAdapterType();
  }

  /**
   * Detect what type of adapter we're using for query optimization
   */
  private detectAdapterType(): AdapterType {
    // Access the internal adapter type through DataDaemon
    // This would need to be exposed by DataDaemon or detected another way
    const config = (this.dataDaemon as any).config;
    if (config) {
      const strategy = config.strategy;
      const backend = config.backend;

      if (strategy === 'sql' || backend?.includes('sql') || backend?.includes('sqlite') || backend?.includes('postgres')) {
        return 'sql';
      }
      if (strategy === 'nosql' || backend?.includes('mongo') || backend?.includes('couch')) {
        return 'nosql';
      }
      if (strategy === 'file' || backend === 'json') {
        return 'json';
      }
      if (strategy === 'memory') {
        return 'memory';
      }
      if (strategy === 'network') {
        return 'network';
      }
    }

    return 'unknown';
  }

  /**
   * Execute query using adapter-aware query builder
   */
  private async executeAdapterQuery<T>(
    collection: string,
    options: UniversalQueryOptions,
    context: DataOperationContext
  ): Promise<StorageResult<T[]>> {
    const query = AdapterAwareQueryBuilder.buildQuery(collection, options, this.adapterType);
    const result = await this.dataDaemon.query<T>(query, context);

    if (!result.success || !result.data) {
      return { success: false, error: result.error };
    }

    // Extract data from DataRecords
    const extractedData = result.data.map(record => record.data);
    return {
      success: true,
      data: extractedData
    };
  }

  // ============================================================================
  // CORE USER CRUD OPERATIONS
  // ============================================================================

  /**
   * Create a new user (polymorphic - creates correct subtype)
   */
  async createUser<T extends BaseUser>(
    userData: BaseUserDataWithRelationships,
    context: DataOperationContext
  ): Promise<StorageResult<T>> {
    // USE NEW CLEAN INTERFACE - DataDaemon.store() with auto-context
    const result = await DataDaemon.store('users', userData);

    if (!result.success || !result.data) {
      return { success: false, error: result.error };
    }

    // Convert to proper domain object
    const domainUser = this.mapToDomainObject(result.data.data) as T;
    return {
      success: true,
      data: domainUser
    };
  }

  /**
   * Find user by ID with optional relationship loading
   */
  async findById<T extends BaseUser>(
    userId: UUID,
    context: DataOperationContext,
    options: {
      includeSessions?: boolean;
      includePermissions?: boolean;
      includeRoomParticipations?: boolean;
    } = {}
  ): Promise<StorageResult<T | null>> {
    // USE NEW CLEAN INTERFACE - DataDaemon.read() with auto-context
    const result = await DataDaemon.read('users', userId);

    if (!result.success) {
      return { success: false, error: result.error };
    }

    if (!result.data) {
      return { success: true, data: null };
    }

    let userData = result.data.data;

    // Load relationships if requested
    if (options.includeSessions) {
      const sessions = await this.findUserSessions(userId, context);
      if (sessions.success && sessions.data) {
        userData = { ...(userData as object), sessions: sessions.data };
      }
    }

    if (options.includePermissions) {
      const permissions = await this.findUserPermissions(userId, context);
      if (permissions.success && permissions.data) {
        userData = { ...(userData as object), permissions: permissions.data };
      }
    }

    if (options.includeRoomParticipations) {
      const participations = await this.findUserRoomParticipations(userId, context);
      if (participations.success && participations.data) {
        userData = { ...(userData as object), roomParticipations: participations.data };
      }
    }

    const domainUser = this.mapToDomainObject(userData as BaseUserDataWithRelationships) as T;
    return {
      success: true,
      data: domainUser
    };
  }

  /**
   * Find users by type - Uses adapter-aware query builder
   */
  async findByType<T extends BaseUser>(
    citizenType: UserCitizenType,
    context: DataOperationContext,
    aiType?: 'agent' | 'persona'
  ): Promise<StorageResult<T[]>> {
    const queryOptions = AdapterAwareQueryBuilder.queryUsersByType(citizenType, aiType);
    const result = await this.executeAdapterQuery<BaseUserDataWithRelationships>('users', queryOptions, context);

    if (!result.success || !result.data) {
      return { success: false, error: result.error };
    }

    const domainUsers = result.data.map(userData => this.mapToDomainObject(userData)) as T[];
    return {
      success: true,
      data: domainUsers
    };
  }

  /**
   * Find active users (online within last hour) - Uses adapter-aware query builder
   */
  async findActiveUsers<T extends BaseUser>(context: DataOperationContext): Promise<StorageResult<T[]>> {
    const oneHourAgo = new Date(Date.now() - 3600000).toISOString();
    const queryOptions = AdapterAwareQueryBuilder.queryActiveUsers(oneHourAgo);
    const result = await this.executeAdapterQuery<BaseUserDataWithRelationships>('users', queryOptions, context);

    if (!result.success || !result.data) {
      return { success: false, error: result.error };
    }

    const domainUsers = result.data.map(userData => this.mapToDomainObject(userData)) as T[];
    return {
      success: true,
      data: domainUsers
    };
  }

  /**
   * Update user
   */
  async updateUser<T extends BaseUser>(
    userId: UUID,
    updates: Partial<BaseUserDataWithRelationships>,
    context: DataOperationContext
  ): Promise<StorageResult<T>> {
    const result = await this.dataDaemon.update('users', userId, updates, context);

    if (!result.success || !result.data) {
      return { success: false, error: result.error };
    }

    const domainUser = this.mapToDomainObject(result.data.data) as T;
    return {
      success: true,
      data: domainUser
    };
  }

  // ============================================================================
  // RELATIONSHIP OPERATIONS
  // ============================================================================

  /**
   * Get user sessions
   */
  async findUserSessions(userId: UUID, context: DataOperationContext): Promise<StorageResult<UserSession[]>> {
    const query: StorageQuery = {
      collection: 'user_sessions',
      filters: { userId },
      sort: [{ field: 'startedAt', direction: 'desc' }]
    };

    const result = await this.dataDaemon.query(query, context);

    if (!result.success || !result.data) {
      return { success: false, error: result.error };
    }

    return {
      success: true,
      data: result.data.map(record => this.convertToUserSession(record.data))
    };
  }

  /**
   * Get user permissions
   */
  async findUserPermissions(userId: UUID, context: DataOperationContext): Promise<StorageResult<UserPermission[]>> {
    const query: StorageQuery = {
      collection: 'user_permissions',
      filters: { userId }
    };

    const result = await this.dataDaemon.query(query, context);

    if (!result.success || !result.data) {
      return { success: false, error: result.error };
    }

    return {
      success: true,
      data: result.data.map(record => this.convertToUserPermission(record.data))
    };
  }

  /**
   * Get user room participations
   */
  async findUserRoomParticipations(userId: UUID, context: DataOperationContext): Promise<StorageResult<RoomParticipation[]>> {
    const query: StorageQuery = {
      collection: 'room_participations',
      filters: { userId, isActive: true },
      sort: [{ field: 'joinedAt', direction: 'desc' }]
    };

    const result = await this.dataDaemon.query(query, context);

    if (!result.success || !result.data) {
      return { success: false, error: result.error };
    }

    return {
      success: true,
      data: result.data.map(record => this.convertToRoomParticipation(record.data))
    };
  }

  /**
   * Check if user has permission
   */
  async hasPermission(
    userId: UUID,
    permission: string,
    resource: string,
    context: DataOperationContext
  ): Promise<StorageResult<boolean>> {
    const query: StorageQuery = {
      collection: 'user_permissions',
      filters: {
        userId,
        permission,
        $or: [
          { resource },
          { resource: '*' } // Wildcard permissions
        ]
      },
      limit: 1
    };

    const result = await this.dataDaemon.query(query, context);

    if (!result.success) {
      return { success: false, error: result.error };
    }

    return {
      success: true,
      data: (result.data?.length || 0) > 0
    };
  }

  /**
   * Grant permission to user
   */
  async grantPermission(
    userId: UUID,
    permission: string,
    resource: string,
    grantedBy: UUID,
    context: DataOperationContext,
    expiresAt?: string
  ): Promise<StorageResult<UserPermission>> {
    const permissionData: UserPermission = {
      id: crypto.randomUUID() as UUID,
      createdAt: new Date(),
      updatedAt: new Date(),
      userId,
      permission,
      resource,
      grantedBy,
      grantedAt: new Date(),
      expiresAt: expiresAt ? (typeof expiresAt === 'string' ? new Date(expiresAt) : expiresAt) : undefined
    };

    const result = await this.dataDaemon.create('user_permissions', permissionData, context);

    if (!result.success || !result.data) {
      return { success: false, error: result.error };
    }

    return {
      success: true,
      data: this.convertToUserPermission(result.data.data)
    };
  }

  // ============================================================================
  // DOMAIN OBJECT MAPPING
  // ============================================================================

  /**
   * Map database data to proper domain object (polymorphic)
   */
  private mapToDomainObject(userData: BaseUserDataWithRelationships): BaseUser {
    // Convert Date objects to strings for User classes
    const convertedData = {
      ...userData,
      createdAt: userData.createdAt instanceof Date ? userData.createdAt.toISOString() : userData.createdAt
    };

    if (userData.citizenType === 'human') {
      return new HumanUser(convertedData as HumanUserData);
    } else if (userData.citizenType === 'ai') {
      const aiUserData = convertedData as AIUserData;
      const aiUserDataWithRelationships = userData as AIUserDataWithRelationships;

      if (aiUserDataWithRelationships.aiType === 'agent') {
        // Add missing properties for AgentUser
        const agentData = {
          ...aiUserData,
          aiType: 'agent' as const,
          specialization: 'software-development' as AgentSpecialization,
          toolAccess: [],
          automationLevel: 'assisted' as const,
          maxConcurrentTasks: 5
        };
        return new AgentUser(agentData);
      } else if (aiUserDataWithRelationships.aiType === 'persona') {
        // Add missing properties for PersonaUser
        const personaData = {
          ...aiUserData,
          aiType: 'persona' as const,
          personaStyle: 'friendly-helper' as const,
          contextualMemory: {
            conversationHistory: [],
            userPreferences: {},
            interactionStyle: {},
            domainKnowledge: []
          },
          adaptivePersonality: true,
          emotionalIntelligence: 75,
          conversationalDepth: 'moderate' as const
        };
        return new PersonaUser(personaData);
      } else {
        throw new Error(`Unknown AI type: ${aiUserDataWithRelationships.aiType}`);
      }
    } else if (userData.citizenType === 'system') {
      // Add missing properties for SystemUser
      const systemData: SystemUserData = {
        ...convertedData,
        citizenType: 'system' as const,
        systemRole: 'general',
        autoMessageTypes: ['welcome', 'instructions']
      };
      return new SystemUser(systemData);
    }

    throw new Error(`Unknown citizen type: ${userData.citizenType}`);
  }

  /**
   * Conversion helpers for DataRecord to domain types
   */
  private convertToUserSession(data: unknown): UserSession {
    const record = data as any;
    return {
      ...record,
      startedAt: new Date(record.startedAt),
      lastActivityAt: new Date(record.lastActivityAt),
      endedAt: record.endedAt ? new Date(record.endedAt) : undefined
    } as UserSession;
  }

  private convertToUserPermission(data: unknown): UserPermission {
    const record = data as any;
    return {
      ...record,
      grantedAt: new Date(record.grantedAt),
      expiresAt: record.expiresAt ? new Date(record.expiresAt) : undefined
    } as UserPermission;
  }

  private convertToRoomParticipation(data: unknown): RoomParticipation {
    const record = data as any;
    return {
      ...record,
      joinedAt: new Date(record.joinedAt),
      leftAt: record.leftAt ? new Date(record.leftAt) : undefined,
      lastMessageAt: record.lastMessageAt ? new Date(record.lastMessageAt) : undefined
    } as RoomParticipation;
  }
}

/**
 * Specialized repositories for each user type
 */

export class HumanUserRepository extends UserRepository {
  async createHuman(
    displayName: string,
    sessionId: UUID,
    context: DataOperationContext
  ): Promise<StorageResult<HumanUser>> {
    const userData: HumanUserDataWithRelationships = {
      userId: crypto.randomUUID() as UUID,
      sessionId,
      displayName,
      citizenType: 'human',
      capabilities: [],
      createdAt: new Date(),
      lastActiveAt: new Date().toISOString(),
      preferences: {},
      isOnline: true
    };

    return this.createUser<HumanUser>(userData, context);
  }
}

export class AgentUserRepository extends UserRepository {
  async createAgent(
    displayName: string,
    sessionId: UUID,
    agentPortalConfig: AgentUserDataWithRelationships['agentPortalConfig'],
    modelConfig: AIUserDataWithRelationships['modelConfig'],
    context: DataOperationContext
  ): Promise<StorageResult<AgentUser>> {
    const userData: AgentUserDataWithRelationships = {
      userId: crypto.randomUUID() as UUID,
      sessionId,
      displayName,
      citizenType: 'ai',
      aiType: 'agent',
      capabilities: [],
      createdAt: new Date(),
      lastActiveAt: new Date().toISOString(),
      preferences: {},
      isOnline: true,
      modelConfig,
      agentPortalConfig
    };

    return this.createUser<AgentUser>(userData, context);
  }
}

export class PersonaUserRepository extends UserRepository {
  async createPersona(
    displayName: string,
    sessionId: UUID,
    personaConfig: PersonaConfig,
    modelConfig: AIModelConfig,
    context: DataOperationContext
  ): Promise<StorageResult<PersonaUser>> {
    const userData: PersonaUserDataWithRelationships = {
      userId: crypto.randomUUID() as UUID,
      sessionId,
      displayName,
      citizenType: 'ai',
      aiType: 'persona',
      capabilities: [],
      createdAt: new Date(),
      lastActiveAt: new Date().toISOString(),
      preferences: {},
      isOnline: true,
      modelConfig,
      personaConfig
    };

    return this.createUser<PersonaUser>(userData, context);
  }
}
