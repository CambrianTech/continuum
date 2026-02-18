/**
 * User Create Command - Browser Implementation
 *
 * Thin browser-specific layer (5-10% of logic)
 * Delegates to server for actual user creation
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { UserCreateCommand } from '../shared/UserCreateCommand';
import type { UserCreateParams, UserCreateResult } from '../shared/UserCreateTypes';

export class UserCreateBrowserCommand extends UserCreateCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('user-create', context, subpath, commander);
  }

  /**
   * Browser-side execution
   * Routes to server for actual creation
   */
  async execute(params: UserCreateParams): Promise<UserCreateResult> {
    // Browser delegates all user creation to server
    return await this.remoteExecute<UserCreateParams, UserCreateResult>(params);
  }
}
