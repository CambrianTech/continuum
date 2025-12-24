/**
 * State Content Close Command - Server Implementation
 *
 * Close a content item (remove from user's open tabs). Handles currentItemId reassignment if closing the active tab.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { StateContentCloseParams, StateContentCloseResult } from '../shared/StateContentCloseTypes';
import { Commands } from '@system/core/shared/Commands';
import { Events } from '@system/core/shared/Events';
import { UserStateEntity } from '@system/data/entities/UserStateEntity';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { DataListParams, DataListResult } from '@commands/data/list/shared/DataListTypes';
import type { DataUpdateParams, DataUpdateResult } from '@commands/data/update/shared/DataUpdateTypes';

export class StateContentCloseServerCommand extends CommandBase<StateContentCloseParams, StateContentCloseResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('state/content/close', context, subpath, commander);
  }

  async execute(params: StateContentCloseParams): Promise<StateContentCloseResult> {
    console.log('🔧 SERVER: Executing state/content/close', params);

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
          openItemsCount: 0,
          error: `No UserState found for userId: ${params.userId}`
        });
      }

      const userState = Object.assign(new UserStateEntity(), listResult.items[0]);

      // 2. Remove content item using UserStateEntity method
      userState.removeContentItem(params.contentItemId);

      // 3. Save updated userState to database
      await Commands.execute<DataUpdateParams, DataUpdateResult>(DATA_COMMANDS.UPDATE, {
        collection: 'user_states',
        id: userState.id,
        data: userState
      });

      // 4. Emit content:closed event for widgets to respond to
      await Events.emit('content:closed', {
        contentItemId: params.contentItemId,
        userId: params.userId,
        currentItemId: userState.contentState.currentItemId
      });

      // 5. Return success result
      return transformPayload(params, {
        success: true,
        openItemsCount: userState.contentState.openItems.length,
        currentItemId: userState.contentState.currentItemId
      });

    } catch (error) {
      console.error('❌ StateContentCloseServerCommand: Error closing content:', error);
      return transformPayload(params, {
        success: false,
        openItemsCount: 0,
        error: error instanceof Error ? error.message : 'Unknown error'
      });
    }
  }
}
