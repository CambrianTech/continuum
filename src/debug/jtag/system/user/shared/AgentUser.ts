/**
 * AgentUser - External AI portal connections (Claude, GPT, etc.)
 *
 * Follows ARCHITECTURE-RULES.md:
 * - Clean inheritance from AIUser
 * - Ephemeral state (in-memory storage)
 */

import { AIUser } from './AIUser';
import { UserEntity } from '../../data/entities/UserEntity';
import { UserStateEntity } from '../../data/entities/UserStateEntity';
import type { IUserStateStorage } from '../storage/IUserStateStorage';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { JTAGContext } from '../../core/types/JTAGTypes';
import type { JTAGRouter } from '../../core/router/shared/JTAGRouter';
import type { UserCreateParams } from '../../../commands/user/create/shared/UserCreateTypes';
import { DataDaemon } from '../../../daemons/data-daemon/shared/DataDaemon';
import { COLLECTIONS } from '../../data/config/DatabaseConfig';
import { MemoryStateBackend } from '../storage/MemoryStateBackend';
import { getDefaultCapabilitiesForType, getDefaultPreferencesForType } from '../config/UserCapabilitiesDefaults';

/**
 * AgentUser class for external AI agents
 * Used for Claude, GPT, and other external AI portal connections
 */
export class AgentUser extends AIUser {
  get isAgent(): boolean {
    return true;
  }

  /**
   * AgentUser creation recipe
   *
   * Simpler than PersonaUser - agents connect externally via WebSocket
   * They don't need persistent server-side instances
   */
  static async create(
    params: UserCreateParams,
    context: JTAGContext,
    router: JTAGRouter
  ): Promise<AgentUser> {
    console.log(`🤖 AgentUser.create: Creating agent "${params.displayName}"`);

    // STEP 1: Create UserEntity in database
    const userEntity = new UserEntity();
    userEntity.type = 'agent';
    userEntity.uniqueId = params.uniqueId;
    userEntity.displayName = params.displayName;
    userEntity.status = params.status ?? 'offline';  // Agents start offline until they connect
    userEntity.lastActiveAt = new Date();
    userEntity.capabilities = params.capabilities ?? getDefaultCapabilitiesForType('agent');
    userEntity.sessionsActive = [];
    // Optional extended fields for agents
    if (params.provider) {
      Object.assign(userEntity, { provider: params.provider });
    }
    if (params.modelConfig) {
      Object.assign(userEntity, { modelConfig: params.modelConfig });
    }
    // createdAt, updatedAt, version, id handled by constructor

    const storedEntity = await DataDaemon.store<UserEntity>(
      COLLECTIONS.USERS,
      userEntity
    );

    // STEP 2: Create UserStateEntity (agent-specific defaults - ephemeral)
    const userState = this.getDefaultState(storedEntity.id);
    userState.preferences = getDefaultPreferencesForType('agent');

    const storedState = await DataDaemon.store<UserStateEntity>(
      COLLECTIONS.USER_STATES,
      userState
    );

    // STEP 3: Create AgentUser instance (ephemeral, in-memory)
    const storage = new MemoryStateBackend();
    return new AgentUser(storedEntity, storedState, storage);
  }

}