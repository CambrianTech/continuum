/**
 * Data Read Command - Server Implementation
 *
 * Updated to use DataDaemon for consistent storage access
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataReadParams, DataReadResult } from '../shared/DataReadTypes';
import { createDataReadResultFromParams } from '../shared/DataReadTypes';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import { DataReadCommand } from '../shared/DataReadCommand';

export class DataReadServerCommand extends DataReadCommand<BaseEntity> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-read', context, subpath, commander);
  }

  protected async executeDataCommand(params: DataReadParams): Promise<DataReadResult> {
    console.log(`🗄️ DATA SERVER: Reading ${params.collection}/${params.id} via DataDaemon`);

    try {
      // Use DataDaemon for consistent storage access
      const result = await DataDaemon.read<BaseEntity>(params.collection, params.id);

      if (result.success && result.data) {
        console.log(`✅ DATA SERVER: Read ${params.collection}/${params.id}`);

        return createDataReadResultFromParams(params, {
          success: true,
          data: result.data.data, // Extract entity data from DataRecord
          found: true
        });
      } else {
        console.log(`ℹ️ DATA SERVER: Record not found ${params.collection}/${params.id}`);

        return createDataReadResultFromParams(params, {
          success: true,
          data: undefined,
          found: false
        });
      }
    } catch (error) {
      console.error(`❌ DATA SERVER: Failed to read ${params.collection}/${params.id}:`, error);

      return createDataReadResultFromParams(params, {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error',
        found: false
      });
    }
  }
}