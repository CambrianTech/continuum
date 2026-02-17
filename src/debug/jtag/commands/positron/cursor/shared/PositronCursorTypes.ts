/**
 * Positron Cursor Command Types
 *
 * Enables AIs to point, highlight, and draw attention to elements in the UI.
 * The cursor is the AI's "hand" - its spatial presence in the interface.
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import { createPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

export type CursorAction = 'focus' | 'unfocus' | 'draw' | 'clear';
export type DrawShape = 'circle' | 'rectangle' | 'arrow' | 'underline';

export interface PositronCursorParams extends CommandParams {
  /** Action to perform */
  action: CursorAction;

  /** Target - either coordinates or selector */
  x?: number;
  y?: number;
  selector?: string;

  /** For draw action - shape type */
  shape?: DrawShape;

  /** Visual customization */
  color?: string;

  /** Auto-hide after duration (ms), 0 for persistent */
  duration?: number;

  /** Optional message/tooltip */
  message?: string;

  /** Persona making this cursor action */
  personaId?: string;
  personaName?: string;
}

export interface PositronCursorResult extends CommandResult {
  success: boolean;
  action: CursorAction;
  message?: string;
}

/**
 * Factory function for creating PositronCursorResult
 */
export const createPositronCursorResult = (
  context: JTAGContext,
  sessionId: UUID,
  action: CursorAction,
  data: Omit<Partial<PositronCursorResult>, 'context' | 'sessionId' | 'action'>
): PositronCursorResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  success: true,
  action,
  ...data
});

/**
 * PositronCursor — Type-safe command executor
 *
 * Usage:
 *   import { PositronCursor } from '...shared/PositronCursorTypes';
 *   const result = await PositronCursor.execute({ ... });
 */
export const PositronCursor = {
  execute(params: CommandInput<PositronCursorParams>): Promise<PositronCursorResult> {
    return Commands.execute<PositronCursorParams, PositronCursorResult>('positron/cursor', params as Partial<PositronCursorParams>);
  },
  commandName: 'positron/cursor' as const,
} as const;
