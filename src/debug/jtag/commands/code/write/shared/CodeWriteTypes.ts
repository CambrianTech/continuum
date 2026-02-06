/**
 * Code Write Command - Shared Types
 *
 * Write or create a file in the persona's workspace. Creates a ChangeNode in the change graph for undo support. File extension must be in the allowlist.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Code Write Command Parameters
 */
export interface CodeWriteParams extends CommandParams {
  // Relative path to file within workspace
  filePath: string;
  // File content to write
  content: string;
  // Description of what this change does
  description?: string;
}

/**
 * Factory function for creating CodeWriteParams
 */
export const createCodeWriteParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Relative path to file within workspace
    filePath: string;
    // File content to write
    content: string;
    // Description of what this change does
    description?: string;
  }
): CodeWriteParams => createPayload(context, sessionId, {
  description: data.description ?? '',
  ...data
});

/**
 * Code Write Command Result
 */
export interface CodeWriteResult extends CommandResult {
  success: boolean;
  // UUID of the ChangeNode created (for undo)
  changeId: string;
  // Resolved file path
  filePath: string;
  // Number of bytes written
  bytesWritten: number;
  error?: JTAGError;
}

/**
 * Factory function for creating CodeWriteResult with defaults
 */
export const createCodeWriteResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // UUID of the ChangeNode created (for undo)
    changeId?: string;
    // Resolved file path
    filePath?: string;
    // Number of bytes written
    bytesWritten?: number;
    error?: JTAGError;
  }
): CodeWriteResult => createPayload(context, sessionId, {
  changeId: data.changeId ?? '',
  filePath: data.filePath ?? '',
  bytesWritten: data.bytesWritten ?? 0,
  ...data
});

/**
 * Smart Code Write-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createCodeWriteResultFromParams = (
  params: CodeWriteParams,
  differences: Omit<CodeWriteResult, 'context' | 'sessionId'>
): CodeWriteResult => transformPayload(params, differences);

/**
 * Code Write — Type-safe command executor
 *
 * Usage:
 *   import { CodeWrite } from '...shared/CodeWriteTypes';
 *   const result = await CodeWrite.execute({ ... });
 */
export const CodeWrite = {
  execute(params: CommandInput<CodeWriteParams>): Promise<CodeWriteResult> {
    return Commands.execute<CodeWriteParams, CodeWriteResult>('code/write', params as Partial<CodeWriteParams>);
  },
  commandName: 'code/write' as const,
} as const;
