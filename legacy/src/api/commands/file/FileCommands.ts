/**
 * File Commands API - Public Interface Types
 * 
 * Consumer-first API for file operations.
 * These are the types external consumers import.
 */

// Base file operation parameters
export interface FileOperationParams {
  filepath: string;
  encoding?: 'utf8' | 'binary' | 'base64';
}

// File load operations
export interface FileLoadParams extends FileOperationParams {
  // Pure inheritance from base file params
}

export interface FileLoadResult {
  success: boolean;
  filepath: string;
  content: string;
  bytesRead: number;
  exists: boolean;
  error?: string;
}

// File save operations  
export interface FileSaveParams extends FileOperationParams {
  content: string;
  createDirectories?: boolean;
  backup?: boolean;
}

export interface FileSaveResult {
  success: boolean;
  filepath: string;
  bytesWritten: number;
  created: boolean;
  error?: string;
}

// File append operations
export interface FileAppendParams extends FileOperationParams {
  content: string;
  createIfNotExists?: boolean;
}

export interface FileAppendResult {
  success: boolean;
  filepath: string;
  bytesAppended: number;
  totalSize: number;
  error?: string;
}

// File system operations
export interface FileExistsParams {
  filepath: string;
}

export interface FileExistsResult {
  success: boolean;
  exists: boolean;
  filepath: string;
  isDirectory: boolean;
  isFile: boolean;
  error?: string;
}

export interface FileDeleteParams {
  filepath: string;
  recursive?: boolean;
}

export interface FileDeleteResult {
  success: boolean;
  filepath: string;
  deleted: boolean;
  error?: string;
}

// Directory operations
export interface DirectoryListParams {
  path: string;
  recursive?: boolean;
  includeHidden?: boolean;
  filter?: string; // glob pattern
}

export interface DirectoryEntry {
  name: string;
  path: string;
  isDirectory: boolean;
  isFile: boolean;
  size?: number;
  modifiedAt?: string;
}

export interface DirectoryListResult {
  success: boolean;
  path: string;
  entries: DirectoryEntry[];
  totalCount: number;
  error?: string;
}

// Export all file command types
export type FileCommandParams = 
  | FileLoadParams 
  | FileSaveParams 
  | FileAppendParams 
  | FileExistsParams 
  | FileDeleteParams 
  | DirectoryListParams;

export type FileCommandResult = 
  | FileLoadResult 
  | FileSaveResult 
  | FileAppendResult 
  | FileExistsResult 
  | FileDeleteResult 
  | DirectoryListResult;