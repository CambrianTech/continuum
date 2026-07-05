/**
 * Content Open Command - Browser Implementation
 *
 * Delegates to server for content opening. Server handles event emission.
 */

import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { ContentOpenParams, ContentOpenResult } from '../shared/ContentOpenTypes';
import { ContentOpenCommand } from '../shared/ContentOpenCommand';

export class ContentOpenBrowserCommand extends ContentOpenCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('collaboration/content/open', context, subpath, commander);
  }

  protected async executeContentCommand(params: ContentOpenParams): Promise<ContentOpenResult> {
    // Delegate to server - server handles event emission
    // DO NOT emit content:opened locally here - the server already emits it
    // and the event routes back to us via WebSocket. Emitting here causes
    // duplicate event handling which triggers expensive re-renders.
    return await this.remoteExecute(params) as ContentOpenResult;
  }
}
