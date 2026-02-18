/**
 * Search Params Command - Server Implementation
 * Routes to Rust SearchModule via continuum-core IPC
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { SearchParamsParams, SearchParamsResult } from '../shared/SearchParamsTypes';
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../../workers/continuum-core/bindings/RustCoreIPC';

export class SearchParamsServerCommand extends CommandBase<SearchParamsParams, SearchParamsResult> {
  private rustClient: RustCoreIPCClient;

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('search/params', context, subpath, commander);
    this.rustClient = new RustCoreIPCClient(getContinuumCoreSocketPath());
  }

  async execute(payload: JTAGPayload): Promise<SearchParamsResult> {
    const params = payload as SearchParamsParams;

    if (!params.algorithm) {
      throw new Error('Missing required parameter: algorithm');
    }

    await this.rustClient.connect();
    const result = await this.rustClient.searchParams(params.algorithm);
    this.rustClient.disconnect();

    return transformPayload(payload, {
      algorithm: params.algorithm,
      ...result,
    });
  }
}
