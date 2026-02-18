/**
 * Agent Stop Command - Server Implementation
 *
 * Routes to Rust AgentModule via IPC.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';
import type { AgentStopParams, AgentStopResult } from '../shared/AgentStopTypes';

export class AgentStopServerCommand extends CommandBase<AgentStopParams, AgentStopResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('agent/stop', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<AgentStopResult> {
    const stopParams = params as AgentStopParams;

    if (!stopParams.handle) {
      return transformPayload(params, {
        success: false,
        handle: '',
        stopped: false,
        error: 'Missing required parameter: handle',
      });
    }

    const client = RustCoreIPCClient.getInstance();

    const result = await client.execute<{ stopped: boolean }>('agent/stop', {
      handle: stopParams.handle,
    });

    if (!result.success) {
      return transformPayload(params, {
        success: false,
        handle: stopParams.handle,
        stopped: false,
        error: result.error || 'Failed to stop agent',
      });
    }

    return transformPayload(params, {
      success: true,
      handle: stopParams.handle,
      stopped: result.data?.stopped ?? false,
    });
  }
}
