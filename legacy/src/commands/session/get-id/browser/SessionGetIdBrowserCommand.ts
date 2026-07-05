/**
 * Session Get ID Command - Browser Implementation
 *
 * Browser-side pass-through for session/get-id command.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import type { SessionGetIdParams, SessionGetIdResult } from '../shared/SessionGetIdTypes';

export class SessionGetIdBrowserCommand extends CommandBase<SessionGetIdParams, SessionGetIdResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('session/get-id', context, subpath, commander);
  }

  async execute(params: SessionGetIdParams): Promise<SessionGetIdResult> {
    // Browser always delegates to server
    return await this.remoteExecute(params);
  }
}
