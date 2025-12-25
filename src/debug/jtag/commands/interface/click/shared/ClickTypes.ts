// ISSUES: 0 open, last updated 2025-07-25 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Click Command - Shared Types for Element Interaction
 * 
 * Minimal types for clicking DOM elements. Follows screenshot/navigate pattern
 * with clean inheritance and Object.assign() initialization.
 * 
 * DESIGN ANALYSIS:
 * ✅ Focused on single action - clicking elements
 * ✅ Clean parameter interface with optional properties
 * ✅ Proper constructor pattern with Object.assign()
 * ✅ Result type includes success state and metadata
 * ✅ No over-engineering - just what's needed for clicks
 * 
 * SCOPE:
 * - Browser: Direct DOM element.click() calls
 * - Server: Delegates to browser context
 * - Consistent interface across contexts
 */

import { CommandParams, CommandResult, createPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export interface ClickParams extends CommandParams {
  readonly selector: string;
  readonly button?: 'left' | 'right' | 'middle';
  readonly timeout?: number;
  readonly shadowRoot?: boolean;  // For clicking elements inside widget shadow DOMs
  readonly innerSelector?: string;  // Selector for element inside widget's shadow root
  readonly text?: string;  // Find element containing this text (searches in shadow DOM for widgets)
}

export const createClickParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    selector: string;  // Required, not optional!
    button?: 'left' | 'right' | 'middle';
    timeout?: number;
    shadowRoot?: boolean;
    innerSelector?: string;
  }
): ClickParams => createPayload(context, sessionId, {
  button: data.button ?? 'left',
  timeout: data.timeout ?? 30000,
  ...data  // selector is required, so it's in data
});

export interface ClickResult extends CommandResult {
  readonly success: boolean;
  readonly selector: string;
  readonly clicked: boolean;
  readonly error?: JTAGError;
  readonly timestamp: string;
}

export const createClickResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    selector?: string;
    clicked?: boolean;
    error?: JTAGError;
  }
): ClickResult => createPayload(context, sessionId, {
  selector: data.selector ?? '',
  clicked: data.clicked ?? false,
  timestamp: new Date().toISOString(),
  ...data
});