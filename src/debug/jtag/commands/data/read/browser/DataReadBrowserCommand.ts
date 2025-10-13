/**
 * Data Read Command - Browser Implementation
 *
 * Handles localStorage reads when backend=browser, delegates to server otherwise
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { DataReadCommand } from '../shared/DataReadCommand';
import { LocalStorageDataBackend } from '../../../../daemons/data-daemon/browser/LocalStorageDataBackend';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import type { DataReadParams, DataReadResult } from '../shared/DataReadTypes';
import { createDataReadResultFromParams } from '../shared/DataReadTypes';

export class DataReadBrowserCommand extends DataReadCommand<BaseEntity> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-read', context, subpath, commander);
  }

  protected async executeDataCommand(params: DataReadParams): Promise<DataReadResult<BaseEntity>> {
    // Reduce log spam - only log errors
    // console.log(`🗄️ BROWSER: Reading ${params.collection}/${params.id} from localStorage`);

    try {
      const result = await LocalStorageDataBackend.read(params.collection, params.id);

      if (result.success && result.entity) {
        return createDataReadResultFromParams(params, {
          success: true,
          data: result.entity,
          found: true
        });
      } else {
        return createDataReadResultFromParams(params, {
          success: true,
          data: undefined,
          found: false
        });
      }
    } catch (error) {
      console.error(`❌ BROWSER: Failed to read ${params.collection}/${params.id}:`, error);

      return createDataReadResultFromParams(params, {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error',
        found: false
      });
    }
  }
}