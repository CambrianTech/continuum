/**
 * Social Propose Command - Shared base class
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { SocialProposeParams, SocialProposeResult } from './SocialProposeTypes';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';

export abstract class SocialProposeBaseCommand extends CommandBase<SocialProposeParams, SocialProposeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('social/propose', context, subpath, commander);
  }

  protected abstract executeSocialPropose(params: SocialProposeParams): Promise<SocialProposeResult>;

  async execute(params: JTAGPayload): Promise<SocialProposeResult> {
    return this.executeSocialPropose(params as SocialProposeParams);
  }
}
