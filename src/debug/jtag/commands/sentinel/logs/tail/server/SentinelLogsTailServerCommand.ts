/**
 * Sentinel Logs Tail Command - Server Implementation
 *
 * Routes to Rust SentinelModule for log tailing.
 * Logs are stored in .continuum/jtag/logs/system/sentinels/{handle}/
 */

import { CommandBase, type ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../../system/core/types/JTAGTypes';
import type { SentinelLogsTailParams, SentinelLogsTailResult } from '../shared/SentinelLogsTailTypes';
import { RustCoreIPCClient } from '../../../../../workers/continuum-core/bindings/RustCoreIPC';

const DEFAULT_LINES = 20;

export class SentinelLogsTailServerCommand extends CommandBase<SentinelLogsTailParams, SentinelLogsTailResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('sentinel/logs/tail', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<SentinelLogsTailResult> {
    const tailParams = params as SentinelLogsTailParams;
    const { handle, stream, lines = DEFAULT_LINES } = tailParams;

    if (!handle) {
      return transformPayload(params, {
        success: false,
        handle: '',
        stream: '',
        content: '',
        lineCount: 0,
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
        error: 'Missing required parameter: stream',
      });
    }

    try {
      const rustClient = RustCoreIPCClient.getInstance();
      const result = await rustClient.sentinelLogsTail(handle, stream, lines);

      return transformPayload(params, {
        success: true,
        handle,
        stream,
        content: result.content || '',
        lineCount: result.lineCount || 0,
      });
    } catch (error: any) {
      return transformPayload(params, {
        success: false,
        handle,
        stream,
        content: '',
        lineCount: 0,
        error: error.message,
      });
    }
  }
}
