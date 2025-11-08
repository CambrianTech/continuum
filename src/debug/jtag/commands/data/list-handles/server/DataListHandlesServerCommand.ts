/**
 * Data List-Handles Command - Server Implementation
 */

import { DatabaseHandleRegistry } from '../../../../daemons/data-daemon/server/DatabaseHandleRegistry';
import type { DataListHandlesParams, DataListHandlesResult } from '../shared/DataListHandlesTypes';
import { createDataListHandlesResultFromParams } from '../shared/DataListHandlesTypes';

export class DataListHandlesServerCommand {
  private registry: DatabaseHandleRegistry;

  constructor() {
    this.registry = DatabaseHandleRegistry.getInstance();
  }

  async execute(params: DataListHandlesParams): Promise<DataListHandlesResult> {
    try {
      const handles = this.registry.listHandles();

      return createDataListHandlesResultFromParams(params, {
        success: true,
        handles
      });
    } catch (error) {
      return createDataListHandlesResultFromParams(params, {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }
}
