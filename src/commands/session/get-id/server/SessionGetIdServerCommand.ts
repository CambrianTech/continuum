/**
 * Session Get ID Command - Server Implementation
 *
 * Returns the caller's session ID.
 * Ultra-simple convenience command with no parameters.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { SessionGetIdParams, SessionGetIdResult } from '../shared/SessionGetIdTypes';

export class SessionGetIdServerCommand extends CommandBase<SessionGetIdParams, SessionGetIdResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('session/get-id', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<SessionGetIdResult> {
    const getIdParams = params as SessionGetIdParams;

    try {
      // Simply return success - sessionId is already in the result via JTAGPayload
      return transformPayload(getIdParams, {
        success: true
      });
    } catch (error) {
      return transformPayload(getIdParams, {
        success: false,
        error: `Failed to get session ID: ${error instanceof Error ? error.message : String(error)}`
      });
    }
  }
}
