/**
 * Social Engage Command - Shared base class
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { SocialEngageParams, SocialEngageResult } from './SocialEngageTypes';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';

export abstract class SocialEngageBaseCommand extends CommandBase<SocialEngageParams, SocialEngageResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('social/engage', context, subpath, commander);
  }

  protected abstract executeSocialEngage(params: SocialEngageParams): Promise<SocialEngageResult>;

  async execute(params: JTAGPayload): Promise<SocialEngageResult> {
    return this.executeSocialEngage(params as SocialEngageParams);
  }
}
