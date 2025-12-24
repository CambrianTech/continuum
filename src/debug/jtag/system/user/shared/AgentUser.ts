/**
 * AgentUser - External AI portal connections (Claude, GPT, etc.)
 *
 * Per CONTINUUM-STATE-ARCHITECTURE.md:
 * - Agents get persistent home directories like personas
 * - State persists per-project via SQLite
 * - Same .continuum structure as other user types
 *
 * Follows ARCHITECTURE-RULES.md:
 * - Clean inheritance from AIUser
 * - SQLite storage for persistence (server-side)
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
import { SystemPaths } from '../../core/config/SystemPaths';

/**
 * AgentUser class for external AI agents
 * Used for Claude, GPT, and other external AI portal connections
 */
export class AgentUser extends AIUser {
  get isAgent(): boolean {
    return true;
  }

  /**
   * Implementation of abstract homeDirectory getter from BaseUser
   * AgentUsers live in the 'agents/' directory with persistent state
   * Returns ABSOLUTE path via SystemPaths - THE SINGLE SOURCE OF TRUTH
   */
  get homeDirectory(): string {
    return SystemPaths.agents.dir(this.entity.uniqueId);
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

    // STEP 3: Create AgentUser instance with SQLite storage (persistent)
    // Use SQLite on server, Memory in browser (though agents are typically server-only)
    let storage: IUserStateStorage;
    if (typeof window === 'undefined') {
      // Server environment - use SQLite for persistence
      const { SQLiteStateBackend } = require('../storage/server/SQLiteStateBackend');
      storage = new SQLiteStateBackend(SystemPaths.agents.state(params.uniqueId));
    } else {
      // Browser environment - fallback to memory (shouldn't happen for agents)
      storage = new MemoryStateBackend();
    }
    return new AgentUser(storedEntity, storedState, storage);
  }

}