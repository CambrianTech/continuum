/**
 * Sentinel Logs Read Command - Server Implementation
 *
 * Routes to Rust SentinelModule for log reading.
 * Logs are stored in .continuum/jtag/logs/system/sentinels/{handle}/
 */

import { CommandBase, type ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../../system/core/types/JTAGTypes';
import type { SentinelLogsReadParams, SentinelLogsReadResult } from '../shared/SentinelLogsReadTypes';
import { RustCoreIPCClient } from '../../../../../workers/continuum-core/bindings/RustCoreIPC';

export class SentinelLogsReadServerCommand extends CommandBase<SentinelLogsReadParams, SentinelLogsReadResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('sentinel/logs/read', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<SentinelLogsReadResult> {
    const readParams = params as SentinelLogsReadParams;
    const { handle, stream, offset = 0, limit } = readParams;

    if (!handle) {
      return transformPayload(params, {
        success: false,
        handle: '',
        stream: '',
        content: '',
        lineCount: 0,
        totalLines: 0,
        truncated: false,
        error: 'Missing required parameter: handle',
      });
    }

    if (!stream) {
      return transformPayload(params, {
        success: false,
        handle,
        stream: '',
        content: '',
        lineCount: 0,
        totalLines: 0,
        truncated: false,
        error: 'Missing required parameter: stream',
      });
    }

    try {
      const rustClient = RustCoreIPCClient.getInstance();
      const result = await rustClient.sentinelLogsRead(handle, stream, offset, limit);

      return transformPayload(params, {
        success: true,
        handle,
        stream,
        content: result.content || '',
        lineCount: result.lineCount || 0,
        totalLines: result.totalLines || 0,
        truncated: result.truncated || false,
      });
    } catch (error: any) {
      return transformPayload(params, {
        success: false,
        handle,
        stream,
        content: '',
        lineCount: 0,
        totalLines: 0,
        truncated: false,
        error: error.message,
      });
    }
  }
}
