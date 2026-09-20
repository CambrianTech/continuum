/**
 * Data Delete Command - Browser Implementation
 * 
 * Browser always delegates to server for data operations
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataDeleteParams, DataDeleteResult } from '../shared/DataDeleteTypes';

export class DataDeleteBrowserCommand extends CommandBase<DataDeleteParams, DataDeleteResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-delete', context, subpath, commander);
  }

  /**
   * Browser ALWAYS delegates data operations to server
   */
  async execute(params: DataDeleteParams): Promise<DataDeleteResult> {
    console.log(`📤 BROWSER: Delegating data delete to server`);
    return await this.remoteExecute(params);
  }
}