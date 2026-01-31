/**
 * Social Signup Command - Shared base class
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { SocialSignupParams, SocialSignupResult } from './SocialSignupTypes';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';

export abstract class SocialSignupCommand extends CommandBase<SocialSignupParams, SocialSignupResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('social/signup', context, subpath, commander);
  }

  protected abstract executeSocialSignup(params: SocialSignupParams): Promise<SocialSignupResult>;

  async execute(params: JTAGPayload): Promise<SocialSignupResult> {
    return this.executeSocialSignup(params as SocialSignupParams);
  }
}
