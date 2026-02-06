/**
 * Code Read Command - Shared Types
 *
 * Read a file or line range from the persona's workspace. Returns content with line numbers and metadata. Supports partial reads via start/end line parameters.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Code Read Command Parameters
 */
export interface CodeReadParams extends CommandParams {
  // Relative path to file within workspace
  filePath: string;
  // First line to read (1-indexed, inclusive)
  startLine?: number;
  // Last line to read (1-indexed, inclusive)
  endLine?: number;
}

/**
 * Factory function for creating CodeReadParams
 */
export const createCodeReadParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Relative path to file within workspace
    filePath: string;
    // First line to read (1-indexed, inclusive)
    startLine?: number;
    // Last line to read (1-indexed, inclusive)
    endLine?: number;
  }
): CodeReadParams => createPayload(context, sessionId, {
  startLine: data.startLine ?? 0,
  endLine: data.endLine ?? 0,
  ...data
});

/**
 * Code Read Command Result
 */
export interface CodeReadResult extends CommandResult {
  success: boolean;
  // File content (or line range)
  content: string;
  // Resolved file path
  filePath: string;
  // Total lines in file
  totalLines: number;
  // Number of lines returned
  linesReturned: number;
  // Start line of returned content
  startLine: number;
  // End line of returned content
  endLine: number;
  // File size in bytes
  sizeBytes: number;
  error?: JTAGError;
}

/**
 * Factory function for creating CodeReadResult with defaults
 */
export const createCodeReadResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // File content (or line range)
    content?: string;
    // Resolved file path
    filePath?: string;
    // Total lines in file
    totalLines?: number;
    // Number of lines returned
    linesReturned?: number;
    // Start line of returned content
    startLine?: number;
    // End line of returned content
    endLine?: number;
    // File size in bytes
    sizeBytes?: number;
    error?: JTAGError;
  }
): CodeReadResult => createPayload(context, sessionId, {
  content: data.content ?? '',
  filePath: data.filePath ?? '',
  totalLines: data.totalLines ?? 0,
  linesReturned: data.linesReturned ?? 0,
  startLine: data.startLine ?? 0,
  endLine: data.endLine ?? 0,
  sizeBytes: data.sizeBytes ?? 0,
  ...data
});

/**
 * Smart Code Read-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createCodeReadResultFromParams = (
  params: CodeReadParams,
  differences: Omit<CodeReadResult, 'context' | 'sessionId'>
): CodeReadResult => transformPayload(params, differences);

/**
 * Code Read — Type-safe command executor
 *
 * Usage:
 *   import { CodeRead } from '...shared/CodeReadTypes';
 *   const result = await CodeRead.execute({ ... });
 */
export const CodeRead = {
  execute(params: CommandInput<CodeReadParams>): Promise<CodeReadResult> {
    return Commands.execute<CodeReadParams, CodeReadResult>('code/read', params as Partial<CodeReadParams>);
  },
  commandName: 'code/read' as const,
} as const;
