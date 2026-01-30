/**
 * Dataset Create Command Types
 */

import type { CommandParams, CommandResult, CommandInput} from '../../../../../system/core/types/JTAGTypes';
import { createPayload, type JTAGContext } from '../../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../../system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../../system/core/shared/Commands';

export interface DatasetCreateParams extends CommandParams {
  /** Project ID to archive (if omitted, archives all enabled projects) */
  project?: string;

  /** Source ID to filter projects (if omitted, uses all enabled sources) */
  source?: string;

  /** Override default output path */
  outputPath?: string;

  /** Override compression type */
  compression?: 'gzip' | 'bzip2' | 'xz' | 'none';

  /** Include manifest.json in archive */
  includeManifest?: boolean;
}

export interface DatasetCreateResult extends CommandResult {
  /** Whether operation succeeded */
  success: boolean;

  /** Human-readable message */
  message: string;

  /** Archives that were created */
  archives: Array<{
    projectId: string;
    projectName: string;
    filename: string;
    path: string;
    sizeBytes: number;
    compressionType: string;
  }>;

  /** Total size of all archives in bytes */
  totalSizeBytes: number;

  /** Duration in milliseconds */
  durationMs: number;
}

/**
 * Factory for creating DatasetCreateResult with context/sessionId
 */
export const createDatasetCreateResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DatasetCreateResult, 'context' | 'sessionId'>
): DatasetCreateResult => createPayload(context, sessionId, data);

/**
 * DatasetCreate — Type-safe command executor
 *
 * Usage:
 *   import { DatasetCreate } from '...shared/DatasetCreateTypes';
 *   const result = await DatasetCreate.execute({ ... });
 */
export const DatasetCreate = {
  execute(params: CommandInput<DatasetCreateParams>): Promise<DatasetCreateResult> {
    return Commands.execute<DatasetCreateParams, DatasetCreateResult>('ai/dataset/create', params as Partial<DatasetCreateParams>);
  },
  commandName: 'ai/dataset/create' as const,
} as const;
