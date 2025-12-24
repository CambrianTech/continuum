/**
 * State Content Close Command - Shared Types
 *
 * Close a content item (remove from user's open tabs). Handles currentItemId reassignment if closing the active tab.
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

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
