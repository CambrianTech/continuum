/**
 * Search List Command - Server Implementation
 * Routes to Rust SearchModule via continuum-core IPC
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { SearchListParams, SearchListResult } from '../shared/SearchListTypes';
import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';

export class SearchListServerCommand extends CommandBase<SearchListParams, SearchListResult> {
  private rustClient: RustCoreIPCClient;

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('search/list', context, subpath, commander);
    this.rustClient = new RustCoreIPCClient('/tmp/continuum-core.sock');
  }

  async execute(params: JTAGPayload): Promise<SearchListResult> {
    await this.rustClient.connect();
    const algorithms = await this.rustClient.searchList();
    this.rustClient.disconnect();

    return transformPayload(params, { algorithms });
  }
}
