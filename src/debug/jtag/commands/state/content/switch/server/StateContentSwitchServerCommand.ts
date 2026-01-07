/**
 * State Content Switch Command - Server Implementation
 *
 * Switch to an existing open content item (set as current/highlighted tab). Does NOT add to openItems - use content/open for that.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { StateContentSwitchParams, StateContentSwitchResult } from '../shared/StateContentSwitchTypes';
import { Commands } from '@system/core/shared/Commands';
import { Events } from '@system/core/shared/Events';
import { UserStateEntity } from '@system/data/entities/UserStateEntity';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { DataListParams, DataListResult } from '@commands/data/list/shared/DataListTypes';
import type { DataUpdateParams, DataUpdateResult } from '@commands/data/update/shared/DataUpdateTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export class StateContentSwitchServerCommand extends CommandBase<StateContentSwitchParams, StateContentSwitchResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('state/content/switch', context, subpath, commander);
  }

  async execute(params: StateContentSwitchParams): Promise<StateContentSwitchResult> {

    // Validate required parameters
    if (!params.userId) {
      throw new ValidationError('userId', 'Missing required parameter userId');
    }
    if (!params.contentItemId) {
      throw new ValidationError('contentItemId', 'Missing required parameter contentItemId');
    }

    try {
      // 1. Load user's UserStateEntity from database
      const listResult = await Commands.execute<DataListParams<UserStateEntity>, DataListResult<UserStateEntity>>(DATA_COMMANDS.LIST, {
        collection: 'user_states',
        filter: { userId: params.userId },
        limit: 1
      });

      if (!listResult?.success || !listResult.items || listResult.items.length === 0) {
        return transformPayload(params, {
          success: false,
          currentItemId: '' as UUID,
          error: `No UserState found for userId: ${params.userId}`
        });
      }

      const userState = Object.assign(new UserStateEntity(), listResult.items[0]);

      // 2. Switch to content item using UserStateEntity method
      const switched = userState.setCurrentContent(params.contentItemId);

      if (!switched) {
        return transformPayload(params, {
          success: false,
          currentItemId: userState.contentState.currentItemId || '' as UUID,
          error: `Content item not found in openItems: ${params.contentItemId}`
        });
      }

      // 3. Save updated userState to database
      await Commands.execute<DataUpdateParams, DataUpdateResult>(DATA_COMMANDS.UPDATE, {
        collection: 'user_states',
        id: userState.id,
        data: userState
      });

      // 4. Get content item details for the event
      const contentItem = userState.contentState.openItems.find(
        item => item.id === params.contentItemId
      );

      // 5. Emit content:switched event with full details for widgets
      await Events.emit(this.context, 'content:switched', {
        contentItemId: params.contentItemId,
        userId: params.userId,
        currentItemId: userState.contentState.currentItemId,
        // Include content item details so widgets don't need to look them up
        contentType: contentItem?.type,
        entityId: contentItem?.entityId,
        title: contentItem?.title
      });

      // 6. Return success result with content item details for browser-side event
      return transformPayload(params, {
        success: true,
        currentItemId: userState.contentState.currentItemId!,
        contentType: contentItem?.type,
        entityId: contentItem?.entityId,
        title: contentItem?.title
      });

    } catch (error) {
      console.error('❌ StateContentSwitchServerCommand: Error switching content:', error);
      return transformPayload(params, {
        success: false,
        currentItemId: '' as UUID,
        error: error instanceof Error ? error.message : 'Unknown error'
      });
    }
  }
}
