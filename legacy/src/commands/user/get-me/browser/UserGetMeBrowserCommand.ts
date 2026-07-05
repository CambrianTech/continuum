/**
 * User Get Me Command - Browser Implementation
 *
 * Browser-side pass-through for user/get-me command.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import type { UserGetMeParams, UserGetMeResult } from '../shared/UserGetMeTypes';

export class UserGetMeBrowserCommand extends CommandBase<UserGetMeParams, UserGetMeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('user/get-me', context, subpath, commander);
  }

  async execute(params: UserGetMeParams): Promise<UserGetMeResult> {
    // Browser always delegates to server
    return await this.remoteExecute(params);
  }
}
