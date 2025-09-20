/**
 * User Create Human Command - Browser Implementation
 *
 * Delegates to server for actual HumanUser creation via UserDaemonServer
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { UserCreateHumanParams, UserCreateHumanResult } from '../shared/UserCreateHumanTypes';

export class UserCreateHumanBrowserCommand extends CommandBase<UserCreateHumanParams, UserCreateHumanResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('user-create-human', context, subpath, commander);
  }

  async execute(params: UserCreateHumanParams): Promise<UserCreateHumanResult> {
    console.debug(`👤 USER BROWSER: Delegating HumanUser creation "${params.displayName}" to server`);

    // Delegate to server implementation
    return await this.remoteExecute(params);
  }
}