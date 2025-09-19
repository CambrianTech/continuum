/**
 * Data Executor Adapter - Simple Bridge to JTAG Commands
 */

import type { BaseEntity } from '../../system/data/domains/CoreTypes';
import type { DataListResult } from '../../commands/data/list/shared/DataListTypes';
import type { DataExecutor, DataQueryParams, DataQueryResult } from './DataLoaders';

export function createDataExecutor<T extends BaseEntity>(
  executeCommand: any
): DataExecutor<T> {
  return {
    async execute(params: DataQueryParams): Promise<DataQueryResult<T>> {
      const result = await executeCommand('data/list', {
        collection: params.collection,
        filter: params.filter || {},
        orderBy: params.orderBy || [],
        limit: params.limit || 50,
        ...(params.cursor && { cursor: params.cursor })
      });

      return {
        success: result.success,
        items: result.items || [],
        totalCount: result.count || 0
      };
    }
  };
}