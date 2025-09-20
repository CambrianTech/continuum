/**
 * UserDaemon Browser Implementation
 * Handles user management operations in browser environment
 */

import { UserDaemonBase } from '../shared/UserDaemonBase';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { BaseUser } from '../../../domain/user/BaseUser';
import type { StorageResult } from '../../data-daemon/shared/DataStorageAdapter';
import type { UserOperationContext, UserQueryParams } from '../shared/UserDaemonTypes';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';

export class UserDaemonBrowser extends UserDaemonBase {
  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  async createHuman(
    displayName: string,
    sessionId: UUID,
    context: UserOperationContext
  ): Promise<StorageResult<BaseUser>> {
    // Browser delegates to server via commands
    throw new Error('UserDaemonBrowser: createHuman not implemented - should use commands');
  }

  async createAgent(
    displayName: string,
    agentType: string,
    context: UserOperationContext
  ): Promise<StorageResult<BaseUser>> {
    // Browser delegates to server via commands
    throw new Error('UserDaemonBrowser: createAgent not implemented - should use commands');
  }

  async query(
    params: UserQueryParams,
    context: UserOperationContext
  ): Promise<StorageResult<BaseUser[]>> {
    // Browser delegates to server via commands
    throw new Error('UserDaemonBrowser: query not implemented - should use commands');
  }

  async getById(
    userId: UUID,
    context: UserOperationContext
  ): Promise<StorageResult<BaseUser | null>> {
    // Browser delegates to server via commands
    throw new Error('UserDaemonBrowser: getById not implemented - should use commands');
  }

  async updatePresence(
    userId: UUID,
    isOnline: boolean,
    lastActiveAt: string,
    context: UserOperationContext
  ): Promise<StorageResult<void>> {
    // Browser delegates to server via commands
    throw new Error('UserDaemonBrowser: updatePresence not implemented - should use commands');
  }

  async getPresence(
    userId: UUID,
    context: UserOperationContext
  ): Promise<StorageResult<{ isOnline: boolean; lastActiveAt: string }>> {
    // Browser delegates to server via commands
    throw new Error('UserDaemonBrowser: getPresence not implemented - should use commands');
  }
}