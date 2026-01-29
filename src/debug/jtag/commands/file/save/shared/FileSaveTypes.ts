/**
 * FileSave Types - Generic Inheritance from File Base Types
 * 
 * GENERIC HIERARCHY:
 * FileParams<{content, createDirs}> → FileSaveParams
 * FileResult<{bytesWritten, created}> → FileSaveResult
 */

import { type FileParams, type FileResult, createFileParams, createFileResult } from '../../shared/FileTypes';
import type { JTAGContext, CommandParams, CommandInput} from '../../../../system/core/types/JTAGTypes';
import type { JTAGError } from '../../../../system/core/types/ErrorTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

/** File save command parameters */
export interface FileSaveParams extends CommandParams {
  /** Path to save the file */
  readonly filepath: string;
  /** Content to write to the file */
  readonly content: string | Buffer;
  /** Create parent directories if they don't exist */
  readonly createDirs?: boolean;
  /** File encoding */
  readonly encoding?: string;
}

export const createFileSaveParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    filepath?: string;
    content?: string;
    createDirs?: boolean;
    encoding?: string;
  }
): FileSaveParams => createFileParams(context, sessionId, {
  content: data.content ?? '',
  createDirs: data.createDirs ?? true,
  ...data
});

export interface FileSaveResult extends FileResult {
  readonly bytesWritten: number;
  readonly created: boolean;
}

export const createFileSaveResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    filepath: string;
    bytesWritten?: number;
    created?: boolean;
    exists?: boolean;
    error?: JTAGError;
  }
): FileSaveResult => createFileResult(context, sessionId, {
  bytesWritten: data.bytesWritten ?? 0,
  created: data.created ?? false,
  ...data
});
/**
 * FileSave — Type-safe command executor
 *
 * Usage:
 *   import { FileSave } from '...shared/FileSaveTypes';
 *   const result = await FileSave.execute({ ... });
 */
export const FileSave = {
  execute(params: CommandInput<FileSaveParams>): Promise<FileSaveResult> {
    return Commands.execute<FileSaveParams, FileSaveResult>('file/save', params as Partial<FileSaveParams>);
  },
  commandName: 'file/save' as const,
} as const;
