/**
 * Daemons Command - Browser Implementation
 *
 * Forwards to server (daemons only exist on server)
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { DaemonsParams, DaemonsResult } from '../shared/DaemonsTypes';

export class DaemonsBrowserCommand extends CommandBase<DaemonsParams, DaemonsResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('system/daemons', context, subpath, commander);
  }

  async execute(params: DaemonsParams): Promise<DaemonsResult> {
    // Daemons only exist on server - forward request
    return this.remoteExecute(params);
  }
}
