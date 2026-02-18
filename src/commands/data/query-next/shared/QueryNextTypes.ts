/**
 * Data Query Next Command Types
 *
 * Gets the next page of results from a query handle
 * DataDaemon manages cursor position internally
 */

import type { CommandParams, JTAGPayload, JTAGEnvironment, CommandInput} from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import { Commands } from '../../../../system/core/shared/Commands';

export interface DataQueryNextParams extends CommandParams {
  readonly queryHandle: UUID; // Handle from query-open
  readonly collection: string; // Required by BaseDataParams
  readonly backend: JTAGEnvironment; // Required by BaseDataParams
}

export interface DataQueryNextResult<T extends BaseEntity = BaseEntity> extends JTAGPayload {
  readonly success: boolean;
  readonly items: readonly T[];
  readonly pageNumber: number; // Which page this is (0-indexed)
  readonly hasMore: boolean; // Are there more pages available?
  readonly totalCount: number; // Total matching records
  readonly error?: string;
  readonly timestamp: string; // Required by BaseDataResult
}

/**
 * DataQueryNext — Type-safe command executor
 *
 * Usage:
 *   import { DataQueryNext } from '...shared/DataQueryNextTypes';
 *   const result = await DataQueryNext.execute({ ... });
 */
export const DataQueryNext = {
  execute<T extends BaseEntity = BaseEntity>(params: CommandInput<DataQueryNextParams>): Promise<DataQueryNextResult<T>> {
    return Commands.execute<DataQueryNextParams, DataQueryNextResult<T>>('data/query-next', params as Partial<DataQueryNextParams>);
  },
  commandName: 'data/query-next' as const,
} as const;
