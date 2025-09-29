/**
 * Data Update Command - Browser Implementation
 *
 * Handles localStorage updates when backend=browser, delegates to server otherwise
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataUpdateParams, DataUpdateResult } from '../shared/DataUpdateTypes';
import { createDataUpdateResultFromParams } from '../shared/DataUpdateTypes';
import { DataUpdateCommand } from '../shared/DataUpdateCommand';
import { LocalStorageDataBackend } from '../../../../daemons/data-daemon/browser/LocalStorageDataBackend';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';

export class DataUpdateBrowserCommand extends DataUpdateCommand<BaseEntity> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-update', context, subpath, commander);
  }

  protected async executeDataCommand(params: DataUpdateParams): Promise<DataUpdateResult> {
    console.log(`🔄 BROWSER: Updating ${params.collection}/${params.id} in localStorage`);

    try {
      const result = await LocalStorageDataBackend.update(params.collection, params.id, params.data);

      if (result.success) {
        // Read back the updated entity
        const readResult = await LocalStorageDataBackend.read(params.collection, params.id);
        return createDataUpdateResultFromParams(params, {
          success: true,
          data: readResult.entity,
          found: true,
          id: params.id
        });
      } else {
        return createDataUpdateResultFromParams(params, {
          success: false,
          found: false,
          id: params.id,
          error: result.error ?? 'Update failed'
        });
      }
    } catch (error) {
      console.error(`❌ BROWSER: Failed to update ${params.collection}/${params.id}:`, error);

      return createDataUpdateResultFromParams(params, {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error',
        found: false,
        id: params.id
      });
    }
  }
}