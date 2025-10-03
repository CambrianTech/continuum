/**
 * HumanUser - Represents a human user (browser or test client)
 *
 * Follows ARCHITECTURE-RULES.md:
 * - Clean inheritance from BaseUser
 * - Environment-agnostic (shared directory)
 * - Storage backend injected (no hardcoded localStorage)
 */

import { BaseUser } from './BaseUser';
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
 * HumanUser class for human users
 * Used by both browser clients and test clients
 */
export class HumanUser extends BaseUser {
  get isHuman(): boolean {
    return true;
  }

  /**
   * Check if user is authenticated (has passkey)
   */
  get isAuthenticated(): boolean {
    // TODO: Implement passkey authentication check
    return false;
  }

  /**
   * HumanUser creation recipe
   *
   * Usually created via session/create, but can be created directly
   */
  static async create(
    params: UserCreateParams,
    context: JTAGContext,
    router: JTAGRouter
  ): Promise<HumanUser> {
    console.log(`👤 HumanUser.create: Creating human "${params.displayName}"`);

    // STEP 1: Create UserEntity in database
    const userEntity = new UserEntity();
    userEntity.type = 'human';
    userEntity.uniqueId = params.uniqueId;
    userEntity.displayName = params.displayName;
    userEntity.status = params.status ?? 'online';
    userEntity.lastActiveAt = new Date();
    userEntity.capabilities = params.capabilities ?? getDefaultCapabilitiesForType('human');
    userEntity.sessionsActive = [];
    // createdAt, updatedAt, version, id handled by constructor

    const storedEntity = await DataDaemon.store<UserEntity>(
      COLLECTIONS.USERS,
      userEntity
    );

    // STEP 2: Create UserStateEntity (human-specific defaults)
    const userState = this.getDefaultState(storedEntity.id);
    userState.preferences = getDefaultPreferencesForType('human');

    const storedState = await DataDaemon.store<UserStateEntity>(
      COLLECTIONS.USER_STATES,
      userState
    );

    // STEP 3: Create HumanUser instance (in-memory)
    const storage = new MemoryStateBackend();
    return new HumanUser(storedEntity, storedState, storage);
  }

}