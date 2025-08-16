/**
 * Data List Command - Browser Implementation
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataListParams, DataListResult } from '../shared/DataListTypes';

export class DataListBrowserCommand extends CommandBase<DataListParams, DataListResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-list', context, subpath, commander);
  }

  async execute(params: DataListParams): Promise<DataListResult> {
    console.log(`🗄️ BROWSER: Delegating data list to server`);
    return await this.remoteExecute(params);
  }
}