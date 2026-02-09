/**
 * Search Execute Command - Server Implementation
 * Routes to Rust SearchModule via continuum-core IPC
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { SearchExecuteParams, SearchExecuteResult } from '../shared/SearchExecuteTypes';
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../../workers/continuum-core/bindings/RustCoreIPC';

export class SearchExecuteServerCommand extends CommandBase<SearchExecuteParams, SearchExecuteResult> {
  private rustClient: RustCoreIPCClient;

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('search/execute', context, subpath, commander);
    this.rustClient = new RustCoreIPCClient(getContinuumCoreSocketPath());
  }

  async execute(payload: JTAGPayload): Promise<SearchExecuteResult> {
    const params = payload as SearchExecuteParams;

    if (!params.query) {
      throw new Error('Missing required parameter: query');
    }
    if (!params.corpus || !Array.isArray(params.corpus)) {
      throw new Error('Missing required parameter: corpus (array of strings)');
    }

    await this.rustClient.connect();
    const result = await this.rustClient.searchExecute(
      params.query,
      params.corpus,
      params.algorithm || 'bm25',
      params.params
    );
    this.rustClient.disconnect();

    return transformPayload(payload, result);
  }
}
