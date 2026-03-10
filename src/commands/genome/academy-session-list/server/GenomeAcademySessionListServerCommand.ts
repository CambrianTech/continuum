/**
 * Genome Academy Session List Command - Server Implementation
 *
 * Queries academy_sessions collection with optional filters for persona, status, and skill.
 * Returns sessions ordered by creation date (newest first).
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { DataList } from '@commands/data/list/shared/DataListTypes';
import { AcademySessionEntity } from '@system/genome/entities/AcademySessionEntity';
import type { AcademySessionMode, AcademySessionStatus } from '@system/genome/shared/AcademyTypes';
import type {
  GenomeAcademySessionListParams,
  GenomeAcademySessionListResult,
  AcademySessionSummary,
} from '../shared/GenomeAcademySessionListTypes';
import { createGenomeAcademySessionListResultFromParams } from '../shared/GenomeAcademySessionListTypes';

export class GenomeAcademySessionListServerCommand extends CommandBase<GenomeAcademySessionListParams, GenomeAcademySessionListResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/academy-session-list', context, subpath, commander);
  }

  async execute(params: GenomeAcademySessionListParams): Promise<GenomeAcademySessionListResult> {
    const filter: Record<string, unknown> = {};

    if (params.personaId) filter.personaId = params.personaId;
    if (params.status) filter.status = params.status;
    if (params.skill) filter.skill = params.skill;

    try {
      const result = await DataList.execute<AcademySessionEntity>({
        collection: AcademySessionEntity.collection,
        filter,
        orderBy: [{ field: 'createdAt', direction: 'desc' }],
        limit: params.limit ?? 50,
        dbHandle: 'default',
        skipCount: true,
      });

      if (!result.success) {
        return createGenomeAcademySessionListResultFromParams(params, {
          success: false,
          sessions: [],
          error: result.error ?? 'Failed to query academy sessions',
        });
      }

      const sessions: AcademySessionSummary[] = (result.items ?? []).map((record) => ({
        id: String(record.id ?? ''),
        personaId: String(record.personaId ?? ''),
        personaName: String(record.personaName ?? ''),
        skill: String(record.skill ?? ''),
        mode: ((record as Record<string, unknown>).mode ?? 'knowledge') as AcademySessionMode,
        status: (record.status ?? 'pending') as AcademySessionStatus,
        baseModel: String(record.baseModel ?? ''),
        createdAt: String(record.createdAt ?? ''),
        updatedAt: String(record.updatedAt ?? ''),
        teacherHandle: record.teacherHandle ? String(record.teacherHandle) : undefined,
        studentHandle: record.studentHandle ? String(record.studentHandle) : undefined,
        metrics: record.metrics as AcademySessionSummary['metrics'],
      }));

      return createGenomeAcademySessionListResultFromParams(params, {
        success: true,
        sessions,
      });

    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      return createGenomeAcademySessionListResultFromParams(params, {
        success: false,
        sessions: [],
        error: `Academy session list failed: ${message}`,
      });
    }
  }
}
