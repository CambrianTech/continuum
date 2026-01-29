/**
 * Activity Get Server Command
 */

import { CommandBase, type ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../../system/core/types/JTAGTypes';
import type { ActivityGetParams, ActivityGetResult } from '../shared/ActivityGetTypes';
import { Commands } from '@system/core/shared/Commands';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { DataReadParams, DataReadResult } from '@commands/data/read/shared/DataReadTypes';
import type { DataListParams, DataListResult } from '@commands/data/list/shared/DataListTypes';
import type { BaseEntity } from '@system/data/entities/BaseEntity';
import type { ActivityEntity } from '@system/data/entities/ActivityEntity';

export class ActivityGetServerCommand extends CommandBase<ActivityGetParams, ActivityGetResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('collaboration/activity/get', context, subpath, commander);
  }

  async execute(payload: JTAGPayload): Promise<ActivityGetResult> {
    const params = payload as ActivityGetParams;
    const { id, uniqueId } = params;

    if (!id && !uniqueId) {
      return transformPayload(params, {
        success: false,
        error: 'Either id or uniqueId is required'
      });
    }

    let activity: ActivityEntity | undefined;

    if (id) {
      // Get by primary key
      const result = await Commands.execute<DataReadParams, DataReadResult>(DATA_COMMANDS.READ, {
        collection: 'activities',
        id,
        context: params.context,
        sessionId: params.sessionId
      });

      if (result.success && result.data) {
        activity = result.data as ActivityEntity;
      }
    } else if (uniqueId) {
      // Get by uniqueId
      const result = await Commands.execute<DataListParams, DataListResult<BaseEntity>>(DATA_COMMANDS.LIST, {
        collection: 'activities',
        filter: { uniqueId },
        limit: 1,
        context: params.context,
        sessionId: params.sessionId
      });

      if (result.success && result.items?.length) {
        activity = result.items[0] as ActivityEntity;
      }
    }

    if (!activity) {
      return transformPayload(params, {
        success: false,
        error: `Activity not found: ${id || uniqueId}`
      });
    }

    return transformPayload(params, {
      success: true,
      activity
    });
  }
}
