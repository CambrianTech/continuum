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
    console.log('🔧 SERVER: Executing state/content/switch', params);

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

      // 4. Emit content:switched event for widgets to respond to
      await Events.emit('content:switched', {
        contentItemId: params.contentItemId,
        userId: params.userId,
        currentItemId: userState.contentState.currentItemId
      });

      // 5. Return success result
      return transformPayload(params, {
        success: true,
        currentItemId: userState.contentState.currentItemId!
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
