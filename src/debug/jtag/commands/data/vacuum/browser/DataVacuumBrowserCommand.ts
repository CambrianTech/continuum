/**
 * Data Vacuum Command - Browser Implementation
 *
 * Reclaim disk space after bulk deletes (VACUUM for SQLite, OPTIMIZE for MySQL, etc.)
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { DataVacuumParams, DataVacuumResult } from '../shared/DataVacuumTypes';

export class DataVacuumBrowserCommand extends CommandBase<DataVacuumParams, DataVacuumResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('Data Vacuum', context, subpath, commander);
  }

  async execute(params: DataVacuumParams): Promise<DataVacuumResult> {
    console.log('🌐 BROWSER: Delegating Data Vacuum to server');
    return await this.remoteExecute(params);
  }
}
