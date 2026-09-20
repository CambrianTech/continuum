/**
 * Positron Cursor Server Command
 *
 * Delegates to browser for UI control
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';
import type {
  PositronCursorParams,
  PositronCursorResult
} from '../shared/PositronCursorTypes';

export class PositronCursorServerCommand extends CommandBase<PositronCursorParams, PositronCursorResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('positron/cursor', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<PositronCursorResult> {
    // Delegate to browser - cursor is a UI element
    return await this.remoteExecute(params as PositronCursorParams);
  }
}
