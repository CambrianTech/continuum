/**
 * Search Vector Command - Server Implementation
 * Routes to Rust SearchModule via continuum-core IPC
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { SearchVectorParams, SearchVectorResult } from '../shared/SearchVectorTypes';
import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';

export class SearchVectorServerCommand extends CommandBase<SearchVectorParams, SearchVectorResult> {
  private rustClient: RustCoreIPCClient;

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('search/vector', context, subpath, commander);
    this.rustClient = new RustCoreIPCClient('/tmp/continuum-core.sock');
  }

  async execute(payload: JTAGPayload): Promise<SearchVectorResult> {
    const params = payload as SearchVectorParams;

    if (!params.queryVector || !Array.isArray(params.queryVector)) {
      throw new Error('Missing required parameter: queryVector (array of numbers)');
    }
    if (!params.corpusVectors || !Array.isArray(params.corpusVectors)) {
      throw new Error('Missing required parameter: corpusVectors (array of arrays)');
    }

    await this.rustClient.connect();
    const result = await this.rustClient.searchVector(
      params.queryVector,
      params.corpusVectors,
      params.normalize ?? true,
      params.threshold ?? 0.0
    );
    this.rustClient.disconnect();

    return transformPayload(payload, {
      algorithm: 'cosine',
      ...result,
    });
  }
}
