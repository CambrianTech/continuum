/**
 * User Get Me Command - Server Implementation
 *
 * Returns the caller's full user information.
 * Ultra-simple convenience command that internally calls session/get-user.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UserGetMeParams, UserGetMeResult } from '../shared/UserGetMeTypes';
import { Commands } from '../../../../system/core/shared/Commands';
import type { SessionGetUserParams, SessionGetUserResult } from '../../../session/get-user/shared/SessionGetUserTypes';

import { SessionGetUser } from '../../../session/get-user/shared/SessionGetUserTypes';
export class UserGetMeServerCommand extends CommandBase<UserGetMeParams, UserGetMeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('user/get-me', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<UserGetMeResult> {
    const getMeParams = params as UserGetMeParams;

    try {
      // Call session/get-user without targetSessionId - uses caller's auto-injected sessionId
      const userResult = await SessionGetUser.execute({}) as SessionGetUserResult;

      if (!userResult.success || !userResult.user) {
        return transformPayload(getMeParams, {
          success: false,
          error: userResult.error ?? 'Failed to look up user from session'
        });
      }

      return transformPayload(getMeParams, {
        success: true,
        user: userResult.user
      });
    } catch (error) {
      return transformPayload(getMeParams, {
        success: false,
        error: `Failed to get user info: ${error instanceof Error ? error.message : String(error)}`
      });
    }
  }
}
