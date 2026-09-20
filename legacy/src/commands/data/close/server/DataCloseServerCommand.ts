/**
 * Data Close Command - Server Implementation
 */

import { DatabaseHandleRegistry } from '../../../../daemons/data-daemon/server/DatabaseHandleRegistry';
import type { DataCloseParams, DataCloseResult } from '../shared/DataCloseTypes';
import { createDataCloseResultFromParams } from '../shared/DataCloseTypes';

export class DataCloseServerCommand {
  private registry: DatabaseHandleRegistry;

  constructor() {
    this.registry = DatabaseHandleRegistry.getInstance();
  }

  async execute(params: DataCloseParams): Promise<DataCloseResult> {
    try {
      if (!params.dbHandle) {
        return createDataCloseResultFromParams(params, {
          success: false,
          error: 'Missing required parameter: dbHandle'
        });
      }

      await this.registry.close(params.dbHandle);

      return createDataCloseResultFromParams(params, {
        success: true
      });
    } catch (error) {
      return createDataCloseResultFromParams(params, {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }
}
