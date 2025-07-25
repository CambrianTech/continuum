/**
 * FileLoad Types - Elegant Inheritance from File Base Types
 * 
 * INHERITANCE HIERARCHY:
 * FileParams → FileLoadParams (no additional fields - pure inheritance)
 * FileResult → FileLoadResult (adds content, bytesRead)
 */

import { FileParams, FileResult } from '../../shared/FileTypes';

export class FileLoadParams extends FileParams {
  // Pure inheritance - no additional fields needed
  // FileParams already provides filepath, encoding
  
  constructor(data: Partial<FileLoadParams> = {}) {
    super(data); // Parent handles filepath, encoding
  }
}

export class FileLoadResult extends FileResult {
  content: string;
  bytesRead: number;

  constructor(data: Partial<FileLoadResult>) {
    super(data); // Parent handles success, filepath, exists, error, environment, timestamp
    Object.assign(this, {
      content: '',
      bytesRead: 0,
      ...data
    });
  }
}