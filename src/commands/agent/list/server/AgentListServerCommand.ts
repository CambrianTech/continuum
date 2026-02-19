/**
 * Agent List Command - Server Implementation
 *
 * Routes to Rust AgentModule via IPC.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';
import type { AgentListParams, AgentListResult } from '../shared/AgentListTypes';

export class AgentListServerCommand extends CommandBase<AgentListParams, AgentListResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('agent/list', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<AgentListResult> {
    const client = RustCoreIPCClient.getInstance();

    const result = await client.execute<{ agents: any[] }>('agent/list', {});

    if (!result.success) {
      return transformPayload(params, {
        success: false,
        agents: [],
        error: result.error || 'Failed to list agents',
      });
    }

    return transformPayload(params, {
      success: true,
      agents: result.data?.agents || [],
    });
  }
}
