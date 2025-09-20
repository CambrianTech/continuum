/**
 * UserDaemon Base - Abstract base class for user management
 * Follows JTAG daemon pattern with proper message handling
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { BaseUser } from '../../../domain/user/BaseUser';
import type { StorageResult } from '../../data-daemon/shared/DataStorageAdapter';
import type { UserOperationContext, UserQueryParams } from './UserDaemonTypes';
import type { JTAGContext, JTAGMessage, JTAGPayload } from '../../../system/core/types/JTAGTypes';
import { createPayload } from '../../../system/core/types/JTAGTypes';
import { DaemonBase } from '../../command-daemon/shared/DaemonBase';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import type { BaseResponsePayload } from '../../../system/core/types/ResponseTypes';
import { createBaseResponse } from '../../../system/core/types/ResponseTypes';

/**
 * User operation payload types
 */
export interface UserOperationPayload extends JTAGPayload {
  readonly operation: 'create_human' | 'create_agent' | 'query' | 'get_by_id' | 'update_presence' | 'get_presence';
  readonly displayName?: string;
  readonly userId?: UUID;
  readonly agentType?: string;
  readonly queryParams?: UserQueryParams;
  readonly isOnline?: boolean;
  readonly lastActiveAt?: string;
}

export const createUserOperationPayload = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<UserOperationPayload>, 'context' | 'sessionId'>
): UserOperationPayload => createPayload(context, sessionId, {
  operation: data.operation ?? 'query',
  ...data
});

export abstract class UserDaemonBase extends DaemonBase {
  public readonly subpath: string = 'user';

  constructor(context: JTAGContext, router: JTAGRouter) {
    super('user-daemon', context, router);
  }

  /**
   * Static convenience method - same pattern as CommandDaemon.execute()
   * Server-side interface for user operations with automatic context injection
   */
  static async execute<T>(
    operation: 'create_human' | 'create_agent' | 'query' | 'get_by_id' | 'update_presence' | 'get_presence',
    params: any
  ): Promise<T> {
    // For server-side, we'll use DataDaemon.store() and other established patterns
    // This will be implemented once we have the proper server-side equivalent
    // of the browser's window.jtag interface
    throw new Error('UserDaemon.execute() not yet implemented - need server-side equivalent of CommandDaemon.execute()');
  }

  /**
   * Handle incoming user operation messages
   */
  async handleMessage(message: JTAGMessage): Promise<BaseResponsePayload> {
    const payload = message.payload as UserOperationPayload;
    const userContext = this.createUserContext();

    try {
      let result: StorageResult<any>;

      switch (payload.operation) {
        case 'create_human':
          result = await this.createHuman(
            payload.displayName!,
            payload.sessionId!,
            userContext
          );
          break;
        case 'create_agent':
          result = await this.createAgent(
            payload.displayName!,
            payload.agentType!,
            userContext
          );
          break;
        case 'query':
          result = await this.query(
            payload.queryParams!,
            userContext
          );
          break;
        case 'get_by_id':
          result = await this.getById(
            payload.userId!,
            userContext
          );
          break;
        case 'update_presence':
          result = await this.updatePresence(
            payload.userId!,
            payload.isOnline!,
            payload.lastActiveAt!,
            userContext
          );
          break;
        case 'get_presence':
          result = await this.getPresence(
            payload.userId!,
            userContext
          );
          break;
        default:
          result = {
            success: false,
            error: `Unknown user operation: ${payload.operation}`
          };
      }

      return createBaseResponse(result.success, this.context, payload.sessionId!, {
        ...result
      });

    } catch (error: any) {
      return createBaseResponse(false, this.context, payload.sessionId!, {
        error: `User daemon error: ${error.message}`
      });
    }
  }

  /**
   * Initialize daemon-specific functionality
   */
  protected async initialize(): Promise<void> {
    console.log(`✅ ${this.toString()}: UserDaemon initialized`);
  }

  /**
   * Create user operation context
   */
  protected createUserContext(source: string = 'user-daemon'): UserOperationContext {
    return {
      sessionId: this.context.uuid,
      timestamp: new Date().toISOString(),
      source
    };
  }

  // User creation operations
  abstract createHuman(
    displayName: string,
    sessionId: UUID,
    context: UserOperationContext
  ): Promise<StorageResult<BaseUser>>;

  abstract createAgent(
    displayName: string,
    agentType: string,
    context: UserOperationContext
  ): Promise<StorageResult<BaseUser>>;

  // Query operations
  abstract query(
    params: UserQueryParams,
    context: UserOperationContext
  ): Promise<StorageResult<BaseUser[]>>;

  abstract getById(
    userId: UUID,
    context: UserOperationContext
  ): Promise<StorageResult<BaseUser | null>>;

  // Presence operations
  abstract updatePresence(
    userId: UUID,
    isOnline: boolean,
    lastActiveAt: string,
    context: UserOperationContext
  ): Promise<StorageResult<void>>;

  abstract getPresence(
    userId: UUID,
    context: UserOperationContext
  ): Promise<StorageResult<{ isOnline: boolean; lastActiveAt: string }>>;
}