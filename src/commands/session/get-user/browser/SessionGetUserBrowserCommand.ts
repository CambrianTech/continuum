/**
 * Session Get User Command - Browser Implementation
 *
 * Pass-through to server - no browser-specific logic needed.
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { SessionGetUserParams, SessionGetUserResult } from '../shared/SessionGetUserTypes';

export class SessionGetUserBrowserCommand extends CommandBase<SessionGetUserParams, SessionGetUserResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('session/get-user', context, subpath, commander);
  }

  async execute(params: SessionGetUserParams): Promise<SessionGetUserResult> {
    return await this.remoteExecute(params);
  }
}
