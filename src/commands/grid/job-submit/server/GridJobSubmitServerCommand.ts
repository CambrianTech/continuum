/**
 * Grid Job Submit Command - Server Implementation
 *
 * Thin TS wrapper — delegates ALL logic to Rust grid/job-submit handler.
 * Rust handles alloy writing, forge pipeline launch, and job metadata.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { GridJobSubmitParams, GridJobSubmitResult } from '../shared/GridJobSubmitTypes';
import { createGridJobSubmitResultFromParams } from '../shared/GridJobSubmitTypes';
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../../../core/continuum-core/bindings/RustCoreIPC';
import { COMMANDS } from '@shared/generated-command-constants';

export class GridJobSubmitServerCommand extends CommandBase<GridJobSubmitParams, GridJobSubmitResult> {
  private rustClient: RustCoreIPCClient;

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('grid/job-submit', context, subpath, commander);
    this.rustClient = new RustCoreIPCClient(getContinuumCoreSocketPath());
  }

  async execute(params: GridJobSubmitParams): Promise<GridJobSubmitResult> {
    if (!params.alloy || typeof params.alloy !== 'object') {
      throw new ValidationError('alloy', 'Missing required parameter \'alloy\' — a complete alloy JSON recipe.');
    }

    await this.rustClient.connect();
    const response = await (this.rustClient as any).request({
      command: COMMANDS.GRID_JOB_SUBMIT,
      nodeId: params.nodeId ?? '',
      alloy: params.alloy,
      priority: params.priority ?? 5,
    });

    if (!response.success) {
      throw new Error(response.error || 'grid/job-submit failed');
    }

    const raw = (response.result ?? response) as Record<string, unknown>;
    return createGridJobSubmitResultFromParams(params, {
      success: true,
      jobId: (raw.jobId as string) ?? '',
      position: (raw.position as number) ?? 0,
      nodeId: (raw.nodeId as string) ?? params.nodeId ?? '',
      estimatedStart: (raw.estimatedStart as string) ?? '',
    });
  }
}
