/**
 * Dataset Create Command Types
 */

import type { CommandParams, CommandResult } from '../../../../../system/core/types/JTAGTypes';
import { createPayload, type JTAGContext } from '../../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../../system/core/types/CrossPlatformUUID';

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
