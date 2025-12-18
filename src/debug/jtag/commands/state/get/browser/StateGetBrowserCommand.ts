/**
 * State Get Browser Command - Elegant entity retrieval with user context
 *
 * Builds on data/list command with automatic user context injection and type safety
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { StateGetParams, StateGetResult } from '../shared/StateGetTypes';
import { createStateGetResult } from '../shared/StateGetTypes';
import { Commands } from '../../../../system/core/shared/Commands';
import type { DataListResult } from '../../../data/list/shared/DataListTypes';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';

export class StateGetBrowserCommand extends CommandBase<StateGetParams, StateGetResult<BaseEntity>> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('state-get', context, subpath, commander);
  }

  async execute(params: StateGetParams): Promise<StateGetResult<BaseEntity>> {
    try {
      console.log(`🔧 StateGet: Getting ${params.collection} entities with user context`);

      // Auto-inject user context if not provided
      let filter = params.filter || {};
      if (params.userId) {
        filter = { ...filter, userId: params.userId };
      }

      // Delegate to the elegant data/list command
      const dataResult = await Commands.execute<any, DataListResult<BaseEntity>>(DATA_COMMANDS.LIST, {
        collection: params.collection,
        filter,
        limit: params.limit,
        orderBy: params.orderBy
      });

      if (dataResult.success) {
        console.log(`✅ StateGet: Retrieved ${dataResult.items.length} ${params.collection} entities`);

        return createStateGetResult(params, {
          success: true,
          items: dataResult.items,
          count: dataResult.items.length
        });
      } else {
        console.error(`❌ StateGet: Failed to get ${params.collection}:`, dataResult.error);

        return createStateGetResult(params, {
          success: false,
          error: dataResult.error
        });
      }

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ StateGet: Error getting ${params.collection}:`, error);

      return createStateGetResult(params, {
        success: false,
        error: errorMessage
      });
    }
  }
}