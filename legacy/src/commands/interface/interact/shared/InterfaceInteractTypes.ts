/**
 * Interface Interact Command - Shared Types
 *
 * Interact with UI elements — click, type, select, scroll. Enables personas to navigate and modify the UI via DOM selectors. Works through shadow DOM boundaries (Lit web components).
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Interface Interact Command Parameters
 */
export interface InterfaceInteractParams extends CommandParams {
  // Interaction type: click, type, select, scroll, focus, clear, check
  action: string;
  // CSS selector for target element. Supports shadow DOM piercing via >> (e.g. 'chat-widget >> .send-btn')
  selector: string;
  // Value for type/select actions. Text to type or option value to select.
  value?: string;
  // Scroll direction: up, down, left, right (default: down)
  direction?: string;
  // Scroll amount in pixels (default: 300)
  amount?: number;
  // Wait time after interaction for UI to settle (default: 100)
  waitAfterMs?: number;
  // Max time to wait for element to appear in DOM (default: 0 = immediate, no waiting).
  // Use after tab switches when elements mount asynchronously.
  waitForMs?: number;
}

/**
 * Factory function for creating InterfaceInteractParams
 */
export const createInterfaceInteractParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Interaction type: click, type, select, scroll, focus, clear, check
    action: string;
    // CSS selector for target element. Supports shadow DOM piercing via >> (e.g. 'chat-widget >> .send-btn')
    selector: string;
    // Value for type/select actions. Text to type or option value to select.
    value: string;
    // Scroll direction: up, down, left, right (default: down)
    direction: string;
    // Scroll amount in pixels (default: 300)
    amount: number;
    // Wait time after interaction for UI to settle (default: 100)
    waitAfterMs: number;
  }
): InterfaceInteractParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,

  ...data
});

/**
 * Interface Interact Command Result
 */
export interface InterfaceInteractResult extends CommandResult {
  success: boolean;
  // True if the selector matched an element
  elementFound: boolean;
  // Tag name of the matched element (e.g. button, input, select)
  elementTag: string;
  // Text content of the element (truncated to 200 chars)
  elementText: string;
  // Previous value before type/select/clear (for undo context)
  previousValue: string;
  error?: JTAGError;
}

/**
 * Factory function for creating InterfaceInteractResult with defaults
 */
export const createInterfaceInteractResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // True if the selector matched an element
    elementFound?: boolean;
    // Tag name of the matched element (e.g. button, input, select)
    elementTag?: string;
    // Text content of the element (truncated to 200 chars)
    elementText?: string;
    // Previous value before type/select/clear (for undo context)
    previousValue?: string;
    error?: JTAGError;
  }
): InterfaceInteractResult => createPayload(context, sessionId, {
  ...data,
  success: data.success ?? false,
  elementFound: data.elementFound ?? false,
  elementTag: data.elementTag ?? '',
  elementText: data.elementText ?? '',
  previousValue: data.previousValue ?? '',
});

/**
 * Smart Interface Interact-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createInterfaceInteractResultFromParams = (
  params: InterfaceInteractParams,
  differences: Omit<InterfaceInteractResult, 'context' | 'sessionId' | 'userId'>
): InterfaceInteractResult => transformPayload(params, differences);

/**
 * Interface Interact — Type-safe command executor
 *
 * Usage:
 *   import { InterfaceInteract } from '...shared/InterfaceInteractTypes';
 *   const result = await InterfaceInteract.execute({ ... });
 */
export const InterfaceInteract = {
  execute(params: CommandInput<InterfaceInteractParams>): Promise<InterfaceInteractResult> {
    return Commands.execute<InterfaceInteractParams, InterfaceInteractResult>('interface/interact', params as Partial<InterfaceInteractParams>);
  },
  commandName: 'interface/interact' as const,
} as const;
