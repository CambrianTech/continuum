/**
 * Data Update Command - Browser Implementation
 * 
 * Browser always delegates to server for data operations
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataUpdateParams, DataUpdateResult } from '../shared/DataUpdateTypes';

export class DataUpdateBrowserCommand extends CommandBase<DataUpdateParams, DataUpdateResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-update', context, subpath, commander);
  }

  /**
   * Browser ALWAYS delegates data operations to server
   */
  async execute(params: DataUpdateParams): Promise<DataUpdateResult> {
    console.log(`📤 BROWSER: Delegating data update to server`);
    return await this.remoteExecute(params);
  }
}