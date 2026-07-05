/**
 * Data Clear Command - Browser Implementation
 *
 * Forwards clear requests to server via command daemon
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataClearParams, DataClearResult } from '../shared/DataClearTypes';

export class DataClearBrowserCommand extends CommandBase<DataClearParams, DataClearResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-clear', context, subpath, commander);
  }

  async execute(params: DataClearParams): Promise<DataClearResult> {
    console.log('🧹 BROWSER: Delegating data clear to server');
    return await this.remoteExecute(params);
  }
}