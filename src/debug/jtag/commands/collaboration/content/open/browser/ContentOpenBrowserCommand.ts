/**
 * Content Open Command - Browser Implementation
 *
 * Delegates to server, then emits content:opened locally for browser widgets.
 */

import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { ContentOpenParams, ContentOpenResult, ContentOpenedEvent } from '../shared/ContentOpenTypes';
import { ContentOpenCommand } from '../shared/ContentOpenCommand';
import { Events } from '@system/core/shared/Events';

export class ContentOpenBrowserCommand extends ContentOpenCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('collaboration/content/open', context, subpath, commander);
  }

  protected async executeContentCommand(params: ContentOpenParams): Promise<ContentOpenResult> {
    // Delegate to server
    const result = await this.remoteExecute(params) as ContentOpenResult;

    // If successful, emit content:opened locally for browser widgets to respond
    if (result.success) {
      const event: ContentOpenedEvent & { setAsCurrent?: boolean } = {
        contentItemId: result.contentItemId,
        contentType: params.contentType,
        entityId: params.entityId,
        title: params.title,
        userId: params.userId,
        currentItemId: result.currentItemId,
        setAsCurrent: params.setAsCurrent
      };

      console.log('📋 ContentOpenBrowserCommand: Emitting content:opened locally', event);
      Events.emit('content:opened', event);
    }

    return result;
  }
}
