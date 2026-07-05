/**
 * Tree Command - Browser Implementation
 *
 * No browser-specific logic needed - all logic is in shared TreeCommand.
 */

import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { TreeCommand } from '../shared/TreeCommand';
import type { TreeParams, TreeResult } from '../shared/TreeTypes';

export class TreeBrowserCommand extends TreeCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  // All logic inherited from shared TreeCommand
}
