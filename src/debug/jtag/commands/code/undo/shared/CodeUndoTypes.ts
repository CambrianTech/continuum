/**
 * Code Undo Command - Shared Types
 *
 * Undo a specific change or the last N changes. Applies reverse diffs from the change graph to restore previous file state.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { WriteResult } from '@shared/generated/code/WriteResult';

/**
 * Code Undo Command Parameters
 */
export interface CodeUndoParams extends CommandParams {
  // UUID of a specific change to undo
  changeId?: string;
  // Number of most recent changes to undo (default: 1)
  count?: number;
}

/**
 * Factory function for creating CodeUndoParams
 */
export const createCodeUndoParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // UUID of a specific change to undo
    changeId?: string;
    // Number of most recent changes to undo (default: 1)
    count?: number;
  }
): CodeUndoParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  changeId: data.changeId ?? '',
  count: data.count ?? 0,
  ...data
});

/**
 * Code Undo Command Result
 */
export interface CodeUndoResult extends CommandResult {
  success: boolean;
  // Undo results from Rust (generated type via ts-rs)
  changesUndone: WriteResult[];
  error?: JTAGError;
}

/**
 * Factory function for creating CodeUndoResult with defaults
 */
export const createCodeUndoResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Undo results from Rust (generated type via ts-rs)
    changesUndone?: WriteResult[];
    error?: JTAGError;
  }
): CodeUndoResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  changesUndone: data.changesUndone ?? [],
  ...data
});

/**
 * Smart Code Undo-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createCodeUndoResultFromParams = (
  params: CodeUndoParams,
  differences: Omit<CodeUndoResult, 'context' | 'sessionId'>
): CodeUndoResult => transformPayload(params, differences);

/**
 * Code Undo — Type-safe command executor
 *
 * Usage:
 *   import { CodeUndo } from '...shared/CodeUndoTypes';
 *   const result = await CodeUndo.execute({ ... });
 */
export const CodeUndo = {
  execute(params: CommandInput<CodeUndoParams>): Promise<CodeUndoResult> {
    return Commands.execute<CodeUndoParams, CodeUndoResult>('code/undo', params as Partial<CodeUndoParams>);
  },
  commandName: 'code/undo' as const,
} as const;
