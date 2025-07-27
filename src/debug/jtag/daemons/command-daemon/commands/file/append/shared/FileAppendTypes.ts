/**
 * FileAppend Types - Elegant Inheritance from File Base Types
 * 
 * INHERITANCE HIERARCHY:
 * FileParams → FileAppendParams (adds content, createIfMissing)
 * FileResult → FileAppendResult (adds bytesAppended, wasCreated)
 */

import { type FileParams, type FileResult, createFileParams, createFileResult } from '@fileShared/FileTypes';
import type { JTAGContext } from '@shared/JTAGTypes';
import { UUID } from 'crypto';

export interface FileAppendParams extends FileParams {
  readonly content: string;
  readonly createIfMissing?: boolean;
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
    error?: string;
  }
): FileAppendResult => createFileResult(context, sessionId, {
  bytesAppended: data.bytesAppended ?? 0,
  wasCreated: data.wasCreated ?? false,
  ...data
});