// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * File Command Module - Client-Only Exports
 * 
 * Browser-safe exports for the file command module.
 * Only includes client-side functionality, no server commands.
 */

// Shared types only (no server implementations)
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

// Base client export (used by all file command modules)
export { FileClient } from './client/FileClient';

// Client-only exports from individual command modules
export { FileSaveClient, saveFile } from './fileSave/client/FileSaveClient';