/**
 * Genome Academy Session Detail Command - Server Implementation
 *
 * Fetches a single AcademySession by ID, then joins its curricula and
 * examination records to build a comprehensive detail view.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import { DataRead } from '@commands/data/read/shared/DataReadTypes';
import { DataList } from '@commands/data/list/shared/DataListTypes';
import { AcademySessionEntity } from '@system/genome/entities/AcademySessionEntity';
import { AcademyCurriculumEntity } from '@system/genome/entities/AcademyCurriculumEntity';
import { AcademyExaminationEntity } from '@system/genome/entities/AcademyExaminationEntity';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type {
  GenomeAcademySessionDetailParams,
  GenomeAcademySessionDetailResult,
  AcademySessionDetail,
  CurriculumTopicSummary,
  ExaminationResult,
} from '../shared/GenomeAcademySessionDetailTypes';
import { createGenomeAcademySessionDetailResultFromParams } from '../shared/GenomeAcademySessionDetailTypes';

export class GenomeAcademySessionDetailServerCommand extends CommandBase<
  GenomeAcademySessionDetailParams,
  GenomeAcademySessionDetailResult
> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/academy-session-detail', context, subpath, commander);
  }

  async execute(params: GenomeAcademySessionDetailParams): Promise<GenomeAcademySessionDetailResult> {
    const { sessionId } = params;

    if (!sessionId) {
      throw new ValidationError('sessionId', 'Missing required parameter. Provide the Academy session ID.');
    }

    // 1. Fetch the session entity
    const sessionResult = await DataRead.execute<AcademySessionEntity>({
      collection: AcademySessionEntity.collection,
      id: sessionId as UUID,
      dbHandle: 'default',
    });

    if (!sessionResult.success || !sessionResult.data) {
      return createGenomeAcademySessionDetailResultFromParams(params, {
        success: false,
        error: sessionResult.found === false
          ? `Academy session not found: ${sessionId}`
          : `Failed to read academy session: ${sessionResult.error ?? 'unknown'}`,
        curricula: [],
        examinations: [],
        adapterIds: [],
      });
    }

    const entity = sessionResult.data;

    // 2. Fetch curricula and examinations in parallel
    const [curriculaResult, examsResult] = await Promise.all([
      DataList.execute<AcademyCurriculumEntity>({
        collection: AcademyCurriculumEntity.collection,
        filter: { sessionId },
        dbHandle: 'default',
        skipCount: true,
      }),
      DataList.execute<AcademyExaminationEntity>({
        collection: AcademyExaminationEntity.collection,
        filter: { sessionId },
        orderBy: [{ field: 'topicIndex', direction: 'asc' }, { field: 'round', direction: 'asc' }],
        dbHandle: 'default',
        skipCount: true,
      }),
    ]);

    // 3. Build examination result summaries
    const examinations: ExaminationResult[] = (examsResult.items ?? []).map((exam) => ({
      topicIndex: exam.topicIndex,
      round: exam.round,
      score: exam.overallScore,
      passed: exam.passed,
    }));

    // 4. Build curriculum topic summaries with exam scores joined
    const curricula: CurriculumTopicSummary[] = [];
    const curriculumEntity = (curriculaResult.items ?? [])[0];
    if (curriculumEntity?.topics) {
      for (let i = 0; i < curriculumEntity.topics.length; i++) {
        const topic = curriculumEntity.topics[i];
        const topicExams = examinations.filter((e) => e.topicIndex === i);
        const passed = topicExams.some((e) => e.passed);
        curricula.push({
          name: topic.name,
          passed,
          examScores: topicExams.map((e) => e.score),
        });
      }
    }

    // 5. Collect adapter/layer IDs from metrics
    const adapterIds: string[] = (entity.metrics?.layerIds ?? []).map(String);

    // 6. Build the session detail
    // Mode is not persisted on the entity; infer from teacher handle naming convention
    // e.g. "academy-coding-teacher-*" → "coding", "academy-teacher-*" → "knowledge"
    const mode = this.inferModeFromHandle(entity.teacherHandle);

    const session: AcademySessionDetail = {
      id: entity.id,
      personaId: entity.personaId,
      personaName: entity.personaName,
      skill: entity.skill,
      mode,
      status: entity.status,
      baseModel: entity.baseModel,
      createdAt: (entity as Record<string, unknown>).createdAt as string ?? '',
      updatedAt: (entity as Record<string, unknown>).updatedAt as string ?? '',
      teacherHandle: entity.teacherHandle,
      studentHandle: entity.studentHandle,
      config: entity.config,
      metrics: entity.metrics,
    };

    return createGenomeAcademySessionDetailResultFromParams(params, {
      success: true,
      session,
      curricula,
      examinations,
      adapterIds,
    });
  }

  /**
   * Infer the Academy mode from the teacher sentinel handle name.
   * Handle format: "academy-{modePrefix}teacher-{skill}"
   * Mode prefixes: coding-, project-, realclasseval-, recipe-, (empty for knowledge)
   */
  private inferModeFromHandle(handle?: string): string {
    if (!handle) return 'unknown';
    if (handle.includes('realclasseval-teacher')) return 'realclasseval';
    if (handle.includes('coding-teacher')) return 'coding';
    if (handle.includes('project-teacher')) return 'project';
    if (handle.includes('recipe-teacher')) return 'recipe';
    if (handle.includes('academy-teacher')) return 'knowledge';
    return 'unknown';
  }
}
