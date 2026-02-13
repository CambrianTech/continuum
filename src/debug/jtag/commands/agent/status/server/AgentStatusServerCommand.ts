/**
 * Agent Status Command - Server Implementation
 *
 * Routes to Rust AgentModule via IPC.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';
import type { AgentStatusParams, AgentStatusResult } from '../shared/AgentStatusTypes';

export class AgentStatusServerCommand extends CommandBase<AgentStatusParams, AgentStatusResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('agent/status', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<AgentStatusResult> {
    const statusParams = params as AgentStatusParams;

    if (!statusParams.handle) {
      return transformPayload(params, {
        success: false,
        handle: '',
        task: '',
        status: 'not_found',
        iteration: 0,
        startedAt: 0,
        error: 'Missing required parameter: handle',
      });
    }

    const client = RustCoreIPCClient.getInstance();

    const result = await client.execute<AgentStatusResult>('agent/status', {
      handle: statusParams.handle,
    });

    if (!result.success) {
      return transformPayload(params, {
        success: false,
        handle: statusParams.handle,
        task: '',
        status: 'not_found',
        iteration: 0,
        startedAt: 0,
        error: result.error || 'Failed to get agent status',
      });
    }

    const data = result.data as Partial<AgentStatusResult> || {};
    return transformPayload(params, {
      success: true,
      handle: data.handle || statusParams.handle,
      task: data.task || '',
      status: data.status || 'running',
      iteration: data.iteration || 0,
      startedAt: data.startedAt || 0,
      completedAt: data.completedAt,
      summary: data.summary,
      error: data.error,
      events: data.events,
    });
  }
}
