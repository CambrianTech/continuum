/**
 * Genome Training Overview — Server Implementation
 *
 * Aggregates ALL training data across local + grid nodes in ONE call.
 * The browser widget calls this once instead of chaining grid/send × N.
 * Filters zombie sessions (>24h stuck in curriculum).
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GenomeTrainingOverviewParams, GenomeTrainingOverviewResult } from '../shared/GenomeTrainingOverviewTypes';
import { createGenomeTrainingOverviewResultFromParams } from '../shared/GenomeTrainingOverviewTypes';
import { DataList } from '@commands/data/list/shared/DataListTypes';
import { GenomeLayers } from '@commands/genome/layers/shared/GenomeLayersTypes';
import { GenomeAcademySessionList } from '@commands/genome/academy-session-list/shared/GenomeAcademySessionListTypes';
import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';
import { Logger } from '@system/core/logging/Logger';

const log = Logger.create('genome/training-overview', 'genome');
const ZOMBIE_THRESHOLD_MS = 24 * 60 * 60 * 1000; // 24h in curriculum = dead

export class GenomeTrainingOverviewServerCommand extends CommandBase<GenomeTrainingOverviewParams, GenomeTrainingOverviewResult> {
  private _debugLog: string[] = [];

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/training-overview', context, subpath, commander);
  }

  async execute(params: GenomeTrainingOverviewParams): Promise<GenomeTrainingOverviewResult> {
    const includeGrid = params.includeGrid !== false;
    const adapters: any[] = [];
    const sessions: any[] = [];
    const nodes: any[] = [];
    this._debugLog = [];

    // 1. Local data
    await this.loadLocal(adapters, sessions, params.personaId);
    nodes.push({ nodeId: 'local', nodeName: 'local', adapterCount: adapters.length, sessionCount: sessions.length });

    // 2. Grid nodes
    if (includeGrid) {
      await this.loadGrid(adapters, sessions, nodes, params.personaId);
    }

    // 3. Summary
    const withLoss = adapters.filter(a => a.finalLoss > 0);
    const summary = {
      totalAdapters: adapters.length,
      totalSessions: sessions.length,
      activeSessions: sessions.filter(s => !['completed', 'failed', 'cancelled'].includes(s.status)).length,
      bestLoss: withLoss.length > 0 ? Math.min(...withLoss.map(a => a.finalLoss)) : 0,
      avgMaturity: adapters.length > 0 ? adapters.reduce((s, a) => s + (a.maturity ?? 0), 0) / adapters.length : 0,
    };

    return createGenomeTrainingOverviewResultFromParams(params, {
      success: true,
      adapters: adapters as any,
      sessions: sessions as any,
      nodes: nodes as any,
      summary: { ...summary, _debug: this._debugLog } as any,
    });
  }

  private async loadLocal(adapters: any[], sessions: any[], personaFilter?: string): Promise<void> {
    try {
      const usersResult = await DataList.execute({
        collection: 'users', filter: { type: 'ai' }, limit: 50, dbHandle: 'default',
      }) as any;

      for (const user of usersResult?.items ?? []) {
        if (personaFilter && user.id !== personaFilter) continue;
        const pName = user.uniqueId ?? user.displayName;

        try {
          const lr = await GenomeLayers.execute({ personaId: user.id, personaName: pName }) as any;
          for (const l of lr?.layers ?? []) {
            if (l.trainingMetrics) {
              adapters.push({
                name: l.name, domain: l.domain, baseModel: l.baseModel,
                personaName: pName, personaId: user.id, nodeName: 'local',
                finalLoss: l.trainingMetrics.finalLoss ?? 0,
                epochs: l.trainingMetrics.epochs ?? 0,
                examplesProcessed: l.trainingMetrics.examplesProcessed ?? 0,
                maturity: l.maturity ?? 0, sizeMB: l.sizeMB ?? 0,
                createdAt: l.createdAt ?? '',
                lossHistory: l.trainingMetrics.lossHistory ?? [],
                trainingDurationMs: l.trainingMetrics.trainRuntime ? l.trainingMetrics.trainRuntime * 1000 : 0,
              });
            }
          }
        } catch { /* skip */ }

        try {
          const sr = await GenomeAcademySessionList.execute({ personaId: user.id }) as any;
          for (const s of sr?.sessions ?? []) {
            if (this.isZombie(s)) continue;
            sessions.push({ ...s, personaName: s.personaName ?? pName, nodeName: 'local' });
          }
        } catch { /* skip */ }
      }
    } catch (err) {
      log.warn(`Local data load failed: ${err}`);
    }
  }

  private async loadGrid(adapters: any[], sessions: any[], nodes: any[], personaFilter?: string): Promise<void> {
    try {
      const rustClient = RustCoreIPCClient.getInstance();
      const nodesResult = await rustClient.gridNodes() as any;
      const gridNodesList = Array.isArray(nodesResult) ? nodesResult : nodesResult?.nodes ?? [];

      for (const n of gridNodesList) {
        const nodeId = n.node_id ?? n.nodeId;
        const nodeName = n.node_name ?? n.nodeName ?? nodeId;
        const gpuCap = (n.capabilities ?? []).find((c: any) => c.gpu);
        let nodeAdapters = 0, nodeSessions = 0;

        try {
          const usersResult = await rustClient.gridSend(nodeId, 'user/list', { limit: 50 }) as any;
          const userKeys = Object.keys(usersResult || {}).slice(0, 8).join(',');
          const userCount = (usersResult?.users ?? []).length;
          this._debugLog.push(`${nodeName}: keys=[${userKeys}] users=${userCount}`);
          for (const user of usersResult?.users ?? []) {
            if (user.type === 'human') continue;
            if (personaFilter && user.id !== personaFilter) continue;
            const pName = user.uniqueId ?? user.displayName;

            try {
              const lr = await rustClient.gridSend(nodeId, 'genome/layers', { personaId: user.id, personaName: pName }) as any;
              this._debugLog.push(`${pName} layers=${(lr?.layers ?? []).length} keys=[${Object.keys(lr || {}).slice(0,5).join(',')}]`);
              for (const l of lr?.layers ?? []) {
                if (l.trainingMetrics) {
                  adapters.push({
                    name: l.name, domain: l.domain, baseModel: l.baseModel,
                    personaName: pName, personaId: user.id, nodeName,
                    finalLoss: l.trainingMetrics.finalLoss ?? 0,
                    epochs: l.trainingMetrics.epochs ?? 0,
                    examplesProcessed: l.trainingMetrics.examplesProcessed ?? 0,
                    maturity: l.maturity ?? 0, sizeMB: l.sizeMB ?? 0,
                    createdAt: l.createdAt ?? '',
                    lossHistory: l.trainingMetrics.lossHistory ?? [],
                    trainingDurationMs: l.trainingMetrics.trainRuntime ? l.trainingMetrics.trainRuntime * 1000 : 0,
                  });
                  nodeAdapters++;
                }
              }
            } catch (err) { this._debugLog.push(`${pName} layers ERROR: ${err}`); }
          }

          try {
            const sr = await rustClient.gridSend(nodeId, 'genome/academy-session-list', {}) as any;
            for (const s of sr?.sessions ?? []) {
              if (this.isZombie(s)) continue;
              sessions.push({ ...s, nodeName });
              nodeSessions++;
            }
          } catch { /* skip */ }
        } catch (err) {
          log.warn(`Grid node ${nodeName} failed: ${err}`);
        }

        nodes.push({ nodeId, nodeName, adapterCount: nodeAdapters, sessionCount: nodeSessions, gpu: gpuCap?.gpu, vramMb: gpuCap?.vram_mb });
      }
    } catch (err) {
      log.warn(`Grid load failed: ${err}`);
    }
  }

  private isZombie(session: any): boolean {
    const age = Date.now() - new Date(session.createdAt).getTime();
    return session.status === 'curriculum' && age > ZOMBIE_THRESHOLD_MS;
  }
}
