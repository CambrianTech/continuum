/**
 * HumanUser - Represents a human user (browser or test client)
 *
 * Per CONTINUUM-STATE-ARCHITECTURE.md:
 * - Humans get persistent home directories like personas
 * - State persists per-project via SQLite (open tabs, preferences)
 * - Same .continuum structure as other user types
 *
 * Follows ARCHITECTURE-RULES.md:
 * - Clean inheritance from BaseUser
 * - Environment-agnostic (shared directory)
 * - SQLite storage for persistence (server-side)
 */

import { BaseUser } from './BaseUser';
import { UserEntity } from '../../data/entities/UserEntity';
import { UserStateEntity } from '../../data/entities/UserStateEntity';
import type { IUserStateStorage } from '../storage/IUserStateStorage';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { JTAGContext } from '../../core/types/JTAGTypes';
import type { JTAGRouter } from '../../core/router/shared/JTAGRouter';
import type { UserCreateParams } from '../../../commands/user/create/shared/UserCreateTypes';
import { ORM } from '../../../daemons/data-daemon/server/ORM';
import { COLLECTIONS } from '../../data/config/DatabaseConfig';
import { MemoryStateBackend } from '../storage/MemoryStateBackend';
import { getDefaultCapabilitiesForType, getDefaultPreferencesForType } from '../config/UserCapabilitiesDefaults';
import { SystemPaths } from '../../core/config/SystemPaths';
import { DEFAULT_USERS, DEFAULT_USER_UNIQUE_IDS } from '../../data/domains/DefaultEntities';

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
   * Implementation of abstract homeDirectory getter from BaseUser
   * HumanUsers live in the 'users/' directory with persistent state
   * Returns ABSOLUTE path via SystemPaths - THE SINGLE SOURCE OF TRUTH
   */
  get homeDirectory(): string {
    return SystemPaths.users.dir(this.entity.uniqueId);
  }

  /**
   * Initialize human user
   */
  async initialize(): Promise<void> {
    await super.initialize();
    // Humans interact via browser - no additional server-side initialization needed
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

    // Use deterministic UUID for known system users (single source of truth)
    // This ensures DEFAULT_USERS.HUMAN matches the seeded Joel user
    if (params.uniqueId === DEFAULT_USER_UNIQUE_IDS.PRIMARY_HUMAN) {
      userEntity.id = DEFAULT_USERS.HUMAN;
    }
    // Note: other id fields handled by constructor if not explicitly set

    const storedEntity = await ORM.store<UserEntity>(
      COLLECTIONS.USERS,
      userEntity
    );

    // STEP 2: Create UserStateEntity (human-specific defaults)
    const userState = this.getDefaultState(storedEntity.id);
    userState.preferences = getDefaultPreferencesForType('human');

    const storedState = await ORM.store<UserStateEntity>(
      COLLECTIONS.USER_STATES,
      userState
    );

    // STEP 3: Room membership now handled by RoomMembershipDaemon via events
    // User creation → data:users:created event → RoomMembershipDaemon auto-joins user

    // STEP 4: Add to additional rooms if specified
    if (params.addToRooms && params.addToRooms.length > 0) {
      for (const roomId of params.addToRooms) {
        await this.addToRoom(storedEntity.id, roomId, params.displayName);
      }
    }

    // STEP 5: Create HumanUser instance with SQLite storage (persistent)
    // Use SQLite on server, Memory in browser
    let storage: IUserStateStorage;
    if (typeof window === 'undefined') {
      // Server environment - use SQLite for persistence
      const { SQLiteStateBackend } = require('../storage/server/SQLiteStateBackend');
      storage = new SQLiteStateBackend(SystemPaths.users.state(params.uniqueId));
    } else {
      // Browser environment - use memory (browser state handled separately)
      storage = new MemoryStateBackend();
    }
    return new HumanUser(storedEntity, storedState, storage);
  }

}