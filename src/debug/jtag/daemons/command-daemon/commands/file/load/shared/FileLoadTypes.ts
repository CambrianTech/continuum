/**
 * FileLoad Types - Elegant Inheritance from File Base Types
 * 
 * INHERITANCE HIERARCHY:
 * FileParams → FileLoadParams (no additional fields - pure inheritance)
 * FileResult → FileLoadResult (adds content, bytesRead)
 */

import { FileParams, FileResult } from '@fileShared/FileTypes';
import type { JTAGContext } from '@shared/JTAGTypes';

export class FileLoadParams extends FileParams {
  // Pure inheritance - no additional fields needed
  // FileParams already provides filepath, encoding
  
  constructor(data: Partial<FileLoadParams> = {}, context: JTAGContext, sessionId: string) {
    super(data, context, sessionId); // Parent handles filepath, encoding
  }
}

export class FileLoadResult extends FileResult {
  content!: string;
  bytesRead!: number;

  constructor(data: Partial<FileLoadResult>, context: JTAGContext, sessionId: string) {
    super(data, context, sessionId); // Parent handles success, filepath, exists, error, timestamp
    Object.assign(this, {
      content: '',
      bytesRead: 0,
      ...data
    });
  }
}