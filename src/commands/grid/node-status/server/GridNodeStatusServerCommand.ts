/**
 * Grid Node Status Command - Server Implementation
 *
 * Thin TS wrapper — delegates ALL logic to Rust grid/node-status handler.
 * Rust handles both local (nvidia-smi, ps) and remote (grid/send delegation).
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GridNodeStatusParams, GridNodeStatusResult } from '../shared/GridNodeStatusTypes';
import { createGridNodeStatusResultFromParams } from '../shared/GridNodeStatusTypes';
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../../workers/continuum-core/bindings/RustCoreIPC';
import { COMMANDS } from '@shared/generated-command-constants';

export class GridNodeStatusServerCommand extends CommandBase<GridNodeStatusParams, GridNodeStatusResult> {
  private rustClient: RustCoreIPCClient;

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('grid/node-status', context, subpath, commander);
    this.rustClient = new RustCoreIPCClient(getContinuumCoreSocketPath());
  }

  async execute(params: GridNodeStatusParams): Promise<GridNodeStatusResult> {
    await this.rustClient.connect();
    // Route directly to Rust grid/node-status handler — it handles local AND remote
    const response = await (this.rustClient as any).request({
      command: COMMANDS.GRID_NODE_STATUS,
      nodeId: params.nodeId ?? '',
    });

    if (!response.success) {
      throw new Error(response.error || 'grid/node-status failed');
    }

    const raw = (response.result ?? response) as Record<string, unknown>;
    return createGridNodeStatusResultFromParams(params, {
      success: true,
      state: (raw.state as string) ?? 'unknown',
      gpu: (raw.gpu as object) ?? {},
      jobs: (raw.jobs as object) ?? [],
      queue: (raw.queue as object) ?? [],
      nodeId: (raw.nodeId as string) ?? params.nodeId ?? '',
      timestamp: (raw.timestamp as string) ?? new Date().toISOString(),
    });
  }
}
