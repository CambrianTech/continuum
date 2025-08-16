/**
 * Data Create Command - Browser Implementation
 * 
 * Browser always delegates to server for data operations
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataCreateParams, DataCreateResult } from '../shared/DataCreateTypes';

export class DataCreateBrowserCommand extends CommandBase<DataCreateParams, DataCreateResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-create', context, subpath, commander);
  }

  /**
   * Browser ALWAYS delegates data operations to server
   */
  async execute(params: DataCreateParams): Promise<DataCreateResult> {
    console.log(`🗄️ BROWSER: Delegating data create to server`);
    return await this.remoteExecute(params);
  }
}