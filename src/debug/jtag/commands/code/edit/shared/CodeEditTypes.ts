/**
 * Code Edit Command - Shared Types
 *
 * Edit a file using search-replace, line-range replacement, insert-at, or append. Creates a ChangeNode for undo. Safer than full file write for targeted modifications.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Code Edit Command Parameters
 */
export interface CodeEditParams extends CommandParams {
  // Relative path to file within workspace
  filePath: string;
  // Edit mode: 'search_replace', 'line_range', 'insert_at', or 'append'
  editType: string;
  // Text to find (for search_replace mode)
  search?: string;
  // Replacement text (for search_replace mode)
  replace?: string;
  // Replace all occurrences (for search_replace mode, default: false)
  replaceAll?: boolean;
  // Start line (for line_range mode, 1-indexed)
  startLine?: number;
  // End line (for line_range mode, 1-indexed, inclusive)
  endLine?: number;
  // New content (for line_range mode)
  newContent?: string;
  // Line number to insert at (for insert_at mode)
  line?: number;
  // Content to insert or append
  content?: string;
  // Description of what this change does
  description?: string;
}

/**
 * Factory function for creating CodeEditParams
 */
export const createCodeEditParams = (
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
    // Replace all occurrences (for search_replace mode, default: false)
    replaceAll?: boolean;
    // Start line (for line_range mode, 1-indexed)
    startLine?: number;
    // End line (for line_range mode, 1-indexed, inclusive)
    endLine?: number;
    // New content (for line_range mode)
    newContent?: string;
    // Line number to insert at (for insert_at mode)
    line?: number;
    // Content to insert or append
    content?: string;
    // Description of what this change does
    description?: string;
  }
): CodeEditParams => createPayload(context, sessionId, {
  search: data.search ?? '',
  replace: data.replace ?? '',
  replaceAll: data.replaceAll ?? false,
  startLine: data.startLine ?? 0,
  endLine: data.endLine ?? 0,
  newContent: data.newContent ?? '',
  line: data.line ?? 0,
  content: data.content ?? '',
  description: data.description ?? '',
  ...data
});

/**
 * Code Edit Command Result
 */
export interface CodeEditResult extends CommandResult {
  success: boolean;
  // UUID of the ChangeNode created (for undo)
  changeId: string;
  // Resolved file path
  filePath: string;
  // New file size in bytes
  bytesWritten: number;
  error?: JTAGError;
}

/**
 * Factory function for creating CodeEditResult with defaults
 */
export const createCodeEditResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // UUID of the ChangeNode created (for undo)
    changeId?: string;
    // Resolved file path
    filePath?: string;
    // New file size in bytes
    bytesWritten?: number;
    error?: JTAGError;
  }
): CodeEditResult => createPayload(context, sessionId, {
  changeId: data.changeId ?? '',
  filePath: data.filePath ?? '',
  bytesWritten: data.bytesWritten ?? 0,
  ...data
});

/**
 * Smart Code Edit-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createCodeEditResultFromParams = (
  params: CodeEditParams,
  differences: Omit<CodeEditResult, 'context' | 'sessionId'>
): CodeEditResult => transformPayload(params, differences);

/**
 * Code Edit — Type-safe command executor
 *
 * Usage:
 *   import { CodeEdit } from '...shared/CodeEditTypes';
 *   const result = await CodeEdit.execute({ ... });
 */
export const CodeEdit = {
  execute(params: CommandInput<CodeEditParams>): Promise<CodeEditResult> {
    return Commands.execute<CodeEditParams, CodeEditResult>('code/edit', params as Partial<CodeEditParams>);
  },
  commandName: 'code/edit' as const,
} as const;
