/**
 * Code Diff Command - Shared Types
 *
 * Preview an edit as a unified diff without applying it. Useful for reviewing changes before committing them. Uses the same edit modes as code/edit.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Code Diff Command Parameters
 */
export interface CodeDiffParams extends CommandParams {
  // Relative path to file within workspace
  filePath: string;
  // Edit mode: 'search_replace', 'line_range', 'insert_at', or 'append'
  editType: string;
  // Text to find (for search_replace mode)
  search?: string;
  // Replacement text (for search_replace mode)
  replace?: string;
  // Replace all occurrences (for search_replace mode)
  replaceAll?: boolean;
  // Start line (for line_range mode)
  startLine?: number;
  // End line (for line_range mode)
  endLine?: number;
  // New content (for line_range mode)
  newContent?: string;
  // Line number (for insert_at mode)
  line?: number;
  // Content to insert or append
  content?: string;
}

/**
 * Factory function for creating CodeDiffParams
 */
export const createCodeDiffParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Relative path to file within workspace
    filePath: string;
    // Edit mode: 'search_replace', 'line_range', 'insert_at', or 'append'
    editType: string;
    // Text to find (for search_replace mode)
    search?: string;
    // Replacement text (for search_replace mode)
    replace?: string;
    // Replace all occurrences (for search_replace mode)
    replaceAll?: boolean;
    // Start line (for line_range mode)
    startLine?: number;
    // End line (for line_range mode)
    endLine?: number;
    // New content (for line_range mode)
    newContent?: string;
    // Line number (for insert_at mode)
    line?: number;
    // Content to insert or append
    content?: string;
  }
): CodeDiffParams => createPayload(context, sessionId, {
  search: data.search ?? '',
  replace: data.replace ?? '',
  replaceAll: data.replaceAll ?? false,
  startLine: data.startLine ?? 0,
  endLine: data.endLine ?? 0,
  newContent: data.newContent ?? '',
  line: data.line ?? 0,
  content: data.content ?? '',
  ...data
});

/**
 * Code Diff Command Result
 */
export interface CodeDiffResult extends CommandResult {
  success: boolean;
  // Unified diff text showing the proposed changes
  unified: string;
  error?: JTAGError;
}

/**
 * Factory function for creating CodeDiffResult with defaults
 */
export const createCodeDiffResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Unified diff text showing the proposed changes
    unified?: string;
    error?: JTAGError;
  }
): CodeDiffResult => createPayload(context, sessionId, {
  unified: data.unified ?? '',
  ...data
});

/**
 * Smart Code Diff-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createCodeDiffResultFromParams = (
  params: CodeDiffParams,
  differences: Omit<CodeDiffResult, 'context' | 'sessionId'>
): CodeDiffResult => transformPayload(params, differences);

/**
 * Code Diff — Type-safe command executor
 *
 * Usage:
 *   import { CodeDiff } from '...shared/CodeDiffTypes';
 *   const result = await CodeDiff.execute({ ... });
 */
export const CodeDiff = {
  execute(params: CommandInput<CodeDiffParams>): Promise<CodeDiffResult> {
    return Commands.execute<CodeDiffParams, CodeDiffResult>('code/diff', params as Partial<CodeDiffParams>);
  },
  commandName: 'code/diff' as const,
} as const;
