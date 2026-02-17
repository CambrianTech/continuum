/**
 * Sentinel Logs List Command - Server Implementation
 *
 * Routes to Rust SentinelModule for log listing.
 * Logs are stored in .continuum/jtag/logs/system/sentinels/{handle}/
 */

import { CommandBase, type ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../../system/core/types/JTAGTypes';
import type { SentinelLogsListParams, SentinelLogsListResult } from '../shared/SentinelLogsListTypes';
import { RustCoreIPCClient } from '../../../../../workers/continuum-core/bindings/RustCoreIPC';

export class SentinelLogsListServerCommand extends CommandBase<SentinelLogsListParams, SentinelLogsListResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('sentinel/logs/list', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<SentinelLogsListResult> {
    const listParams = params as SentinelLogsListParams;
    const { handle } = listParams;

    if (!handle) {
      return transformPayload(params, {
        success: false,
        handle: '',
        logsDir: '',
        streams: [],
        error: 'Missing required parameter: handle',
      });
    }

    try {
      const rustClient = RustCoreIPCClient.getInstance();
      const result = await rustClient.sentinelLogsList(handle);

      return transformPayload(params, {
        success: true,
        handle,
        logsDir: result.logsDir || '',
        streams: result.streams || [],
      });
    } catch (error: any) {
      return transformPayload(params, {
        success: false,
        handle,
        logsDir: '',
        streams: [],
        error: error.message,
      });
    }
  }
}
