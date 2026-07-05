/**
 * State Content Close Command - Shared Types
 *
 * Close a content item (remove from user's open tabs). Handles currentItemId reassignment if closing the active tab.
 */

import type { CommandParams, CommandResult, CommandInput} from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';

/**
 * State Content Close Command Parameters
 */
export interface StateContentCloseParams extends CommandParams {
  /** User ID whose content state to modify */
  readonly userId: UUID;
  /** ID of the content item to close */
  readonly contentItemId: UUID;
}

/**
 * State Content Close Command Result
 */
export interface StateContentCloseResult extends CommandResult {
  /** Whether the close operation succeeded */
  readonly success: boolean;
  /** Number of open items after closing */
  readonly openItemsCount: number;
  /** New current item ID after closing (may change if closed item was active) */
  readonly currentItemId?: UUID;
  /** Error message if operation failed */
  readonly error?: string;
}

/**
 * StateContentClose — Type-safe command executor
 *
 * Usage:
 *   import { StateContentClose } from '...shared/StateContentCloseTypes';
 *   const result = await StateContentClose.execute({ ... });
 */
export const StateContentClose = {
  execute(params: CommandInput<StateContentCloseParams>): Promise<StateContentCloseResult> {
    return Commands.execute<StateContentCloseParams, StateContentCloseResult>('state/content/close', params as Partial<StateContentCloseParams>);
  },
  commandName: 'state/content/close' as const,
} as const;

/**
 * Factory function for creating StateContentCloseParams
 */
export const createStateContentCloseParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<StateContentCloseParams, 'context' | 'sessionId' | 'userId'>
): StateContentCloseParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating StateContentCloseResult with defaults
 */
export const createStateContentCloseResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<StateContentCloseResult, 'context' | 'sessionId' | 'userId'>
): StateContentCloseResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart state/content/close-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createStateContentCloseResultFromParams = (
  params: StateContentCloseParams,
  differences: Omit<StateContentCloseResult, 'context' | 'sessionId' | 'userId'>
): StateContentCloseResult => transformPayload(params, differences);

