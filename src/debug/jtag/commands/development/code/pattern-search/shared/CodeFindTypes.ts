/**
 * code/find command types - Find files by name pattern
 */

import type { JTAGContext, JTAGEnvironment } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Base params for code commands
 */
export interface BaseCodeParams {
  readonly context: JTAGContext;
  readonly sessionId: UUID;
  readonly backend: JTAGEnvironment;
}

/**
 * Parameters for code/find command
 */
export interface CodeFindParams extends BaseCodeParams {
  /** Filename pattern to search for (supports wildcards: *, ?, []) */
  readonly pattern: string;

  /** Base directory to search (relative to repository root, default: entire repo) */
  readonly baseDir?: string;

  /** Case-insensitive search */
  readonly caseInsensitive?: boolean;

  /** Maximum results to return (default: 50) */
  readonly maxResults?: number;

  /** Include hidden files/directories (default: false) */
  readonly includeHidden?: boolean;

  /** Directories to exclude from search (default: ['node_modules', 'dist', '.continuum', '.git', 'examples/dist', 'coverage']) */
  readonly excludeDirs?: string[];
}

/**
 * Single file match
 */
export interface FileMatch {
  /** Relative path from repository root */
  path: string;

  /** File size in bytes */
  size: number;

  /** Last modified timestamp */
  modified: string;

  /** File type (file, directory, symlink) */
  type: 'file' | 'directory' | 'symlink';
}

/**
 * Result of code/find command
 */
export interface CodeFindResult {
  readonly context: JTAGContext;
  readonly sessionId: UUID;
  readonly backend: JTAGEnvironment;
  readonly timestamp: string;

  /** Operation success */
  success: boolean;

  /** Search pattern used */
  pattern: string;

  /** Files found */
  matches: FileMatch[];

  /** Total matches found (may be > matches.length if limited by maxResults) */
  totalMatches: number;

  /** Base directory searched */
  baseDir: string;

  /** Error message (if !success) */
  error?: string;

  /** Optional message with guidance or additional context */
  message?: string;
}

/**
 * Create code/find params
 */
export const createCodeFindParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<CodeFindParams, 'context' | 'sessionId' | 'backend'> & { backend?: JTAGEnvironment }
): CodeFindParams => {
  return {
    context,
    sessionId,
    backend: data.backend || 'server',
    pattern: data.pattern,
    baseDir: data.baseDir,
    caseInsensitive: data.caseInsensitive,
    maxResults: data.maxResults,
    includeHidden: data.includeHidden,
    excludeDirs: data.excludeDirs
  };
};

/**
 * Factory function to create result
 */
export const createCodeFindResultFromParams = (
  params: CodeFindParams,
  differences: Omit<Partial<CodeFindResult>, 'context' | 'sessionId' | 'backend'>
): CodeFindResult => transformPayload(params, {
  backend: params.backend,
  success: false,
  pattern: params.pattern,
  matches: [],
  totalMatches: 0,
  baseDir: params.baseDir || '.',
  timestamp: new Date().toISOString(),
  ...differences
});
