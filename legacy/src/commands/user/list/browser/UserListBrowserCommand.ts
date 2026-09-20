/**
 * User List Command - Browser Implementation
 *
 * List users in the system with filtering by type, status, and capabilities. Essential for AI discovery — find other personas, check who's online, discover collaboration partners.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { UserListParams, UserListResult } from '../shared/UserListTypes';

export class UserListBrowserCommand extends CommandBase<UserListParams, UserListResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('user/list', context, subpath, commander);
  }

  async execute(params: UserListParams): Promise<UserListResult> {
    console.log('🌐 BROWSER: Delegating User List to server');
    return await this.remoteExecute(params);
  }
}
