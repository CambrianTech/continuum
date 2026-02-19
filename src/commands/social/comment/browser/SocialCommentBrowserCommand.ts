/**
 * Social Comment Command - Browser Implementation
 * Delegates to server
 */

import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { SocialCommentBaseCommand } from '../shared/SocialCommentCommand';
import type { SocialCommentParams, SocialCommentResult } from '../shared/SocialCommentTypes';

export class SocialCommentBrowserCommand extends SocialCommentBaseCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeSocialComment(params: SocialCommentParams): Promise<SocialCommentResult> {
    return await this.remoteExecute(params);
  }
}
