/**
 * UserDaemon Server Implementation
 * Handles actual user management operations with business logic
 */

import { UserDaemonBase } from '../shared/UserDaemonBase';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { BaseUser } from '../../../domain/user/BaseUser';
import type { StorageResult } from '../../data-daemon/shared/DataStorageAdapter';
import type { UserOperationContext, UserQueryParams } from '../shared/UserDaemonTypes';
import { DataDaemon } from '../../data-daemon/shared/DataDaemon';
import { COLLECTIONS } from '../../../system/data/core/FieldMapping';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';

export class UserDaemonServer extends UserDaemonBase {
  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  async createHuman(
    displayName: string,
    sessionId: UUID,
    context: UserOperationContext
  ): Promise<StorageResult<BaseUser>> {
    // Stub implementation - use existing UserRepository for now
    return { success: false, error: 'UserDaemonServer.createHuman not fully implemented yet' };
  }

  async createAgent(
    displayName: string,
    agentType: string,
    context: UserOperationContext
  ): Promise<StorageResult<BaseUser>> {
    // Stub implementation - use existing UserRepository for now
    return { success: false, error: 'UserDaemonServer.createAgent not fully implemented yet' };
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

  private deserializeUser(userData: any): BaseUser {
    // Stub implementation - would reconstruct BaseUser instances from serialized data
    throw new Error('UserDaemonServer.deserializeUser not implemented yet');
  }
}