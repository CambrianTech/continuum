/**
 * FileAppend Types - Elegant Inheritance from File Base Types
 * 
 * INHERITANCE HIERARCHY:
 * FileParams → FileAppendParams (adds content, createIfMissing)
 * FileResult → FileAppendResult (adds bytesAppended, wasCreated)
 */

import { type FileParams, type FileResult, createFileParams, createFileResult } from '../../shared/FileTypes';
import type { JTAGContext, CommandParams, CommandInput} from '../../../../system/core/types/JTAGTypes';
import type { JTAGError } from '../../../../system/core/types/ErrorTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

/** File append command parameters */
export interface FileAppendParams extends CommandParams {
  /** Path to the file to append to */
  readonly filepath: string;
  /** Content to append */
  readonly content: string;
  /** Create the file if it doesn't exist */
  readonly createIfMissing?: boolean;
  /** File encoding */
  readonly encoding?: string;
}

export const createFileAppendParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    filepath?: string;
    content?: string;
    createIfMissing?: boolean;
    encoding?: string;
  }
): FileAppendParams => createFileParams(context, sessionId, {
  content: data.content ?? '',
  createIfMissing: data.createIfMissing ?? true,
  ...data
});

export interface FileAppendResult extends FileResult {
  readonly bytesAppended: number;
  readonly wasCreated: boolean;
}

export const createFileAppendResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    filepath: string;
    bytesAppended?: number;
    wasCreated?: boolean;
    exists?: boolean;
    error?: JTAGError;
  }
): FileAppendResult => createFileResult(context, sessionId, {
  bytesAppended: data.bytesAppended ?? 0,
  wasCreated: data.wasCreated ?? false,
  ...data
});
/**
 * FileAppend — Type-safe command executor
 *
 * Usage:
 *   import { FileAppend } from '...shared/FileAppendTypes';
 *   const result = await FileAppend.execute({ ... });
 */
export const FileAppend = {
  execute(params: CommandInput<FileAppendParams>): Promise<FileAppendResult> {
    return Commands.execute<FileAppendParams, FileAppendResult>('file/append', params as Partial<FileAppendParams>);
  },
  commandName: 'file/append' as const,
} as const;
