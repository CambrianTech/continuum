/**
 * FileSave Types - Generic Inheritance from File Base Types
 * 
 * GENERIC HIERARCHY:
 * FileParams<{content, createDirs}> → FileSaveParams
 * FileResult<{bytesWritten, created}> → FileSaveResult
 */

import { FileParams, FileResult } from '../../shared/FileTypes';

// Define the extension types for FileSave
type FileSaveExtension = {
  content: string;
  createDirs?: boolean;
};

type FileSaveResultExtension = {
  bytesWritten: number;
  created: boolean;
};

export class FileSaveParams extends FileParams<FileSaveExtension> {
  content: string;
  createDirs?: boolean;

  constructor(data: Partial<FileSaveParams> = {}) {
    super(data); // Parent handles filepath, encoding with type safety
    Object.assign(this, {
      content: '',
      createDirs: true,
      ...data
    });
  }
}

export class FileSaveResult extends FileResult<FileSaveResultExtension> {
  bytesWritten: number;
  created: boolean;

  constructor(data: Partial<FileSaveResult>) {
    super(data); // Parent handles base fields with type safety
    Object.assign(this, {
      bytesWritten: 0,
      created: false,
      ...data
    });
  }
}