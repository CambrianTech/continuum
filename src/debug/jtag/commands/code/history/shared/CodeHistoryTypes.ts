/**
 * Code History Command - Shared Types
 *
 * Get change history for a specific file or the entire workspace. Returns change graph nodes with diffs, timestamps, and descriptions.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { ChangeNode } from '@shared/generated/code/ChangeNode';

/**
 * Code History Command Parameters
 */
export interface CodeHistoryParams extends CommandParams {
  // Filter history to a specific file (optional, defaults to all)
  filePath?: string;
  // Maximum number of history entries to return (default: 50)
  limit?: number;
}

/**
 * Factory function for creating CodeHistoryParams
 */
export const createCodeHistoryParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Filter history to a specific file (optional, defaults to all)
    filePath?: string;
    // Maximum number of history entries to return (default: 50)
    limit?: number;
  }
): CodeHistoryParams => createPayload(context, sessionId, {
  filePath: data.filePath ?? '',
  limit: data.limit ?? 0,
  ...data
});

/**
 * Code History Command Result
 */
export interface CodeHistoryResult extends CommandResult {
  success: boolean;
  // Change graph nodes from Rust (generated type via ts-rs)
  nodes: ChangeNode[];
  // Total number of changes in history
  totalCount: number;
  error?: JTAGError;
}

/**
 * Factory function for creating CodeHistoryResult with defaults
 */
export const createCodeHistoryResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Change graph nodes from Rust (generated type via ts-rs)
    nodes?: ChangeNode[];
    // Total number of changes in history
    totalCount?: number;
    error?: JTAGError;
  }
): CodeHistoryResult => createPayload(context, sessionId, {
  nodes: data.nodes ?? [],
  totalCount: data.totalCount ?? 0,
  ...data
});

/**
 * Smart Code History-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createCodeHistoryResultFromParams = (
  params: CodeHistoryParams,
  differences: Omit<CodeHistoryResult, 'context' | 'sessionId'>
): CodeHistoryResult => transformPayload(params, differences);

/**
 * Code History — Type-safe command executor
 *
 * Usage:
 *   import { CodeHistory } from '...shared/CodeHistoryTypes';
 *   const result = await CodeHistory.execute({ ... });
 */
export const CodeHistory = {
  execute(params: CommandInput<CodeHistoryParams>): Promise<CodeHistoryResult> {
    return Commands.execute<CodeHistoryParams, CodeHistoryResult>('code/history', params as Partial<CodeHistoryParams>);
  },
  commandName: 'code/history' as const,
} as const;
