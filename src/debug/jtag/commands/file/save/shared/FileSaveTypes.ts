/**
 * FileSave Types - Generic Inheritance from File Base Types
 * 
 * GENERIC HIERARCHY:
 * FileParams<{content, createDirs}> → FileSaveParams
 * FileResult<{bytesWritten, created}> → FileSaveResult
 */

import { type FileParams, type FileResult, createFileParams, createFileResult } from '../../shared/FileTypes';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { JTAGError } from '../../../../system/core/types/ErrorTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

export interface FileSaveParams extends FileParams {
  readonly content: string | Buffer;
  readonly createDirs?: boolean;
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