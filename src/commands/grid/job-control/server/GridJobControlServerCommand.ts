/**
 * Grid Job Control Command - Server Implementation
 *
 * Thin TS wrapper — delegates ALL logic to Rust grid/job-control handler.
 * Rust handles signal sending (SIGSTOP/SIGCONT/SIGTERM) and job state management.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { GridJobControlParams, GridJobControlResult } from '../shared/GridJobControlTypes';
import { createGridJobControlResultFromParams } from '../shared/GridJobControlTypes';
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../../workers/continuum-core/bindings/RustCoreIPC';
import { COMMANDS } from '@shared/generated-command-constants';

export class GridJobControlServerCommand extends CommandBase<GridJobControlParams, GridJobControlResult> {
  private rustClient: RustCoreIPCClient;

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('grid/job-control', context, subpath, commander);
    this.rustClient = new RustCoreIPCClient(getContinuumCoreSocketPath());
  }

  async execute(params: GridJobControlParams): Promise<GridJobControlResult> {
    if (!params.jobId) {
      throw new ValidationError('jobId', 'Missing required parameter \'jobId\'.');
    }
    if (!params.action || !['pause', 'resume', 'cancel'].includes(params.action)) {
      throw new ValidationError('action', `Invalid action '${params.action}'. Must be: pause, resume, cancel.`);
    }

    await this.rustClient.connect();
    const response = await (this.rustClient as any).request({
      command: COMMANDS.GRID_JOB_CONTROL,
      jobId: params.jobId,
      action: params.action,
      nodeId: params.nodeId ?? '',
    });

    if (!response.success) {
      throw new Error(response.error || 'grid/job-control failed');
    }

    const raw = (response.result ?? response) as Record<string, unknown>;
    return createGridJobControlResultFromParams(params, {
      success: true,
      jobId: (raw.jobId as string) ?? params.jobId,
      previousState: (raw.previousState as string) ?? '',
      newState: (raw.newState as string) ?? '',
      checkpoint: (raw.checkpoint as object) ?? {},
    });
  }
}
