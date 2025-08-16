/**
 * Data Read Command - Browser Implementation
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataReadParams, DataReadResult } from '../shared/DataReadTypes';

export class DataReadBrowserCommand extends CommandBase<DataReadParams, DataReadResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-read', context, subpath, commander);
  }

  async execute(params: DataReadParams): Promise<DataReadResult> {
    console.log(`🗄️ BROWSER: Delegating data read to server`);
    return await this.remoteExecute(params);
  }
}