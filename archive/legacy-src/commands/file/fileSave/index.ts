// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * FileSave Module - Middle-out Architecture Export
 * 
 * Self-contained module for file saving functionality following the
 * shared/client/server pattern
 */

// Types re-exported from module level
export type { 
  FileSaveClientOptions, 
  FileSaveClientResult 
} from '../shared/FileTypes';

// Client exports
export { FileSaveClient, saveFile } from './client/FileSaveClient';

// Server exports
export { FileSaveCommand } from './FileSaveCommand';