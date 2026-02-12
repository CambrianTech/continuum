/**
 * Agent Start Command - Server Implementation
 *
 * Routes to Rust AgentModule via IPC.
 * Returns a handle immediately; agent runs asynchronously.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';
import type { AgentStartParams, AgentStartResult } from '../shared/AgentStartTypes';

export class AgentStartServerCommand extends CommandBase<AgentStartParams, AgentStartResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('agent/start', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<AgentStartResult> {
    const agentParams = params as AgentStartParams;

    if (!agentParams.task) {
      return transformPayload(params, {
        success: false,
        handle: '',
        status: 'failed',
        error: 'Missing required parameter: task',
      });
    }

    if (!agentParams.working_dir) {
      return transformPayload(params, {
        success: false,
        handle: '',
        status: 'failed',
        error: 'Missing required parameter: working_dir',
      });
    }

    if (!agentParams.model) {
      return transformPayload(params, {
        success: false,
        handle: '',
        status: 'failed',
        error: "Missing required parameter: model. Run 'candle' provider to see available models.",
      });
    }

    const client = RustCoreIPCClient.getInstance();

    const result = await client.execute<{ handle: string }>('agent/start', {
      task: agentParams.task,
      working_dir: agentParams.working_dir,
      model: agentParams.model,
      max_iterations: agentParams.max_iterations || 20,
    });

    if (!result.success) {
      return transformPayload(params, {
        success: false,
        handle: '',
        status: 'failed',
        error: result.error || 'Failed to start agent',
      });
    }

    return transformPayload(params, {
      success: true,
      handle: result.data?.handle || '',
      status: 'running',
    });
  }
}
