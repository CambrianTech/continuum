/**
 * State Content Switch Command - Shared Types
 *
 * Switch to an existing open content item (set as current/highlighted tab).
 * Does NOT add to openItems - use content/open for that.
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * State Content Switch Command Parameters
 */
export interface StateContentSwitchParams extends CommandParams {
  /** User ID whose content state to modify */
  readonly userId: UUID;
  /** ID of the content item to switch to (must already be in openItems) */
  readonly contentItemId: UUID;
}

/**
 * State Content Switch Command Result
 */
export interface StateContentSwitchResult extends CommandResult {
  /** Whether the switch operation succeeded */
  readonly success: boolean;
  /** Current item ID after switching */
  readonly currentItemId: UUID;
  /** Error message if operation failed (e.g., item not found in openItems) */
  readonly error?: string;
}
