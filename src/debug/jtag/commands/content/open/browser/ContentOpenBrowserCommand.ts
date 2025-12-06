/**
 * Content Open Command - Browser Implementation
 *
 * Browser stub - delegates all logic to server.
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { ContentOpenParams, ContentOpenResult } from '../shared/ContentOpenTypes';
import { ContentOpenCommand } from '../shared/ContentOpenCommand';

export class ContentOpenBrowserCommand extends ContentOpenCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('content/open', context, subpath, commander);
  }

  protected async executeContentCommand(params: ContentOpenParams): Promise<ContentOpenResult> {
    // Delegate to server
    return await this.remoteExecute(params);
  }
}
