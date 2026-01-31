/**
 * Social Signup Command - Browser Implementation
 * Delegates to server
 */

import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { SocialSignupCommand } from '../shared/SocialSignupCommand';
import type { SocialSignupParams, SocialSignupResult } from '../shared/SocialSignupTypes';

export class SocialSignupBrowserCommand extends SocialSignupCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeSocialSignup(params: SocialSignupParams): Promise<SocialSignupResult> {
    return await this.remoteExecute(params);
  }
}
