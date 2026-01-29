/**
 * State Update Browser Command
 *
 * Browser-side implementation that injects user context and delegates to data/update
 * Provides the same elegant delegation pattern as state/create
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { StateUpdateParams, StateUpdateResult } from '../shared/StateUpdateTypes';
import { createStateUpdateResult } from '../shared/StateUpdateTypes';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import { Commands } from '../../../../system/core/shared/Commands';
import type { DataUpdateParams, DataUpdateResult } from '../../../data/update/shared/DataUpdateTypes';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';

export class StateUpdateBrowserCommand extends CommandBase<StateUpdateParams, StateUpdateResult<BaseEntity>> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('state-update', context, subpath, commander);
  }

  async execute(params: StateUpdateParams): Promise<StateUpdateResult<BaseEntity>> {
    console.log(`🔧 StateUpdateBrowser: Processing state update with user context injection`);

    try {
      // Auto-inject user context into entity data if provided
      let enhancedData = params.data;
      if (params.userId) {
        enhancedData = { ...params.data, userId: params.userId };
      }

      // Delegate to the elegant data/update command
      const dataResult = await Commands.execute<DataUpdateParams, DataUpdateResult>(DATA_COMMANDS.UPDATE, {
        collection: params.collection,
        id: params.id,
        data: enhancedData
      });

      if (dataResult.found && dataResult.data) {
        return createStateUpdateResult(params.context, params.sessionId, {
          success: true,
          item: dataResult.data as BaseEntity,
          id: dataResult.id,
          collection: params.collection,
          version: dataResult.newVersion
        });
      } else {
        return createStateUpdateResult(params.context, params.sessionId, {
          success: false,
          collection: params.collection,
          error: dataResult.error || 'Update failed'
        });
      }
    } catch (error) {
      console.error('❌ StateUpdateBrowser: State update failed:', error);
      return createStateUpdateResult(params.context, params.sessionId, {
        success: false,
        collection: params.collection,
        error: error instanceof Error ? error.message : 'Unknown error'
      });
    }
  }
}