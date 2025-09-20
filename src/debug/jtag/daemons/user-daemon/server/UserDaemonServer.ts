/**
 * UserDaemon Server Implementation
 * Handles actual user management operations with business logic
 */

import { UserDaemonBase } from '../shared/UserDaemonBase';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { generateUUID } from '../../../system/core/types/CrossPlatformUUID';
import type { BaseUser, BaseUserData } from '../../../domain/user/BaseUser';
import { HumanUser } from '../../../domain/user/HumanUser';
import { PersonaUser, type PersonaStyle } from '../../../domain/user/PersonaUser';
import { AgentUser, type AgentSpecialization } from '../../../domain/user/AgentUser';
import type { StorageResult } from '../../data-daemon/shared/DataStorageAdapter';
import type { UserOperationContext, UserQueryParams } from '../shared/UserDaemonTypes';
import type { UserData } from '../../../system/data/domains/User';
import { ISOString, UserId, CitizenId, SessionId } from '../../../system/data/domains/CoreTypes';
import type { AIModelConfig } from '../../../domain/user/UserRelationships';
import { DataDaemon } from '../../data-daemon/shared/DataDaemon';
import { COLLECTIONS } from '../../../system/data/core/FieldMapping';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';

export class UserDaemonServer extends UserDaemonBase {
  private liveUsers = new Map<UUID, BaseUser>(); // Living user object registry

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  async createHuman(
    displayName: string,
    sessionId: UUID,
    context: UserOperationContext
  ): Promise<StorageResult<BaseUser>> {
    try {
      // Create new HumanUser instance
      const userId = generateUUID();
      const humanUser = HumanUser.createNew(displayName, sessionId);

      // Store in live registry
      this.liveUsers.set(userId, humanUser);

      // Persist to database as UserData (for UI layer)
      const userData = this.serializeToUserData(humanUser);
      const storeResult = await DataDaemon.store(COLLECTIONS.USERS, userData, userId);

      if (!storeResult.success) {
        // Rollback live registry on storage failure
        this.liveUsers.delete(userId);
        return { success: false, error: `Failed to persist user: ${storeResult.error}` };
      }

      console.log(`✅ UserDaemonServer: Created HumanUser ${displayName} (${userId})`);
      return { success: true, data: humanUser };

    } catch (error) {
      return { success: false, error: `Failed to create human user: ${error instanceof Error ? error.message : String(error)}` };
    }
  }

  async createAgent(
    displayName: string,
    agentType: string,
    context: UserOperationContext
  ): Promise<StorageResult<BaseUser>> {
    try {
      // Create new AgentUser instance
      const userId = generateUUID();
      const specialization: AgentSpecialization = agentType as AgentSpecialization;

      // Default AI model config for agents
      const modelConfig: AIModelConfig = {
        provider: 'anthropic',
        model: 'claude-3-sonnet',
        temperature: 0.7,
        systemPrompt: `You are a specialized ${specialization} agent.`
      };

      const agentUser = AgentUser.createNew(displayName, context.sessionId, specialization, modelConfig);

      // Store in live registry
      this.liveUsers.set(userId, agentUser);

      // Persist to database as UserData (for UI layer)
      const userData = this.serializeToUserData(agentUser);
      const storeResult = await DataDaemon.store(COLLECTIONS.USERS, userData, userId);

      if (!storeResult.success) {
        // Rollback live registry on storage failure
        this.liveUsers.delete(userId);
        return { success: false, error: `Failed to persist agent: ${storeResult.error}` };
      }

      console.log(`✅ UserDaemonServer: Created AgentUser ${displayName} (${agentType}) (${userId})`);
      return { success: true, data: agentUser };

    } catch (error) {
      return { success: false, error: `Failed to create agent user: ${error instanceof Error ? error.message : String(error)}` };
    }
  }

  async createPersona(
    displayName: string,
    personaStyle: PersonaStyle,
    sessionId: UUID,
    context: UserOperationContext,
    systemPrompt?: string,
    modelConfig?: AIModelConfig
  ): Promise<StorageResult<BaseUser>> {
    try {
      // Create new PersonaUser instance
      const userId = generateUUID();

      // Default AI model config for personas if not provided
      const defaultModelConfig: AIModelConfig = {
        provider: 'anthropic',
        model: 'claude-3-haiku',
        temperature: 0.8,
        systemPrompt: systemPrompt || `You are a ${personaStyle} persona with distinct personality traits.`
      };

      const finalModelConfig = modelConfig || defaultModelConfig;
      const personaUser = PersonaUser.createNew(displayName, sessionId, personaStyle, finalModelConfig);

      // Store in live registry
      this.liveUsers.set(userId, personaUser);

      // Persist to database as UserData (for UI layer)
      const userData = this.serializeToUserData(personaUser);
      const storeResult = await DataDaemon.store(COLLECTIONS.USERS, userData, userId);

      if (!storeResult.success) {
        // Rollback live registry on storage failure
        this.liveUsers.delete(userId);
        return { success: false, error: `Failed to persist persona: ${storeResult.error}` };
      }

      console.log(`✅ UserDaemonServer: Created PersonaUser ${displayName} (${personaStyle}) (${userId})`);
      return { success: true, data: personaUser };

    } catch (error) {
      return { success: false, error: `Failed to create persona user: ${error instanceof Error ? error.message : String(error)}` };
    }
  }

  async query(
    params: UserQueryParams,
    context: UserOperationContext
  ): Promise<StorageResult<BaseUser[]>> {
    try {
      // Convert params to DataDaemon query
      const query = {
        collection: COLLECTIONS.USERS,
        filter: {
          ...(params.isActive !== undefined && { isActive: params.isActive }),
          ...(params.citizenType && { citizenType: params.citizenType }),
          ...(params.sessionId && { sessionId: params.sessionId })
        },
        limit: params.limit || 100,
        orderBy: params.orderBy || [{ field: 'lastActiveAt', direction: 'desc' }]
      };

      const result = await DataDaemon.query(query);

      if (!result.success) {
        return { success: false, error: `Query failed: ${result.error}` };
      }

      // Deserialize data records back to BaseUser instances
      const users = result.data?.map(record => this.deserializeUser(record.data)) || [];

      return { success: true, data: users };
    } catch (error) {
      return { success: false, error: `UserDaemonServer.query failed: ${error instanceof Error ? error.message : String(error)}` };
    }
  }

  async getById(
    userId: UUID,
    context: UserOperationContext
  ): Promise<StorageResult<BaseUser | null>> {
    try {
      const result = await DataDaemon.read(COLLECTIONS.USERS, userId);

      if (!result.success) {
        return { success: false, error: `Failed to get user: ${result.error}` };
      }

      if (!result.data) {
        return { success: true, data: null };
      }

      const user = this.deserializeUser(result.data.data);
      return { success: true, data: user };
    } catch (error) {
      return { success: false, error: `UserDaemonServer.getById failed: ${error instanceof Error ? error.message : String(error)}` };
    }
  }

  async updatePresence(
    userId: UUID,
    isOnline: boolean,
    lastActiveAt: string,
    context: UserOperationContext
  ): Promise<StorageResult<void>> {
    try {
      const result = await DataDaemon.update(COLLECTIONS.USERS, userId, {
        isOnline,
        lastActiveAt
      });

      if (!result.success) {
        return { success: false, error: `Failed to update presence: ${result.error}` };
      }

      return { success: true, data: undefined };
    } catch (error) {
      return { success: false, error: `UserDaemonServer.updatePresence failed: ${error instanceof Error ? error.message : String(error)}` };
    }
  }

  async getPresence(
    userId: UUID,
    context: UserOperationContext
  ): Promise<StorageResult<{ isOnline: boolean; lastActiveAt: string }>> {
    try {
      const result = await DataDaemon.read(COLLECTIONS.USERS, userId);

      if (!result.success) {
        return { success: false, error: `Failed to get presence: ${result.error}` };
      }

      if (!result.data) {
        return { success: false, error: 'User not found' };
      }

      const userData = result.data.data as any;
      return {
        success: true,
        data: {
          isOnline: userData.isOnline || false,
          lastActiveAt: userData.lastActiveAt || new Date().toISOString()
        }
      };
    } catch (error) {
      return { success: false, error: `UserDaemonServer.getPresence failed: ${error instanceof Error ? error.message : String(error)}` };
    }
  }

  async getUserDataById(
    userId: UUID,
    context: UserOperationContext
  ): Promise<StorageResult<UserData | null>> {
    try {
      const result = await DataDaemon.read(COLLECTIONS.USERS, userId);

      if (!result.success) {
        return { success: false, error: `Failed to get user data: ${result.error}` };
      }

      if (!result.data) {
        return { success: true, data: null };
      }

      // Return the stored UserData directly (it's already serialized)
      const userData = result.data.data as UserData;
      return { success: true, data: userData };
    } catch (error) {
      return { success: false, error: `UserDaemonServer.getUserDataById failed: ${error instanceof Error ? error.message : String(error)}` };
    }
  }

  /**
   * Serialize BaseUser to UserData for UI layer persistence
   * This creates the "shadow/recipe" that widgets use for display
   */
  private serializeToUserData(user: BaseUser): UserData {
    const baseData = user.toData();

    return {
      // BaseEntity fields (required by UserData interface)
      id: user.userId,
      createdAt: ISOString(baseData.createdAt),
      updatedAt: ISOString(new Date().toISOString()),
      version: 1,

      // UserData specific fields
      userId: UserId(user.userId),
      citizenId: CitizenId(user.userId), // Same for now
      type: this.mapCitizenTypeToUserType(baseData.citizenType),

      profile: {
        displayName: user.displayName,
        avatar: this.getAvatarForCitizenType(baseData.citizenType),
        joinedAt: ISOString(baseData.createdAt)
      },

      capabilities: this.mapCapabilitiesForUI(user),

      preferences: {
        theme: 'auto' as const,
        language: 'en',
        timezone: 'UTC',
        notifications: {
          mentions: true,
          directMessages: true,
          roomUpdates: user.citizenType === 'ai'
        },
        privacy: {
          showOnlineStatus: true,
          allowDirectMessages: true,
          shareActivity: user.citizenType === 'ai'
        }
      },

      status: user.isOnline ? 'online' as const : 'offline' as const,
      lastActiveAt: ISOString(baseData.lastActiveAt),
      sessionsActive: [SessionId(baseData.sessionId)]
    };
  }

  /**
   * Deserialize database data back to BaseUser instances
   * This recreates the living user objects from stored data
   */
  private deserializeUser(userData: any): BaseUser {
    const baseData: BaseUserData = {
      userId: userData.userId || userData.id,
      sessionId: userData.sessionsActive?.[0] || userData.sessionId,
      displayName: userData.profile?.displayName || userData.displayName,
      citizenType: this.mapUserTypeToCitizenType(userData.type),
      capabilities: userData.capabilities?.canSendMessages ? [] : [], // Map UI capabilities back
      createdAt: userData.profile?.joinedAt || userData.createdAt,
      lastActiveAt: userData.lastActiveAt,
      preferences: userData.preferences || {},
      isOnline: userData.status === 'online'
    };

    // Create appropriate user type based on citizenType
    switch (baseData.citizenType) {
      case 'human':
        return HumanUser.fromData(baseData as any);
      case 'ai':
        // For now, create as PersonaUser - later distinguish between PersonaUser and AgentUser
        return PersonaUser.fromData(baseData as any);
      default:
        throw new Error(`Unsupported citizen type: ${baseData.citizenType}`);
    }
  }

  /**
   * Helper methods for type mapping
   */
  private mapCitizenTypeToUserType(citizenType: string): 'human' | 'ai' | 'system' {
    switch (citizenType) {
      case 'human': return 'human';
      case 'ai':
      case 'persona': return 'ai';
      case 'system': return 'system';
      default: return 'human';
    }
  }

  private mapUserTypeToCitizenType(userType: string): 'human' | 'ai' | 'system' {
    return userType as 'human' | 'ai' | 'system';
  }

  private getAvatarForCitizenType(citizenType: string): string {
    switch (citizenType) {
      case 'human': return '👤';
      case 'ai':
      case 'persona': return '🎭';
      case 'system': return '⚙️';
      default: return '👤';
    }
  }

  private mapCapabilitiesForUI(user: BaseUser) {
    return {
      canSendMessages: true,
      canReceiveMessages: true,
      canCreateRooms: user.citizenType === 'human',
      canInviteOthers: user.citizenType === 'human',
      canModerate: false,
      autoResponds: user.citizenType === 'ai',
      providesContext: user.citizenType === 'ai',
      canTrain: user.constructor.name === 'PersonaUser',
      canAccessPersonas: user.citizenType === 'human'
    };
  }
}