/**
 * Model Forge Command - Server Implementation
 *
 * Starts a forge job on a grid node with GPU. Routes via grid/send when
 * available, falls back to SSH for direct execution on the target node.
 * Emits status events that the FactoryWidget subscribes to.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { ModelForgeParams, ModelForgeResult } from '../shared/ModelForgeTypes';
import { createModelForgeResultFromParams } from '../shared/ModelForgeTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';
import { Events } from '@system/core/shared/Events';
import { randomUUID } from 'crypto';

export class ModelForgeServerCommand extends CommandBase<ModelForgeParams, ModelForgeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('model/forge', context, subpath, commander);
  }

  async execute(params: ModelForgeParams): Promise<ModelForgeResult> {
    // Validate required parameters
    if (!params.model || params.model.trim() === '') {
      throw new ValidationError('model', 'Missing required parameter \'model\'. Example: Qwen/Qwen3.5-4B');
    }
    if (!params.domain || params.domain.trim() === '') {
      throw new ValidationError('domain', 'Missing required parameter \'domain\'. Options: code, reasoning, general');
    }
    if (!params.steps || params.steps <= 0) {
      throw new ValidationError('steps', 'Training steps must be a positive number');
    }

    const jobId = `forge-${randomUUID().slice(0, 8)}`;
    const targetNode = params.nodeId || 'bigmama';

    // Resolve target node info
    const nodeInfo = await this.resolveNode(targetNode);

    // Build the forge command for the Python script
    const forgeArgs = this.buildForgeArgs(params);

    // Emit initial status event
    Events.emit('model:forge:step', {
      step: 0,
      total_steps: params.steps,
      loss: 0,
      phase: 'starting',
      detail: `Starting forge on ${nodeInfo.name}: ${params.model} (${params.domain})`,
      vram_gb: 0,
      timestamp: new Date().toISOString(),
      cycle: 1,
      total_cycles: params.cycles,
    });

    // Try grid/send first, fall back to SSH
    try {
      await this.startForgeViaGrid(targetNode, forgeArgs, jobId);
    } catch {
      await this.startForgeViaSSH(nodeInfo.ip, forgeArgs, jobId);
    }

    // Estimate duration based on model size and steps
    const estimatedDuration = this.estimateDuration(params.model, params.steps, params.cycles);

    return createModelForgeResultFromParams(params, {
      success: true,
      jobId,
      nodeId: targetNode,
      nodeName: nodeInfo.name,
      estimatedDuration,
    });
  }

  private buildForgeArgs(params: ModelForgeParams): string {
    // Build an alloy JSON inline and pass via --alloy
    // This is the transition path — eventually the alloy file is pre-built
    const alloy = this.buildAlloy(params);
    const alloyJson = JSON.stringify(alloy);
    // Write alloy to a temp file on the remote node, then pass it
    const escaped = alloyJson.replace(/'/g, "'\\''");
    return [
      `echo '${escaped}' > /tmp/forge-alloy-input.json`,
      `&&`,
      `python scripts/forge_model.py`,
      `--alloy /tmp/forge-alloy-input.json`,
      `--status-json`,
    ].join(' ');
  }

  private buildAlloy(params: ModelForgeParams): Record<string, unknown> {
    const base = params.model.split('/').pop()?.toLowerCase() ?? 'model';
    const stages: Record<string, unknown>[] = [
      {
        type: 'prune',
        strategy: params.pruneStrategy || 'entropy',
        level: params.pruneLevel,
      },
      {
        type: 'train',
        domain: params.domain,
        steps: params.steps,
        learningRate: params.learningRate,
      },
    ];

    if (params.experts && params.experts > 0) {
      stages.unshift({
        type: 'expert-prune',
        keepExperts: params.experts,
      });
    }

    return {
      name: `${base}-${params.domain}-forged`,
      version: '1.0.0',
      author: 'continuum-ai',
      tags: [params.domain, 'forged', 'experiential-plasticity', 'forge-alloy'],
      license: 'apache-2.0',
      source: {
        baseModel: params.model,
        architecture: base.includes('qwen3.5') ? 'qwen3_5' :
                      base.includes('qwen2') ? 'qwen2' :
                      base.includes('qwen3.5') && base.includes('moe') ? 'qwen3_5_moe' : 'llama',
        isMoE: (params.experts ?? 0) > 0,
      },
      stages,
      cycles: params.cycles,
    };
  }

  private async startForgeViaGrid(nodeId: string, command: string, _jobId: string): Promise<void> {
    // Route through grid — the node will execute the forge script
    await this.executeRemoteCommand('grid/send', {
      nodeId,
      remoteCommand: 'code/shell/execute',
      params: { command, background: true },
    });
  }

  private async startForgeViaSSH(ip: string, command: string, jobId: string): Promise<void> {
    // Direct SSH execution — forge runs in background on the target
    const sshCommand = `ssh ${ip} "cd ~/sentinel-ai && nohup ${command} > /tmp/forge-${jobId}.log 2>&1 &"`;

    await this.executeRemoteCommand('code/shell/execute', {
      command: sshCommand,
      background: true,
    });

    // Start polling status.json on the remote node
    this.startStatusPolling(ip, jobId);
  }

  private startStatusPolling(ip: string, jobId: string): void {
    const pollInterval = setInterval(async () => {
      try {
        const result = await this.executeRemoteCommand('code/shell/execute', {
          command: `ssh ${ip} "cat ~/sentinel-ai/output/forged/*/status.json 2>/dev/null | tail -1"`,
        }) as unknown as Record<string, unknown>;

        const output = result?.output as string | undefined;
        if (output) {
          try {
            const status = JSON.parse(output.trim());
            Events.emit('model:forge:step', {
              step: status.step ?? 0,
              total_steps: status.total_steps ?? 0,
              loss: status.loss ?? 0,
              phase: status.phase ?? 'unknown',
              detail: status.detail ?? '',
              vram_gb: status.vram_gb ?? 0,
              it_per_sec: status.it_per_sec ?? 0,
              eta_seconds: status.eta_seconds ?? 0,
              cycle: status.cycle ?? 0,
              total_cycles: status.total_cycles ?? 1,
              timestamp: status.timestamp ?? new Date().toISOString(),
            });

            // Stop polling when forge completes
            if (status.phase === 'complete' || status.phase === 'error') {
              clearInterval(pollInterval);
              if (status.phase === 'complete') {
                Events.emit('model:forge:complete', {
                  detail: status.detail ?? 'Forge complete',
                  improvementPct: status.improvement_pct,
                  perplexity: status.perplexity,
                  timestamp: new Date().toISOString(),
                });
              }
            }
          } catch {
            // Malformed JSON — skip this poll
          }
        }
      } catch {
        // SSH failed — node may be unreachable
      }
    }, 10_000); // Poll every 10 seconds

    // Safety: stop polling after 24 hours
    setTimeout(() => clearInterval(pollInterval), 24 * 60 * 60 * 1000);
  }

  private async resolveNode(nodeId: string): Promise<{ name: string; ip: string }> {
    try {
      const result = await this.executeRemoteCommand('grid/nodes', {}) as unknown as Record<string, unknown>;
      const nodes = result?.nodes as any[] | undefined;
      if (nodes) {
        for (const node of nodes) {
          if (node.node_name?.toLowerCase() === nodeId.toLowerCase() ||
              node.node_id === nodeId) {
            const addr = node.addresses?.[0];
            return {
              name: node.node_name ?? nodeId,
              ip: addr?.ip ?? '100.124.122.107', // BigMama default
            };
          }
        }
      }
    } catch {
      // Grid not available
    }

    // Default to BigMama
    return { name: 'BigMama', ip: '100.124.122.107' };
  }

  /** Execute a command with loose typing — for cross-command calls where params aren't known at compile time */
  private executeRemoteCommand(commandName: string, params: Record<string, unknown>): Promise<CommandResult> {
    return Commands.execute<CommandParams, CommandResult>(commandName, params as Partial<CommandParams>);
  }

  private estimateDuration(model: string, steps: number, cycles: number): string {
    // Rough estimates based on model size and 5090 performance
    const modelLower = model.toLowerCase();
    let stepsPerMinute: number;

    if (modelLower.includes('0.5b') || modelLower.includes('0.8b')) {
      stepsPerMinute = 20;
    } else if (modelLower.includes('1.5b') || modelLower.includes('3b') || modelLower.includes('4b')) {
      stepsPerMinute = 10;
    } else if (modelLower.includes('7b') || modelLower.includes('8b')) {
      stepsPerMinute = 5;
    } else if (modelLower.includes('14b')) {
      stepsPerMinute = 2.5;
    } else if (modelLower.includes('27b') || modelLower.includes('32b')) {
      stepsPerMinute = 1;
    } else {
      stepsPerMinute = 3;
    }

    const totalMinutes = (steps * cycles) / stepsPerMinute;
    if (totalMinutes < 60) return `~${Math.round(totalMinutes)} minutes`;
    const hours = Math.round(totalMinutes / 60 * 10) / 10;
    return `~${hours} hours`;
  }
}
