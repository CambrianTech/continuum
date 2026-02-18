/**
 * State Content Switch Command - Shared Types
 *
 * Switch to an existing open content item (set as current/highlighted tab).
 * Does NOT add to openItems - use content/open for that.
 */

import type { CommandParams, CommandResult, CommandInput} from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../../system/core/shared/Commands';

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
  /** Content type of the switched item (for browser-side event emission) */
  readonly contentType?: string;
  /** Entity ID (e.g., room UUID for chat content) */
  readonly entityId?: string;
  /** Title of the content item */
  readonly title?: string;
  /** Error message if operation failed (e.g., item not found in openItems) */
  readonly error?: string;
}

/**
 * StateContentSwitch — Type-safe command executor
 *
 * Usage:
 *   import { StateContentSwitch } from '...shared/StateContentSwitchTypes';
 *   const result = await StateContentSwitch.execute({ ... });
 */
export const StateContentSwitch = {
  execute(params: CommandInput<StateContentSwitchParams>): Promise<StateContentSwitchResult> {
    return Commands.execute<StateContentSwitchParams, StateContentSwitchResult>('state/content/switch', params as Partial<StateContentSwitchParams>);
  },
  commandName: 'state/content/switch' as const,
} as const;
