/**
 * Data Truncate Command - Browser Implementation
 *
 * Forwards truncate requests to server via command daemon
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataTruncateParams, DataTruncateResult } from '../shared/DataTruncateTypes';

export class DataTruncateBrowserCommand extends CommandBase<DataTruncateParams, DataTruncateResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-truncate', context, subpath, commander);
  }

  async execute(params: DataTruncateParams): Promise<DataTruncateResult> {
    console.log('🗑️ BROWSER: Delegating data truncate to server');
    return await this.remoteExecute(params);
  }
}