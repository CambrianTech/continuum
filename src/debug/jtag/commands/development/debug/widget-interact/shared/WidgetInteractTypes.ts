/**
 * Widget Interaction Command Types
 *
 * Complete widget interaction capabilities - everything you can do in UX/devtools
 * Perfect for MCP integration where Claude needs full UI control
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import { createPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../../system/core/shared/Commands';

export type WidgetInteractionType =
  | 'click'           // Click button/element
  | 'type'            // Type in input field
  | 'callMethod'      // Call widget method directly
  | 'setProperty'     // Set widget property
  | 'sendMessage'     // Send chat message via UI
  | 'selectRoom'      // Select room in room list
  | 'triggerEvent';   // Trigger custom event

export interface WidgetInteractParams extends CommandParams {
  widgetSelector: string;                    // e.g. "chat-widget", "room-list-widget"
  action: WidgetInteractionType;

  // For click/type actions
  elementSelector?: string;                  // CSS selector within widget shadow DOM
  text?: string;                            // Text to type or message content

  // For method calls
  methodName?: string;                      // Method to call on widget
  methodArgs?: any[];                       // Arguments for method call

  // For property setting
  propertyName?: string;                    // Property to set
  propertyValue?: any;                      // Value to set

  // Screenshots
  screenshotBefore?: boolean;               // Take screenshot before action
  screenshotAfter?: boolean;                // Take screenshot after action
  screenshotFilename?: string;              // Custom filename for screenshots

  // Verification
  verifyResult?: boolean;                   // Verify action worked
  expectedChange?: string;                  // What should change (for verification)
}

export interface WidgetInteractResult extends CommandResult {
  success: boolean;
  action: WidgetInteractionType;
  widgetFound: boolean;
  widgetPath: string;

  // Action results
  actionResult?: {
    executed: boolean;
    returnValue?: any;
    error?: string;
  };

  // Element interaction
  elementFound?: boolean;
  elementPath?: string;

  // Screenshots taken
  screenshots?: {
    before?: string;      // Screenshot file path before action
    after?: string;       // Screenshot file path after action
  };

  // Verification results
  verification?: {
    performed: boolean;
    success: boolean;
    details: string;
  };

  debugging: {
    logs: string[];
    warnings: string[];
    errors: string[];
  };

  error?: string;
}

export const createWidgetInteractResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<WidgetInteractResult>, 'context' | 'sessionId'>
): WidgetInteractResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  success: false,
  action: 'click',
  widgetFound: false,
  widgetPath: '',
  debugging: {
    logs: [],
    warnings: [],
    errors: []
  },
  ...data
});
/**
 * WidgetInteract — Type-safe command executor
 *
 * Usage:
 *   import { WidgetInteract } from '...shared/WidgetInteractTypes';
 *   const result = await WidgetInteract.execute({ ... });
 */
export const WidgetInteract = {
  execute(params: CommandInput<WidgetInteractParams>): Promise<WidgetInteractResult> {
    return Commands.execute<WidgetInteractParams, WidgetInteractResult>('development/debug/widget-interact', params as Partial<WidgetInteractParams>);
  },
  commandName: 'development/debug/widget-interact' as const,
} as const;
