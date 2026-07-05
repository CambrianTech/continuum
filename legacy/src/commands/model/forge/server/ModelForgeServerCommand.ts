/**
 * Model Forge Command - Server Implementation
 *
 * Delegates to grid/job-submit for actual execution. This command exists as a
 * convenience wrapper that builds an alloy from individual forge parameters
 * (for CLI usage like `jtag model/forge --model Qwen/Qwen3.5-4B --domain code`).
 *
 * The FactoryWidget bypasses this and calls grid/job-submit directly with
 * a pre-built alloy from ForgeControlsElement.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { ModelForgeParams, ModelForgeResult } from '../shared/ModelForgeTypes';
import { createModelForgeResultFromParams } from '../shared/ModelForgeTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';
import { COMMANDS } from '@shared/generated-command-constants';

export class ModelForgeServerCommand extends CommandBase<ModelForgeParams, ModelForgeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('model/forge', context, subpath, commander);
  }

  async execute(params: ModelForgeParams): Promise<ModelForgeResult> {
    if (!params.model || params.model.trim() === '') {
      throw new ValidationError('model', 'Missing required parameter \'model\'. Example: Qwen/Qwen3.5-4B');
    }
    if (!params.domain || params.domain.trim() === '') {
      throw new ValidationError('domain', 'Missing required parameter \'domain\'. Options: code, reasoning, general');
    }
    if (!params.steps || params.steps <= 0) {
      throw new ValidationError('steps', 'Training steps must be a positive number');
    }

    // Build alloy from params (CLI path — widget sends pre-built alloy directly)
    const alloy = this.buildAlloy(params);

    // Resolve target node from grid
    const targetNodeId = params.nodeId || await this.resolveFirstGpuNode();

    // Delegate to grid/job-submit — Rust handles queuing, execution, status tracking
    const submitResult = await Commands.execute<CommandParams, CommandResult>(
      COMMANDS.GRID_JOB_SUBMIT,
      { nodeId: targetNodeId, alloy, priority: 5 } as Partial<CommandParams>,
    );

    const raw = submitResult as unknown as Record<string, unknown>;
    if (!raw.success) {
      throw new Error((raw.error as string) || 'grid/job-submit failed — is a grid node paired and online?');
    }

    return createModelForgeResultFromParams(params, {
      success: true,
      jobId: (raw.jobId as string) ?? '',
      nodeId: (raw.nodeId as string) ?? targetNodeId,
      nodeName: targetNodeId,
      estimatedDuration: this.estimateDuration(params.model, params.steps, params.cycles),
    });
  }

  private buildAlloy(params: ModelForgeParams): Record<string, unknown> {
    const base = params.model.split('/').pop()?.toLowerCase() ?? 'model';
    const stages: Record<string, unknown>[] = [
      { type: 'prune', strategy: params.pruneStrategy || 'entropy', level: params.pruneLevel },
      { type: 'train', domain: params.domain, steps: params.steps, learningRate: params.learningRate },
    ];

    if (params.experts && params.experts > 0) {
      stages.unshift({ type: 'expert-prune', keepExperts: params.experts });
    }

    return {
      name: `${base}-${params.domain}-forged`,
      version: '1.0.0',
      author: 'continuum-ai',
      tags: [params.domain, 'forged', 'experiential-plasticity', 'forge-alloy'],
      license: 'apache-2.0',
      source: {
        baseModel: params.model,
        architecture: base.includes('qwen3.5') ? 'qwen3_5' : base.includes('qwen2') ? 'qwen2' : 'llama',
        isMoE: (params.experts ?? 0) > 0,
      },
      stages,
      cycles: params.cycles,
    };
  }

  /** Find first grid node with compute/GPU capability */
  private async resolveFirstGpuNode(): Promise<string> {
    try {
      const result = await Commands.execute<CommandParams, CommandResult>(
        COMMANDS.GRID_NODES, {} as Partial<CommandParams>,
      ) as unknown as Record<string, unknown>;

      const nodes = result?.nodes as Array<{ node_id: string; node_name: string | null; capabilities: unknown[] }> | undefined;
      if (nodes && nodes.length > 0) {
        const gpuNode = nodes.find(n =>
          Array.isArray(n.capabilities) && n.capabilities.some((c: any) => c.type === 'compute')
        );
        return (gpuNode ?? nodes[0]).node_id;
      }
    } catch {
      // Grid not available
    }

    throw new Error('No grid nodes available. Pair a node with `jtag grid/pair` first.');
  }

  private estimateDuration(model: string, steps: number, cycles: number): string {
    const m = model.toLowerCase();
    let spm: number;
    if (m.includes('0.5b') || m.includes('0.8b')) spm = 20;
    else if (m.includes('1.5b') || m.includes('3b') || m.includes('4b')) spm = 10;
    else if (m.includes('7b') || m.includes('8b')) spm = 5;
    else if (m.includes('14b')) spm = 2.5;
    else if (m.includes('27b') || m.includes('32b')) spm = 1;
    else spm = 3;

    const totalMin = (steps * cycles) / spm;
    if (totalMin < 60) return `~${Math.round(totalMin)} minutes`;
    return `~${(Math.round(totalMin / 60 * 10) / 10)} hours`;
  }
}
