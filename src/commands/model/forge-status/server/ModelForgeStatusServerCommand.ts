/**
 * Model Forge Status Command - Server Implementation
 *
 * Polls status.json from forge nodes on the grid to get active forge status.
 * Currently reads from the local sentinel-ai output directory.
 * Future: queries grid nodes via reticulum/SSH.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ModelForgeStatusParams, ModelForgeStatusResult, ForgeJobStatus } from '../shared/ModelForgeStatusTypes';
import { createModelForgeStatusResultFromParams } from '../shared/ModelForgeStatusTypes';
import * as fs from 'fs';
import * as path from 'path';

/** Known grid nodes with forge capability */
const FORGE_NODES = [
  { nodeId: 'bigmama', name: 'BigMama (5090)', ip: '100.124.122.107', vramGb: 32 },
];

export class ModelForgeStatusServerCommand extends CommandBase<ModelForgeStatusParams, ModelForgeStatusResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('model/forge-status', context, subpath, commander);
  }

  async execute(params: ModelForgeStatusParams): Promise<ModelForgeStatusResult> {
    const forges: ForgeJobStatus[] = [];

    // Check local forge directories
    const localForges = this.getLocalForges(params);
    forges.push(...localForges);

    // Check remote grid nodes
    const remoteForges = await this.getRemoteForges(params);
    forges.push(...remoteForges);

    return createModelForgeStatusResultFromParams(params, {
      success: true,
      forges,
    });
  }

  private getLocalForges(params: ModelForgeStatusParams): ForgeJobStatus[] {
    const forges: ForgeJobStatus[] = [];
    const forgeDirs = this.findLocalForgeDirs();

    for (const dir of forgeDirs) {
      const forge = this.parseStatusFile(path.join(dir, 'status.json'), 'local', path.basename(dir), 0);
      if (!forge) continue;
      if (params.nodeId && forge.nodeId !== params.nodeId && forge.nodeName !== params.nodeId) continue;
      forges.push(forge);
    }
    return forges;
  }

  private async getRemoteForges(params: ModelForgeStatusParams): Promise<ForgeJobStatus[]> {
    const forges: ForgeJobStatus[] = [];

    for (const node of FORGE_NODES) {
      if (params.nodeId && node.nodeId !== params.nodeId && node.name !== params.nodeId) continue;

      try {
        // Check forge status via SSH directly
        const { execSync } = await import('child_process');
        const home = process.env.HOME ?? '';
        const cmd = `ssh -i ${home}/.ssh/id_ed25519 -o ConnectTimeout=3 -o StrictHostKeyChecking=no joel@${node.ip} "cat ~/sentinel-ai/output/forged/*/status.json 2>/dev/null | tail -1; echo '|||'; ps aux | grep forge_model | grep python | grep -v grep | head -1" 2>/dev/null`;
        let output = '';
        try {
          output = execSync(cmd, { timeout: 8000, encoding: 'utf-8' }).trim();
        } catch {
          continue; // SSH failed or timed out
        }
        if (!output) continue;

        const [statusRaw, processLine] = output.split('|||').map(s => s.trim());

        // Parse status.json content
        if (statusRaw) {
          try {
            const status = JSON.parse(statusRaw);
            const isRunning = !!processLine;

            forges.push({
              nodeId: node.nodeId,
              nodeName: node.name,
              phase: isRunning ? (status.phase ?? 'running') : 'complete',
              detail: status.detail ?? '',
              model: status.model ?? 'unknown',
              domain: status.domain ?? 'unknown',
              step: status.step ?? 0,
              totalSteps: status.total_steps ?? 0,
              loss: status.loss ?? 0,
              vramGb: status.vram_gb ?? 0,
              vramTotalGb: node.vramGb,
              itPerSec: status.it_per_sec ?? 0,
              etaSeconds: status.eta_seconds ?? 0,
              cycle: status.cycle ?? 0,
              totalCycles: status.total_cycles ?? 1,
              timestamp: status.timestamp ?? new Date().toISOString(),
            });
          } catch {
            // No valid status.json — check if process is at least running
            if (processLine) {
              forges.push({
                nodeId: node.nodeId,
                nodeName: node.name,
                phase: 'loading',
                detail: 'Forge process running (downloading model or initializing)',
                model: 'Qwen3.5-27B',
                domain: 'code',
                step: 0,
                totalSteps: 0,
                loss: 0,
                vramGb: 0,
                vramTotalGb: node.vramGb,
                itPerSec: 0,
                etaSeconds: 0,
                cycle: 0,
                totalCycles: 1,
                timestamp: new Date().toISOString(),
              });
            }
          }
        } else if (processLine) {
          // Process running but no status.json yet (still loading)
          forges.push({
            nodeId: node.nodeId,
            nodeName: node.name,
            phase: 'loading',
            detail: 'Forge process running (downloading model or initializing)',
            model: 'unknown',
            domain: 'unknown',
            step: 0,
            totalSteps: 0,
            loss: 0,
            vramGb: 0,
            vramTotalGb: node.vramGb,
            itPerSec: 0,
            etaSeconds: 0,
            cycle: 0,
            totalCycles: 1,
            timestamp: new Date().toISOString(),
          });
        }
      } catch {
        // Node unreachable — skip
      }
    }

    return forges;
  }

  private parseStatusFile(statusPath: string, nodeId: string, nodeName: string, vramTotalGb: number): ForgeJobStatus | null {
    if (!fs.existsSync(statusPath)) return null;

    try {
      const raw = fs.readFileSync(statusPath, 'utf-8');
      const lines = raw.trim().split('}{');
      const lastJson = lines.length > 1 ? '{' + lines[lines.length - 1] : lines[0];
      const status = JSON.parse(lastJson);

      return {
        nodeId,
        nodeName,
        phase: status.phase ?? 'unknown',
        detail: status.detail ?? '',
        model: status.model ?? nodeName,
        domain: status.domain ?? 'unknown',
        step: status.step ?? 0,
        totalSteps: status.total_steps ?? 0,
        loss: status.loss ?? 0,
        vramGb: status.vram_gb ?? 0,
        vramTotalGb,
        itPerSec: status.it_per_sec ?? 0,
        etaSeconds: status.eta_seconds ?? 0,
        cycle: status.cycle ?? 0,
        totalCycles: status.total_cycles ?? 1,
        timestamp: status.timestamp ?? new Date().toISOString(),
      };
    } catch {
      return null;
    }
  }

  private findLocalForgeDirs(): string[] {
    const dirs: string[] = [];
    const sentinelPaths = [
      path.join(process.env.HOME ?? '', 'sentinel-ai', 'output', 'forged'),
      path.join(process.cwd(), '..', 'sentinel-ai', 'output', 'forged'),
    ];

    for (const base of sentinelPaths) {
      if (!fs.existsSync(base)) continue;
      try {
        const entries = fs.readdirSync(base, { withFileTypes: true });
        for (const entry of entries) {
          if (entry.isDirectory()) {
            dirs.push(path.join(base, entry.name));
          }
        }
      } catch {
        // Skip inaccessible directories
      }
    }
    return dirs;
  }
}
