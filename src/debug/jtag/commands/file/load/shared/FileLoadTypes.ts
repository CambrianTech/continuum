/**
 * FileLoad Types - Elegant Inheritance from File Base Types
 * 
 * INHERITANCE HIERARCHY:
 * FileParams → FileLoadParams (no additional fields - pure inheritance)
 * FileResult → FileLoadResult (adds content, bytesRead)
 */

import { type FileParams, type FileResult, createFileParams, createFileResult } from '@commandsFileShared/FileTypes';
import type { JTAGContext } from '@shared/JTAGTypes';
import type { JTAGError } from '@shared/ErrorTypes';
import { UUID } from 'crypto';

export interface FileLoadParams extends FileParams {
  // Pure inheritance - no additional fields needed
  // FileParams already provides filepath, encoding
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