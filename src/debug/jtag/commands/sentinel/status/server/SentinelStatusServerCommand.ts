/**
 * Sentinel Status Command - Server Implementation
 *
 * Queries Rust SentinelModule directly for handle status.
 * No TypeScript status tracking.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { SentinelStatusParams, SentinelStatusResult } from '../shared/SentinelStatusTypes';
import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';

export class SentinelStatusServerCommand extends CommandBase<SentinelStatusParams, SentinelStatusResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('sentinel/status', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<SentinelStatusResult> {
    const handle = (params as SentinelStatusParams).handle;

    if (!handle) {
      return transformPayload(params, {
        success: false,
        handle: '',
        status: 'not_found',
        error: 'Handle is required',
      });
    }

    // Query Rust directly
    const rustClient = RustCoreIPCClient.getInstance();

    try {
      const result = await rustClient.sentinelStatus(handle);

      return transformPayload(params, {
        success: true,
        handle: result.handle.id,
        status: result.handle.status,
        progress: result.handle.progress,
        exitCode: result.handle.exitCode,
        error: result.handle.error,
        workingDir: result.handle.workingDir,
        logsDir: result.handle.logsDir,
      });
    } catch (error: any) {
      return transformPayload(params, {
        success: false,
        handle,
        status: 'not_found',
        error: error.message,
      });
    }
  }
}
