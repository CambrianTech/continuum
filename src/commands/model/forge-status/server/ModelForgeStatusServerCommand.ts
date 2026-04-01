/**
 * Model Forge Status Command - Server Implementation
 *
 * Returns active forge job status by querying grid/job-queue.
 * CLI convenience: `jtag model/forge-status` shows running forges.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ModelForgeStatusParams, ModelForgeStatusResult, ForgeJobStatus } from '../shared/ModelForgeStatusTypes';
import { createModelForgeStatusResultFromParams } from '../shared/ModelForgeStatusTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';
import { COMMANDS } from '@shared/generated-command-constants';

export class ModelForgeStatusServerCommand extends CommandBase<ModelForgeStatusParams, ModelForgeStatusResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('model/forge-status', context, subpath, commander);
  }

  async execute(params: ModelForgeStatusParams): Promise<ModelForgeStatusResult> {
    const forges: ForgeJobStatus[] = [];

    // Query all grid nodes for running/queued forge jobs
    try {
      const nodesResult = await Commands.execute<CommandParams, CommandResult>(
        COMMANDS.GRID_NODES, {} as Partial<CommandParams>,
      ) as unknown as Record<string, unknown>;

      const nodes = (nodesResult?.nodes ?? []) as Array<{ node_id: string; node_name: string | null }>;

      for (const node of nodes) {
        if (params.nodeId && node.node_id !== params.nodeId && node.node_name !== params.nodeId) continue;

        try {
          const queueResult = await Commands.execute<CommandParams, CommandResult>(
            COMMANDS.GRID_JOB_QUEUE,
            { nodeId: node.node_id, state: 'running', limit: 5 } as Partial<CommandParams>,
          ) as unknown as Record<string, unknown>;

          const jobs = (queueResult?.jobs ?? []) as Array<{
            jobId: string;
            alloyName: string;
            state: string;
            progress: { cycle: number; totalCycles: number; step: number; totalSteps: number };
            startedAt: string;
          }>;

          for (const job of jobs) {
            const p = job.progress;
            forges.push({
              nodeId: node.node_id,
              nodeName: node.node_name ?? node.node_id,
              phase: job.state === 'running' ? 'training' : job.state,
              detail: job.alloyName,
              model: job.alloyName,
              domain: '',
              step: p?.step ?? 0,
              totalSteps: p?.totalSteps ?? 0,
              loss: 0,
              vramGb: 0,
              vramTotalGb: 0,
              itPerSec: 0,
              etaSeconds: 0,
              cycle: p?.cycle ?? 0,
              totalCycles: p?.totalCycles ?? 1,
              timestamp: job.startedAt ?? new Date().toISOString(),
            });
          }
        } catch {
          // Node unreachable
        }
      }
    } catch {
      // Grid not available
    }

    return createModelForgeStatusResultFromParams(params, {
      success: true,
      forges,
    });
  }
}
