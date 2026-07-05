/**
 * Dataset List Command Types
 */

import type { CommandParams, CommandResult, CommandInput} from '../../../../../system/core/types/JTAGTypes';
import { createPayload, type JTAGContext } from '../../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../../system/core/types/CrossPlatformUUID';
import type { DatasetArchiveInfo } from '../../shared/DatasetConfig';
import { Commands } from '../../../../../system/core/shared/Commands';

/**
 * List available training dataset archives with optional filtering and detailed manifest information.
 */
export interface DatasetListParams extends CommandParams {
  /** Filter by output path */
  path?: string;

  /** Show detailed information including manifests */
  detailed?: boolean;
}

export interface DatasetListResult extends CommandResult {
  /** Whether operation succeeded */
  success: boolean;

  /** Human-readable message */
  message: string;

  /** Found archives */
  archives: DatasetArchiveInfo[];

  /** Total size of all archives */
  totalSizeBytes: number;
}

/**
 * Factory for creating DatasetListResult with context/sessionId
 */
export const createDatasetListResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DatasetListResult, 'context' | 'sessionId'>
): DatasetListResult => createPayload(context, sessionId, data);

/**
 * DatasetList — Type-safe command executor
 *
 * Usage:
 *   import { DatasetList } from '...shared/DatasetListTypes';
 *   const result = await DatasetList.execute({ ... });
 */
export const DatasetList = {
  execute(params: CommandInput<DatasetListParams>): Promise<DatasetListResult> {
    return Commands.execute<DatasetListParams, DatasetListResult>('ai/dataset/list', params as Partial<DatasetListParams>);
  },
  commandName: 'ai/dataset/list' as const,
} as const;
