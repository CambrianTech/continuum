// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * File Command Module - Middle-out Architecture Export
 * 
 * Unified exports for the file command module following the
 * individual command module pattern where each command is its own module
 */

// Shared exports (used by all file command modules)
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
  FileCommandContext,
  FileSaveClientOptions,
  FileSaveClientResult
} from './shared/FileTypes';
export { FileValidator, FILE_CONSTRAINTS } from './shared/FileValidator';

// Base client export (used by all file command modules)
export { FileClient } from './client/FileClient';

// Re-export from individual command modules
export * from './fileSave';
export * from './fileWrite';