/**
 * FileSave Types - Generic Inheritance from File Base Types
 * 
 * GENERIC HIERARCHY:
 * FileParams<{content, createDirs}> → FileSaveParams
 * FileResult<{bytesWritten, created}> → FileSaveResult
 */

import { type FileParams, type FileResult, createFileParams, createFileResult } from '@commandsFileShared/FileTypes';
import type { JTAGContext } from '@shared/JTAGTypes';
import type { JTAGError } from '@shared/ErrorTypes';
import { UUID } from 'crypto';

export interface FileSaveParams extends FileParams {
  readonly content: string;
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