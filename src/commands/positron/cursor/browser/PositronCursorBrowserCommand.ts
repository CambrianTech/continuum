/**
 * Positron Cursor Browser Command
 *
 * Emits events to control the PositronCursorWidget
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';
import { Events } from '@system/core/shared/Events';
import {
  POSITRON_CURSOR_EVENTS,
  type PositronFocusEvent,
  type PositronDrawEvent
} from '@widgets/positron-cursor/PositronCursorWidget';
import type {
  PositronCursorParams,
  PositronCursorResult
} from '../shared/PositronCursorTypes';
import { createPositronCursorResult } from '../shared/PositronCursorTypes';

export class PositronCursorBrowserCommand extends CommandBase<PositronCursorParams, PositronCursorResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('positron/cursor', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<PositronCursorResult> {
    const cursorParams = params as PositronCursorParams;
    const { action, x, y, selector, shape, color, duration, message, personaId, personaName } = cursorParams;

    switch (action) {
      case 'focus': {
        const focusEvent: PositronFocusEvent = {
          target: { x, y, selector },
          color,
          duration,
          message,
          mode: 'pointing',
          personaId,
          personaName
        };
        Events.emit(POSITRON_CURSOR_EVENTS.FOCUS, focusEvent);
        return createPositronCursorResult(cursorParams.context, cursorParams.sessionId, action, {
          success: true,
          message: `Focusing at ${selector || `(${x}, ${y})`}`
        });
      }

      case 'unfocus': {
        Events.emit(POSITRON_CURSOR_EVENTS.UNFOCUS, {});
        return createPositronCursorResult(cursorParams.context, cursorParams.sessionId, action, {
          success: true,
          message: 'Cursor hidden'
        });
      }

      case 'draw': {
        if (!shape) {
          return createPositronCursorResult(cursorParams.context, cursorParams.sessionId, action, {
            success: false,
            message: 'draw action requires shape parameter'
          });
        }
        const drawEvent: PositronDrawEvent = {
          target: { x, y, selector },
          shape,
          color,
          duration,
          personaId,
          personaName
        };
        Events.emit(POSITRON_CURSOR_EVENTS.DRAW, drawEvent);
        return createPositronCursorResult(cursorParams.context, cursorParams.sessionId, action, {
          success: true,
          message: `Drawing ${shape} at ${selector || `(${x}, ${y})`}`
        });
      }

      case 'clear': {
        Events.emit(POSITRON_CURSOR_EVENTS.CLEAR, {});
        return createPositronCursorResult(cursorParams.context, cursorParams.sessionId, action, {
          success: true,
          message: 'Overlay cleared'
        });
      }

      default:
        return createPositronCursorResult(cursorParams.context, cursorParams.sessionId, action, {
          success: false,
          message: `Unknown action: ${action}`
        });
    }
  }
}
