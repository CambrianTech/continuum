/**
 * Data Executor Adapter - Elegant Bridge to BaseWidget executeCommand
 *
 * Clean adapter that bridges our DataExecutor interface with BaseWidget's
 * elegant executeCommand pattern, preserving full type safety.
 */

import type { BaseEntity } from '../../system/data/entities/BaseEntity';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { DataListParams, DataListResult } from '../../commands/data/list/shared/DataListTypes';
import type { DataExecutor, DataQueryParams, DataQueryResult } from './DataLoaders';
import type { CommandParams, CommandResult } from '../../system/core/types/JTAGTypes';

export function createDataExecutor<T extends BaseEntity>(
  executeCommand: <P extends CommandParams, R extends CommandResult>(command: string, params?: P) => Promise<R>,
  collection: string
): DataExecutor<T> {
  return {
    async execute(params: DataQueryParams): Promise<DataQueryResult<T>> {
      // Elegant translation - BaseWidget executeCommand will add context/sessionId
      const dataListParams = {
        collection,
        filter: params.filter ?? {},
        orderBy: params.orderBy ? [...params.orderBy] : [],
        limit: params.limit ?? 50,
        ...(params.cursor && { cursor: params.cursor })
      } satisfies Omit<DataListParams, 'context' | 'sessionId' | 'userId'>;

      const result = await executeCommand<DataListParams, DataListResult<T>>(
        DATA_COMMANDS.LIST,
        dataListParams as unknown as DataListParams
      );

      return {
        success: result.success,
        items: result.items ?? [],
        totalCount: result.count ?? 0
      };
    }
  };
}