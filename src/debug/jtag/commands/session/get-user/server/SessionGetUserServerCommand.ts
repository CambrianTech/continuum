/**
 * Session Get User Command - Server Implementation
 *
 * Looks up a user from a sessionId.
 * Uses SessionDaemon to get userId, then looks up UserEntity.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload, JTAGMessageFactory } from '../../../../system/core/types/JTAGTypes';
import type { SessionGetUserParams, SessionGetUserResult } from '../shared/SessionGetUserTypes';
import type { GetSessionParams, GetSessionResult } from '../../../../daemons/session-daemon/shared/SessionTypes';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import type { UserEntity } from '../../../../system/data/entities/UserEntity';
import { COLLECTIONS } from '../../../../system/data/config/DatabaseConfig';

export class SessionGetUserServerCommand extends CommandBase<SessionGetUserParams, SessionGetUserResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('session/get-user', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<SessionGetUserResult> {
    const getUserParams = params as SessionGetUserParams;

    try {
      // Use targetSessionId if provided, otherwise use caller's sessionId
      const lookupSessionId = getUserParams.targetSessionId || getUserParams.sessionId;

      // Get SessionDaemon to look up session metadata
      const sessionDaemon = this.commander.router.getSubscriber('session-daemon');

      if (!sessionDaemon) {
        return transformPayload(getUserParams, {
          success: false,
          error: 'SessionDaemon not available'
        });
      }

      // Create message to get session from daemon
      const sessionMessage = JTAGMessageFactory.createRequest(
        this.context,
        'server/session-daemon',
        'session-daemon/get',
        { sessionId: lookupSessionId, operation: 'get' } as GetSessionParams,
        JTAGMessageFactory.generateCorrelationId()
      );

      // Call SessionDaemon to get session metadata
      const sessionResponse = await sessionDaemon.handleMessage(sessionMessage) as GetSessionResult;

      if (!sessionResponse.success || !sessionResponse.session) {
        return transformPayload(getUserParams, {
          success: false,
          error: sessionResponse.error || `Session not found: ${lookupSessionId}`
        });
      }

      // Extract userId from session metadata
      const userId = sessionResponse.session.userId;

      // Look up user entity from database
      const userResult = await DataDaemon.read<UserEntity>(COLLECTIONS.USERS, userId);

      if (!userResult.success || !userResult.data) {
        return transformPayload(getUserParams, {
          success: false,
          error: `User not found: ${userId}`
        });
      }

      // Extract user entity from DataRecord
      const user = userResult.data.data as UserEntity;

      return transformPayload(getUserParams, {
        success: true,
        user: user
      });

    } catch (error) {
      return transformPayload(getUserParams, {
        success: false,
        error: `Failed to get user from session: ${error instanceof Error ? error.message : String(error)}`
      });
    }
  }
}
