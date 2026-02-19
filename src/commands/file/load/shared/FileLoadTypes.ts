/**
 * FileLoad Types - Elegant Inheritance from File Base Types
 * 
 * INHERITANCE HIERARCHY:
 * FileParams → FileLoadParams (no additional fields - pure inheritance)
 * FileResult → FileLoadResult (adds content, bytesRead)
 */

import { type FileParams, type FileResult, createFileParams, createFileResult } from '../../shared/FileTypes';
import type { JTAGContext, CommandParams, CommandInput} from '../../../../system/core/types/JTAGTypes';
import type { JTAGError } from '../../../../system/core/types/ErrorTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

/** File load command parameters */
export interface FileLoadParams extends CommandParams {
  /** Path to the file to load */
  readonly filepath: string;
  /** File encoding (default: utf8) */
  readonly encoding?: string;
}

export const createFileLoadParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    filepath?: string;
    encoding?: string;
  }
): FileLoadParams => createFileParams(context, sessionId, data);

export interface FileLoadResult extends FileResult {
  readonly content: string;
  readonly bytesRead: number;
}

export const createFileLoadResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    filepath: string;
    content?: string;
    bytesRead?: number;
    exists?: boolean;
    error?: JTAGError;
  }
): FileLoadResult => createFileResult(context, sessionId, {
  content: data.content ?? '',
  bytesRead: data.bytesRead ?? 0,
  ...data
});
/**
 * FileLoad — Type-safe command executor
 *
 * Usage:
 *   import { FileLoad } from '...shared/FileLoadTypes';
 *   const result = await FileLoad.execute({ ... });
 */
export const FileLoad = {
  execute(params: CommandInput<FileLoadParams>): Promise<FileLoadResult> {
    return Commands.execute<FileLoadParams, FileLoadResult>('file/load', params as Partial<FileLoadParams>);
  },
  commandName: 'file/load' as const,
} as const;
