/**
 * Data Query Close Command Types
 *
 * Closes a query handle and frees resources
 * Should be called when done with pagination
 */

import type { CommandParams, JTAGPayload, JTAGEnvironment, CommandInput} from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

export interface DataQueryCloseParams extends CommandParams {
  readonly queryHandle: UUID;
  readonly collection: string; // Required by BaseDataParams
  readonly backend: JTAGEnvironment; // Required by BaseDataParams
}

export interface DataQueryCloseResult extends JTAGPayload {
  readonly success: boolean;
  readonly error?: string;
  readonly timestamp: string; // Required by BaseDataResult
}

/**
 * DataQueryClose — Type-safe command executor
 *
 * Usage:
 *   import { DataQueryClose } from '...shared/DataQueryCloseTypes';
 *   const result = await DataQueryClose.execute({ ... });
 */
export const DataQueryClose = {
  execute(params: CommandInput<DataQueryCloseParams>): Promise<DataQueryCloseResult> {
    return Commands.execute<DataQueryCloseParams, DataQueryCloseResult>('data/query-close', params as Partial<DataQueryCloseParams>);
  },
  commandName: 'data/query-close' as const,
} as const;
