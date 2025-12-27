/**
 * Content Open Command - Server Implementation
 *
 * Opens content and adds it to user's openItems array in UserStateEntity.
 * Emits content:opened event for widgets to respond to.
 */

import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { ContentOpenParams, ContentOpenResult, ContentOpenedEvent } from '../shared/ContentOpenTypes';
import { ContentOpenCommand } from '../shared/ContentOpenCommand';
import { Commands } from '@system/core/shared/Commands';
import { UserStateEntity } from '@system/data/entities/UserStateEntity';
import { Events } from '@system/core/shared/Events';
import { generateUUID, type UUID } from '@system/core/types/CrossPlatformUUID';
import type { DataListParams, DataListResult } from '@commands/data/list/shared/DataListTypes';
import type { DataUpdateParams, DataUpdateResult } from '@commands/data/update/shared/DataUpdateTypes';

export class ContentOpenServerCommand extends ContentOpenCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('collaboration/content/open', context, subpath, commander);
  }

  protected async executeContentCommand(params: ContentOpenParams): Promise<ContentOpenResult> {
    try {
      // userId is REQUIRED in params (should be injected by infrastructure from session)
      const userId = params.userId;

      // 1. Load user's UserStateEntity from database
      const listResult = await Commands.execute<DataListParams<UserStateEntity>, DataListResult<UserStateEntity>>(DATA_COMMANDS.LIST, {
        collection: 'user_states',
        filter: { userId },
        limit: 1
      });

      if (!listResult?.success || !listResult.items || listResult.items.length === 0) {
        return transformPayload(params, {
          success: false,
          contentItemId: '' as UUID,
          openItemsCount: 0,
          error: `No UserState found for userId: ${userId}`
        });
      }

      const userState = Object.assign(new UserStateEntity(), listResult.items[0]);

      // 2. Generate unique ID for the content item
      const contentItemId = generateUUID();

      // 3. Add content item using UserStateEntity method
      userState.addContentItem({
        id: contentItemId,
        type: params.contentType,
        entityId: params.entityId,
        title: params.title,
        subtitle: params.subtitle,
        priority: params.priority || 'normal',
        metadata: params.metadata
      });

      // 4. Optionally set as current item (default: true)
      const setAsCurrent = params.setAsCurrent !== false; // Default to true
      if (setAsCurrent) {
        userState.setCurrentContent(contentItemId);
      }

      // 5. Save updated userState to database
      await Commands.execute<DataUpdateParams, DataUpdateResult>(DATA_COMMANDS.UPDATE, {
        collection: 'user_states',
        id: userState.id,
        data: userState
      });

      // 6. Emit content:opened event for widgets to respond to
      const event: ContentOpenedEvent = {
        contentItemId,
        contentType: params.contentType,
        entityId: params.entityId,
        title: params.title,
        userId,
        currentItemId: userState.contentState.currentItemId,
        setAsCurrent  // Include so browser knows to switch view
      };

      await Events.emit(this.context, 'content:opened', event);

      // 7. Return success result
      return transformPayload(params, {
        success: true,
        contentItemId,
        currentItemId: userState.contentState.currentItemId,
        openItemsCount: userState.contentState.openItems.length
      });

    } catch (error) {
      console.error('❌ ContentOpenServerCommand: Error opening content:', error);
      return transformPayload(params, {
        success: false,
        contentItemId: '' as UUID,
        openItemsCount: 0,
        error: error instanceof Error ? error.message : 'Unknown error'
      });
    }
  }
}
