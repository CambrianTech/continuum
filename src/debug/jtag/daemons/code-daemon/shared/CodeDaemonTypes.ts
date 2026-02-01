/**
 * CodeDaemon Types - Shared type definitions
 *
 * Following DataDaemon pattern with static methods and auto-context injection
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

/**
 * Configuration for CodeDaemon initialization
 */
export interface CodeDaemonConfig {
  /** Root directory of repository */
  repositoryRoot: string;

  /** Maximum file size to read (bytes) */
  maxFileSize?: number;

  /** Enable file content caching */
  enableCache?: boolean;

  /** Cache TTL in milliseconds */
  cacheTTL?: number;

  /** Rate limiting - max operations per minute */
  rateLimit?: number;

  /** Enable audit logging */
  enableAudit?: boolean;
}

/**
 * Context automatically injected into all CodeDaemon operations
 */
export interface CodeOperationContext {
  /** Session ID of requesting user */
  sessionId: UUID;

  /** Timestamp of operation */
  timestamp: string;

  /** Source of operation (command name, daemon, etc) */
  source: string;

  /** Repository root for path validation */
  repositoryRoot: string;
}

/**
 * Options for reading files
 */
export interface CodeReadOptions {
  /** Start line (1-indexed) */
  startLine?: number;

  /** End line (1-indexed) */
  endLine?: number;

  /** Include file metadata */
  includeMetadata?: boolean;

  /** Force bypass cache */
  forceRefresh?: boolean;
}

/**
 * Result of file read operation
 */
export interface CodeReadResult {
  /** Operation success */
  success: boolean;

  /** File content (if success) */
  content?: string;

  /** File metadata */
  metadata: {
    /** Absolute file path */
    path: string;

    /** File size in bytes */
    size: number;

    /** Total line count */
    lines: number;

    /** Lines returned (may differ if range specified) */
    linesReturned: number;

    /** Last modified timestamp */
    modified: string;
  };

  /** Was result served from cache */
  cached?: boolean;

  /** Error message (if !success) */
  error?: string;
}

/**
 * Options for searching code
 */
export interface CodeSearchOptions {
  /** File pattern (glob) to search */
  filePattern?: string;

  /** Case-insensitive search */
  caseInsensitive?: boolean;

  /** Maximum results to return */
  maxResults?: number;

  /** Include context lines around match */
  contextLines?: number;
}

/**
 * Single search match
 */
export interface CodeSearchMatch {
  /** File containing match */
  file: string;

  /** Line number (1-indexed) */
  line: number;

  /** Matched content */
  content: string;

  /** Context before match */
  contextBefore?: string[];

  /** Context after match */
  contextAfter?: string[];
}

/**
 * Result of code search operation
 */
export interface CodeSearchResult {
  /** Operation success */
  success: boolean;

  /** Search pattern used */
  pattern: string;

  /** Matches found */
  matches: CodeSearchMatch[];

  /** Total matches found */
  totalMatches: number;

  /** Total files searched */
  filesSearched: number;

  /** Error message (if !success) */
  error?: string;
}

/**
 * Git operations types
 */
export interface GitLogOptions {
  /** Maximum commits to return */
  maxCount?: number;

  /** Only commits affecting this file */
  file?: string;

  /** Include patch diff */
  includeDiff?: boolean;
}

export interface GitCommit {
  /** Commit hash */
  hash: string;

  /** Author name */
  author: string;

  /** Author email */
  email: string;

  /** Commit timestamp */
  date: string;

  /** Commit message */
  message: string;

  /** Diff (if requested) */
  diff?: string;
}

export interface GitLogResult {
  success: boolean;
  commits: GitCommit[];
  error?: string;
}

/**
 * Event payloads emitted by CodeDaemon
 */
export interface CodeFileReadEvent {
  path: string;
  size: number;
  cached: boolean;
  timestamp: number;
}

export interface CodeSearchEvent {
  pattern: string;
  matchCount: number;
  filesSearched: number;
  timestamp: number;
}

export interface CodeGitLogEvent {
  file?: string;
  commitCount: number;
  timestamp: number;
}

// ============================================================================
// Workspace-Scoped Types — re-exported from ts-rs generated (Rust is source of truth)
// Aliased with Workspace* prefix for domain clarity in CodeDaemon API
// ============================================================================

export type { EditMode as WorkspaceEditMode } from '../../../shared/generated/code/EditMode';
export type { WriteResult as WorkspaceWriteResult } from '../../../shared/generated/code/WriteResult';
export type { ReadResult as WorkspaceReadResult } from '../../../shared/generated/code/ReadResult';
export type { SearchResult as WorkspaceSearchResult } from '../../../shared/generated/code/SearchResult';
export type { SearchMatch as WorkspaceSearchMatch } from '../../../shared/generated/code/SearchMatch';
export type { TreeNode as WorkspaceTreeNode } from '../../../shared/generated/code/TreeNode';
export type { TreeResult as WorkspaceTreeResult } from '../../../shared/generated/code/TreeResult';
export type { UndoResult as WorkspaceUndoResult } from '../../../shared/generated/code/UndoResult';
export type { ChangeNode as WorkspaceChangeNode } from '../../../shared/generated/code/ChangeNode';
export type { HistoryResult as WorkspaceHistoryResult } from '../../../shared/generated/code/HistoryResult';
export type { GitStatusInfo as WorkspaceGitStatusInfo } from '../../../shared/generated/code/GitStatusInfo';
