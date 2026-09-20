/**
 * Genome Training Overview — Server Implementation
 *
 * Aggregates ALL training data across local + grid nodes in ONE call.
 * The browser widget calls this once instead of chaining grid/send × N.
 * Filters zombie sessions (>24h stuck in curriculum).
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GenomeTrainingOverviewParams, GenomeTrainingOverviewResult, TrainingAdapterInfo, TrainingSessionInfo, TrainingNodeInfo } from '../shared/GenomeTrainingOverviewTypes';
import { createGenomeTrainingOverviewResultFromParams } from '../shared/GenomeTrainingOverviewTypes';
import { DataList } from '@commands/data/list/shared/DataListTypes';
import { GenomeLayers } from '@commands/genome/layers/shared/GenomeLayersTypes';
import { GenomeAcademySessionList } from '@commands/genome/academy-session-list/shared/GenomeAcademySessionListTypes';
import type { AcademySessionSummary } from '@commands/genome/academy-session-list/shared/GenomeAcademySessionListTypes';
import { RustCoreIPCClient } from '../../../../../core/continuum-core/bindings/RustCoreIPC';
import type { GridNode, NodeCapability } from '../../../../../core/continuum-core/bindings/modules/grid';
import { UserEntity } from '@system/data/entities/UserEntity';
import { Logger } from '@system/core/logging/Logger';

/** Shape returned by grid/send for user/list */
interface GridUserListResponse {
  users?: Array<{ id: string; type: string; uniqueId?: string; displayName?: string }>;
}

/** Shape returned by grid/send for genome/layers */
interface GridGenomeLayersResponse {
  layers?: Array<{
    id?: string;
    name: string;
    domain: string;
    baseModel: string;
    createdAt?: string;
    sizeMB?: number;
    maturity?: number;
    trainingMetrics?: {
      finalLoss: number;
      epochs: number;
      examplesProcessed: number;
      lossHistory?: number[];
      trainRuntime?: number;
    };
  }>;
}

/** Shape returned by grid/send for genome/academy-session-list */
interface GridAcademySessionListResponse {
  sessions?: AcademySessionSummary[];
}

const log = Logger.create('genome/training-overview', 'genome');
const ZOMBIE_THRESHOLD_MS = 24 * 60 * 60 * 1000; // 24h in curriculum = dead

export class GenomeTrainingOverviewServerCommand extends CommandBase<GenomeTrainingOverviewParams, GenomeTrainingOverviewResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/training-overview', context, subpath, commander);
  }

  async execute(params: GenomeTrainingOverviewParams): Promise<GenomeTrainingOverviewResult> {
    const includeGrid = params.includeGrid !== false;
    const adapters: TrainingAdapterInfo[] = [];
    const sessions: TrainingSessionInfo[] = [];
    const nodes: TrainingNodeInfo[] = [];
    // 1. Local data
    await this.loadLocal(adapters, sessions, params.personaId);
    nodes.push({ nodeId: 'local', nodeName: 'local', adapterCount: adapters.length, sessionCount: sessions.length });

    // 2. Grid nodes
    if (includeGrid) {
      await this.loadGrid(adapters, sessions, nodes, params.personaId);
    }

    // 3. Summary
    const withLoss = adapters.filter(a => a.finalLoss > 0);
    const totalExamples = adapters.reduce((s, a) => s + (a.examplesProcessed ?? 0), 0);
    const totalTrainingTime = adapters.reduce((s, a) => s + (a.trainingDurationMs ?? 0), 0);
    const domains = new Map<string, number>();
    for (const a of adapters) domains.set(a.domain, (domains.get(a.domain) ?? 0) + 1);

    const summary = {
      totalAdapters: adapters.length,
      totalSessions: sessions.length,
      activeSessions: sessions.filter(s => !['completed', 'failed', 'cancelled'].includes(s.status)).length,
      bestLoss: withLoss.length > 0 ? Math.min(...withLoss.map(a => a.finalLoss)) : 0,
      avgMaturity: adapters.length > 0 ? adapters.reduce((s, a) => s + (a.maturity ?? 0), 0) / adapters.length : 0,
      totalExamples,
      totalTrainingTimeMs: totalTrainingTime,
      domains: Object.fromEntries(domains),
    };

    return createGenomeTrainingOverviewResultFromParams(params, {
      success: true,
      adapters,
      sessions,
      nodes,
      summary,
    });
  }

  private async loadLocal(adapters: TrainingAdapterInfo[], sessions: TrainingSessionInfo[], personaFilter?: string): Promise<void> {
    try {
      const usersResult = await DataList.execute<UserEntity>({
        collection: 'users', filter: { type: 'ai' }, limit: 50, dbHandle: 'default',
      });

      const users = (usersResult?.items ?? []).filter((u: UserEntity) => !personaFilter || u.id === personaFilter);

      // Parallel: load layers + sessions for all personas concurrently
      await Promise.all(users.map(async (user: UserEntity) => {
        const pName = user.uniqueId ?? user.displayName;

        try {
          const lr = await GenomeLayers.execute({ personaId: user.id, personaName: pName });
          for (const l of lr?.layers ?? []) {
            if (l.trainingMetrics) {
              adapters.push({
                id: user.id,
                name: l.name, domain: l.domain, baseModel: l.baseModel,
                personaName: pName, personaId: user.id, nodeName: 'local',
                finalLoss: l.trainingMetrics.finalLoss ?? 0,
                epochs: l.trainingMetrics.epochs ?? 0,
                examplesProcessed: l.trainingMetrics.examplesProcessed ?? 0,
                maturity: l.maturity ?? 0, sizeMB: l.sizeMB ?? 0,
                createdAt: l.createdAt ?? '',
                lossHistory: l.trainingMetrics.lossHistory ?? [],
                trainingDurationMs: l.trainingMetrics.trainingDurationMs ?? 0,
              });
            }
          }
        } catch { /* skip */ }

        try {
          const sr = await GenomeAcademySessionList.execute({ personaId: user.id });
          for (const s of sr?.sessions ?? []) {
            if (this.isZombie(s)) continue;
            sessions.push({ ...s, personaName: s.personaName ?? pName, nodeName: 'local' });
          }
        } catch { /* skip */ }
      }));
    } catch (err) {
      log.warn(`Local data load failed: ${err}`);
    }
  }

  private async loadGrid(adapters: TrainingAdapterInfo[], sessions: TrainingSessionInfo[], nodes: TrainingNodeInfo[], personaFilter?: string): Promise<void> {
    try {
      const rustClient = RustCoreIPCClient.getInstance();
      const gridNodesList: GridNode[] = await rustClient.gridNodes();

      for (const n of gridNodesList) {
        const nodeId = n.node_id;
        const nodeName = n.node_name ?? nodeId;
        const gpuCap = (n.capabilities ?? []).find((c: NodeCapability) => c.type === 'compute');
        let nodeAdapters = 0, nodeSessions = 0;

        try {
          const usersResult = await rustClient.gridSend(nodeId, 'user/list', { limit: 50 }) as GridUserListResponse;
          for (const user of usersResult?.users ?? []) {
            if (user.type === 'human') continue;
            if (personaFilter && user.id !== personaFilter) continue;
            const pName = user.uniqueId ?? user.displayName ?? user.id;

            try {
              const lr = await rustClient.gridSend(nodeId, 'genome/layers', { personaId: user.id, personaName: pName }) as GridGenomeLayersResponse;
              for (const l of lr?.layers ?? []) {
                if (l.trainingMetrics) {
                  adapters.push({
                    id: l.id ?? user.id, // layer UUID, fallback to persona UUID
                    name: l.name, domain: l.domain, baseModel: l.baseModel,
                    personaName: pName, personaId: user.id, nodeName,
                    finalLoss: l.trainingMetrics.finalLoss ?? 0,
                    epochs: l.trainingMetrics.epochs ?? 0,
                    examplesProcessed: l.trainingMetrics.examplesProcessed ?? 0,
                    maturity: l.maturity ?? 0, sizeMB: l.sizeMB ?? 0,
                    createdAt: l.createdAt ?? '',
                    lossHistory: l.trainingMetrics.lossHistory ?? [],
                    trainingDurationMs: l.trainingMetrics.trainRuntime ?? 0,
                  });
                  nodeAdapters++;
                }
              }
            } catch { /* skip persona */ }
          }

          try {
            const sr = await rustClient.gridSend(nodeId, 'genome/academy-session-list', {}) as GridAcademySessionListResponse;
            for (const s of sr?.sessions ?? []) {
              if (this.isZombie(s)) continue;
              sessions.push({ ...s, nodeName });
              nodeSessions++;
            }
          } catch { /* skip */ }
        } catch (err) {
          log.warn(`Grid node ${nodeName} failed: ${err}`);
        }

        const computeCap = gpuCap?.type === 'compute' ? gpuCap : undefined;
        nodes.push({ nodeId, nodeName, adapterCount: nodeAdapters, sessionCount: nodeSessions, gpu: computeCap?.gpu ?? undefined, vramMb: computeCap?.vram_mb });
      }
    } catch (err) {
      log.warn(`Grid load failed: ${err}`);
    }
  }

  private isZombie(session: AcademySessionSummary): boolean {
    const age = Date.now() - new Date(session.createdAt).getTime();
    return session.status === 'curriculum' && age > ZOMBIE_THRESHOLD_MS;
  }
}
