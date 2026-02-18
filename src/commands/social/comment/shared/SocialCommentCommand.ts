/**
 * Social Comment Command - Shared base class
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { SocialCommentParams, SocialCommentResult } from './SocialCommentTypes';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';

export abstract class SocialCommentBaseCommand extends CommandBase<SocialCommentParams, SocialCommentResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('social/comment', context, subpath, commander);
  }

  protected abstract executeSocialComment(params: SocialCommentParams): Promise<SocialCommentResult>;

  async execute(params: JTAGPayload): Promise<SocialCommentResult> {
    return this.executeSocialComment(params as SocialCommentParams);
  }
}
