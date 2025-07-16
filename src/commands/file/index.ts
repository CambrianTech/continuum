// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * File Command Module - Middle-out Architecture Export
 * 
 * Unified exports for the file command module following the
 * /shared|client|server pattern
 */

// Shared exports
export { BaseFileCommand, FileSystemOperation, DirectoryOperation } from './shared/BaseFileCommand';
export type { 
  ArtifactType, 
  FileOperationParams, 
  FileOperationResult, 
  DirectoryResolutionParams,
  ResolvedPath,
  SessionPaths,
  SessionStructure,
  FileValidationResult,
  FileOperationMetadata,
  FileCommandContext
} from './shared/FileTypes';
export { FileValidator, FILE_CONSTRAINTS } from './shared/FileValidator';

// Client exports
export { FileClient } from './client/FileClient';
export { FileSaveClient } from './client/FileSaveClient';
export type { FileSaveClientOptions, FileSaveClientResult } from './client/FileSaveClient';

// Server exports
export { FileWriteCommand } from './server/FileWriteCommand';
export { FileReadCommand } from './server/FileReadCommand';
export { FileAppendCommand } from './server/FileAppendCommand';
export { FileSaveCommand } from './server/FileSaveCommand';

// Convenience exports
export { saveFile } from './client/FileSaveClient';