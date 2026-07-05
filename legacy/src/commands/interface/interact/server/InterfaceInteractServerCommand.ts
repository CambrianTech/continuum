/**
 * Interface Interact Command - Server Implementation
 *
 * Delegates to browser — DOM interaction requires browser context.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { InterfaceInteractParams, InterfaceInteractResult } from '../shared/InterfaceInteractTypes';

export class InterfaceInteractServerCommand extends CommandBase<InterfaceInteractParams, InterfaceInteractResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('interface/interact', context, subpath, commander);
  }

  async execute(params: InterfaceInteractParams): Promise<InterfaceInteractResult> {
    return await this.remoteExecute(params);
  }
}
