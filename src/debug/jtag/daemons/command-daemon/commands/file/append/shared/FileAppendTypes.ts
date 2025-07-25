/**
 * FileAppend Types - Elegant Inheritance from File Base Types
 * 
 * INHERITANCE HIERARCHY:
 * FileParams → FileAppendParams (adds content, createIfMissing)
 * FileResult → FileAppendResult (adds bytesAppended, wasCreated)
 */

import { FileParams, FileResult } from '../../shared/FileTypes';

export class FileAppendParams extends FileParams {
  content!: string;
  createIfMissing?: boolean;

  constructor(data: Partial<FileAppendParams> = {}) {
    super(data); // Parent handles filepath, encoding
    Object.assign(this, {
      content: '',
      createIfMissing: true,
      ...data
    });
  }
}

export class FileAppendResult extends FileResult {
  bytesAppended!: number;
  wasCreated!: boolean;

  constructor(data: Partial<FileAppendResult>) {
    super(data); // Parent handles success, filepath, exists, error, environment, timestamp
    Object.assign(this, {
      bytesAppended: 0,
      wasCreated: false,
      ...data
    });
  }
}