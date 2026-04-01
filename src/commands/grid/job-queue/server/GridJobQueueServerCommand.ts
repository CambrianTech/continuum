/**
 * Grid Job Queue Command - Server Implementation
 *
 * Thin TS wrapper — delegates ALL logic to Rust grid/job-queue handler.
 * Rust reads job metadata from filesystem and checks PID liveness.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GridJobQueueParams, GridJobQueueResult } from '../shared/GridJobQueueTypes';
import { createGridJobQueueResultFromParams } from '../shared/GridJobQueueTypes';
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../../workers/continuum-core/bindings/RustCoreIPC';
import { COMMANDS } from '@shared/generated-command-constants';

export class GridJobQueueServerCommand extends CommandBase<GridJobQueueParams, GridJobQueueResult> {
  private rustClient: RustCoreIPCClient;

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('grid/job-queue', context, subpath, commander);
    this.rustClient = new RustCoreIPCClient(getContinuumCoreSocketPath());
  }

  async execute(params: GridJobQueueParams): Promise<GridJobQueueResult> {
    await this.rustClient.connect();
    const response = await (this.rustClient as any).request({
      command: COMMANDS.GRID_JOB_QUEUE,
      nodeId: params.nodeId ?? '',
      state: params.state ?? 'all',
      limit: params.limit ?? 20,
    });

    if (!response.success) {
      throw new Error(response.error || 'grid/job-queue failed');
    }

    const raw = (response.result ?? response) as Record<string, unknown>;
    return createGridJobQueueResultFromParams(params, {
      success: true,
      jobs: (raw.jobs as object) ?? [],
      summary: (raw.summary as object) ?? { queued: 0, running: 0, paused: 0, completed: 0, failed: 0 },
    });
  }
}
