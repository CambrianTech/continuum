/**
 * Code Tree Command - Shared Types
 *
 * Generate a directory tree for the workspace or a subdirectory. Shows file/directory structure with sizes. Skips common ignored directories (node_modules, .git, etc).
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { TreeNode } from '@shared/generated/code/TreeNode';

/**
 * Code Tree Command Parameters
 */
export interface CodeTreeParams extends CommandParams {
  // Subdirectory to tree (default: workspace root)
  path?: string;
  // Maximum directory depth (default: 10)
  maxDepth?: number;
  // Include hidden files and directories (default: false)
  includeHidden?: boolean;
}

/**
 * Factory function for creating CodeTreeParams
 */
export const createCodeTreeParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Subdirectory to tree (default: workspace root)
    path?: string;
    // Maximum directory depth (default: 10)
    maxDepth?: number;
    // Include hidden files and directories (default: false)
    includeHidden?: boolean;
  }
): CodeTreeParams => createPayload(context, sessionId, {
  path: data.path ?? '',
  maxDepth: data.maxDepth ?? 0,
  includeHidden: data.includeHidden ?? false,
  ...data
});

/**
 * Code Tree Command Result
 */
export interface CodeTreeResult extends CommandResult {
  success: boolean;
  // Directory tree from Rust (generated type via ts-rs)
  root: TreeNode | null;
  // Total number of files in tree
  totalFiles: number;
  // Total number of directories in tree
  totalDirectories: number;
  error?: JTAGError;
}

/**
 * Factory function for creating CodeTreeResult with defaults
 */
export const createCodeTreeResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Directory tree from Rust (generated type via ts-rs)
    root?: TreeNode;
    // Total number of files in tree
    totalFiles?: number;
    // Total number of directories in tree
    totalDirectories?: number;
    error?: JTAGError;
  }
): CodeTreeResult => createPayload(context, sessionId, {
  root: data.root ?? null,
  totalFiles: data.totalFiles ?? 0,
  totalDirectories: data.totalDirectories ?? 0,
  ...data
});

/**
 * Smart Code Tree-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createCodeTreeResultFromParams = (
  params: CodeTreeParams,
  differences: Omit<CodeTreeResult, 'context' | 'sessionId'>
): CodeTreeResult => transformPayload(params, differences);

/**
 * Code Tree — Type-safe command executor
 *
 * Usage:
 *   import { CodeTree } from '...shared/CodeTreeTypes';
 *   const result = await CodeTree.execute({ ... });
 */
export const CodeTree = {
  execute(params: CommandInput<CodeTreeParams>): Promise<CodeTreeResult> {
    return Commands.execute<CodeTreeParams, CodeTreeResult>('code/tree', params as Partial<CodeTreeParams>);
  },
  commandName: 'code/tree' as const,
} as const;
