/**
 * Genome Adapter Prune Command - Server Implementation
 *
 * Prunes unused LoRA adapters by age. Defaults to dry-run for safety.
 */

import * as fs from 'fs';
import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GenomeAdapterPruneParams, GenomeAdapterPruneResult, PrunedAdapterEntry } from '../shared/GenomeAdapterPruneTypes';
import { createGenomeAdapterPruneResultFromParams } from '../shared/GenomeAdapterPruneTypes';
import { AdapterStore, type DiscoveredAdapter } from '@system/genome/server/AdapterStore';

export class GenomeAdapterPruneServerCommand extends CommandBase<GenomeAdapterPruneParams, GenomeAdapterPruneResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/adapter-prune', context, subpath, commander);
  }

  async execute(params: GenomeAdapterPruneParams): Promise<GenomeAdapterPruneResult> {
    const isDryRun = params.dryRun !== false;  // Default: true for safety
    const keepLatest = params.keepLatest ?? 1;
    const unusedSinceDays = this.parseDuration(params.unusedSince || '30d');
    const cutoffMs = Date.now() - (unusedSinceDays * 24 * 60 * 60 * 1000);

    let discovered: DiscoveredAdapter[];
    if (params.personaId) {
      discovered = AdapterStore.discoverForPersona(params.personaId);
    } else {
      discovered = AdapterStore.discoverAll();
    }

    if (params.domain) {
      discovered = discovered.filter(a => a.manifest.traitType === params.domain);
    }

    // Group by persona+domain to enforce keepLatest
    const groups = new Map<string, DiscoveredAdapter[]>();
    for (const a of discovered) {
      const key = `${a.manifest.personaId}::${a.manifest.traitType}`;
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key)!.push(a);
    }

    // Sort each group newest-first, mark old ones for pruning
    const toPrune: DiscoveredAdapter[] = [];
    let keptCount = 0;

    for (const [, group] of groups) {
      group.sort((a, b) =>
        new Date(b.manifest.createdAt).getTime() - new Date(a.manifest.createdAt).getTime()
      );

      for (let i = 0; i < group.length; i++) {
        const adapter = group[i];
        const createdMs = new Date(adapter.manifest.createdAt).getTime();

        // Keep the N latest per domain regardless of age
        if (i < keepLatest) {
          keptCount++;
          continue;
        }

        // Prune if older than cutoff
        if (createdMs < cutoffMs) {
          toPrune.push(adapter);
        } else {
          keptCount++;
        }
      }
    }

    const prunedAdapters: PrunedAdapterEntry[] = [];
    let reclaimedMB = 0;

    for (const adapter of toPrune) {
      prunedAdapters.push({
        name: adapter.manifest.name,
        domain: adapter.manifest.traitType,
        sizeMB: adapter.manifest.sizeMB,
        createdAt: adapter.manifest.createdAt,
        personaId: adapter.manifest.personaId,
        dirPath: adapter.dirPath,
      });
      reclaimedMB += adapter.manifest.sizeMB;

      if (!isDryRun) {
        fs.rmSync(adapter.dirPath, { recursive: true, force: true });
      }
    }

    return createGenomeAdapterPruneResultFromParams(params, {
      success: true,
      prunedCount: prunedAdapters.length,
      reclaimedMB: Math.round(reclaimedMB * 100) / 100,
      prunedAdapters,
      keptCount,
      isDryRun,
    });
  }

  private parseDuration(duration: string): number {
    const match = duration.match(/^(\d+)([dhm])$/);
    if (!match) return 30;  // Default 30 days
    const value = parseInt(match[1], 10);
    switch (match[2]) {
      case 'd': return value;
      case 'h': return value / 24;
      case 'm': return value / (24 * 60);
      default: return value;
    }
  }
}
