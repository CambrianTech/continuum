/**
 * State Create Browser Command - Elegant entity creation with user context
 *
 * Builds on data/create command with automatic user context injection and type safety
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { StateCreateParams, StateCreateResult } from '../shared/StateCreateTypes';
import { createStateCreateResult } from '../shared/StateCreateTypes';
import { Commands } from '../../../../system/core/shared/Commands';
import type { DataCreateResult } from '../../../data/create/shared/DataCreateTypes';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';

export class StateCreateBrowserCommand extends CommandBase<StateCreateParams, StateCreateResult<BaseEntity>> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('state/create', context, subpath, commander);
  }

  async execute(params: StateCreateParams): Promise<StateCreateResult<BaseEntity>> {
    try {
      console.log(`🔧 StateCreate: Creating ${params.collection} entity with user context`);

      // Auto-inject user context into entity data if provided
      let enhancedData = params.data;
      if (params.userId) {
        enhancedData = { ...params.data, userId: params.userId };
      }

      // Delegate to the elegant data/create command
      const dataResult = await Commands.execute<any, DataCreateResult<BaseEntity>>('data/create', {
        collection: params.collection,
        data: enhancedData,
        id: params.id
      });

      if (dataResult.success) {
        console.log(`✅ StateCreate: Created ${params.collection} entity`);

        return createStateCreateResult(params, {
          success: true,
          item: dataResult.data,
          id: dataResult.data?.id
        });
      } else {
        console.error(`❌ StateCreate: Failed to create ${params.collection}:`, dataResult.error);

        return createStateCreateResult(params, {
          success: false,
          error: dataResult.error
        });
      }

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ StateCreate: Error creating ${params.collection}:`, error);

      return createStateCreateResult(params, {
        success: false,
        error: errorMessage
      });
    }
  }
}