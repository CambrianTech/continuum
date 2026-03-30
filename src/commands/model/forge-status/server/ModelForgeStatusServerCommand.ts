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

export class ModelForgeStatusServerCommand extends CommandBase<ModelForgeStatusParams, ModelForgeStatusResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('model/forge-status', context, subpath, commander);
  }

  async execute(params: ModelForgeStatusParams): Promise<ModelForgeStatusResult> {
    const forges: ForgeJobStatus[] = [];

    const forgeDirs = this.findForgeDirs();

    for (const dir of forgeDirs) {
      const statusPath = path.join(dir, 'status.json');
      if (!fs.existsSync(statusPath)) continue;

      try {
        const raw = fs.readFileSync(statusPath, 'utf-8');
        // status.json may contain multiple concatenated JSON objects (appended)
        // Take the last valid one
        const lines = raw.trim().split('}{');
        const lastJson = lines.length > 1
          ? '{' + lines[lines.length - 1]
          : lines[0];
        const status = JSON.parse(lastJson);

        const forge: ForgeJobStatus = {
          nodeId: 'local',
          nodeName: path.basename(dir),
          phase: status.phase ?? 'unknown',
          detail: status.detail ?? '',
          model: status.model ?? path.basename(dir),
          domain: status.domain ?? 'unknown',
          step: status.step ?? 0,
          totalSteps: status.total_steps ?? 0,
          loss: status.loss ?? 0,
          vramGb: status.vram_gb ?? 0,
          vramTotalGb: 32,
          itPerSec: status.it_per_sec ?? 0,
          etaSeconds: status.eta_seconds ?? 0,
          cycle: status.cycle ?? 0,
          totalCycles: status.total_cycles ?? 1,
          timestamp: status.timestamp ?? new Date().toISOString(),
        };

        // Filter by nodeId if specified
        if (params.nodeId && forge.nodeId !== params.nodeId && forge.nodeName !== params.nodeId) {
          continue;
        }

        forges.push(forge);
      } catch {
        // Skip malformed status files
      }
    }

    return createModelForgeStatusResultFromParams(params, {
      success: true,
      forges,
    });
  }

  private findForgeDirs(): string[] {
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
