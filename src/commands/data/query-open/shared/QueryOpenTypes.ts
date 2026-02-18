/**
 * Data Query Open Command Types
 *
 * Opens a paginated query and returns a handle (UUID)
 * DataDaemon maintains the query state (filters, sorting, cursor position)
 */

import type { CommandParams, JTAGPayload, JTAGEnvironment, CommandInput} from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

export interface DataQueryOpenParams extends CommandParams {
  readonly collection: string;
  readonly backend: JTAGEnvironment;
  readonly filter?: Record<string, any>;
  readonly orderBy?: { field: string; direction: 'asc' | 'desc' }[];
  readonly pageSize?: number;
}

export interface DataQueryOpenResult extends JTAGPayload {
  readonly success: boolean;
  readonly queryHandle: UUID; // Opaque handle for subsequent nextPage calls
  readonly totalCount: number;
  readonly pageSize: number;
  readonly error?: string;
  readonly timestamp: string; // Required by BaseDataResult
}

/**
 * DataQueryOpen — Type-safe command executor
 *
 * Usage:
 *   import { DataQueryOpen } from '...shared/DataQueryOpenTypes';
 *   const result = await DataQueryOpen.execute({ ... });
 */
export const DataQueryOpen = {
  execute(params: CommandInput<DataQueryOpenParams>): Promise<DataQueryOpenResult> {
    return Commands.execute<DataQueryOpenParams, DataQueryOpenResult>('data/query-open', params as Partial<DataQueryOpenParams>);
  },
  commandName: 'data/query-open' as const,
} as const;
