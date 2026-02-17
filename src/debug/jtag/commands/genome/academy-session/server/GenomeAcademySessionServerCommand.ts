/**
 * Genome Academy Session Command - Server Implementation
 *
 * Creates an AcademySessionEntity and spawns dual sentinels:
 * - Teacher Sentinel: designs curriculum, synthesizes training data, generates exams, grades
 * - Student Sentinel: trains on data, takes exams, proves mastery
 *
 * Returns immediately with session ID and sentinel handles.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { GenomeAcademySessionParams, GenomeAcademySessionResult } from '../shared/GenomeAcademySessionTypes';
import { createGenomeAcademySessionResultFromParams } from '../shared/GenomeAcademySessionTypes';
import { Commands } from '@system/core/shared/Commands';
import { AcademySessionEntity } from '@system/genome/entities/AcademySessionEntity';
import { DEFAULT_ACADEMY_CONFIG } from '@system/genome/shared/AcademyTypes';
import type { AcademyConfig } from '@system/genome/shared/AcademyTypes';
import { buildTeacherPipeline } from '@system/sentinel/pipelines/TeacherPipeline';
import { buildStudentPipeline } from '@system/sentinel/pipelines/StudentPipeline';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { SentinelStep } from '@system/sentinel/SentinelDefinition';
import { DataCreate } from '@commands/data/create/shared/DataCreateTypes';
import { DataUpdate } from '@commands/data/update/shared/DataUpdateTypes';
import type { PipelineSentinelParams, SentinelRunResult } from '@commands/sentinel/run/shared/SentinelRunTypes';

export class GenomeAcademySessionServerCommand extends CommandBase<GenomeAcademySessionParams, GenomeAcademySessionResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/academy-session', context, subpath, commander);
  }

  async execute(params: GenomeAcademySessionParams): Promise<GenomeAcademySessionResult> {
    const { personaId, personaName, skill } = params;
    const baseModel = params.baseModel ?? 'smollm2:135m';

    console.log(`🎓 ACADEMY SESSION: persona="${personaName}", skill="${skill}", model="${baseModel}"`);

    if (!personaId) {
      throw new ValidationError('personaId', 'Missing required parameter. See genome/academy-session README.');
    }
    if (!personaName) {
      throw new ValidationError('personaName', 'Missing required parameter. See genome/academy-session README.');
    }
    if (!skill) {
      throw new ValidationError('skill', 'Missing required parameter. See genome/academy-session README.');
    }

    // Build config from params + defaults
    const config: AcademyConfig = {
      ...DEFAULT_ACADEMY_CONFIG,
      ...(params.maxTopicAttempts !== undefined && { maxTopicAttempts: params.maxTopicAttempts }),
      ...(params.passingScore !== undefined && { passingScore: params.passingScore }),
      ...(params.epochs !== undefined && { epochs: params.epochs }),
      ...(params.rank !== undefined && { rank: params.rank }),
      ...(params.model && { teacherModel: params.model }),
      ...(params.provider && { teacherProvider: params.provider }),
    };

    // 1. Create AcademySessionEntity (instantiate for auto-generated id)
    const entity = new AcademySessionEntity();
    entity.personaId = personaId;
    entity.personaName = personaName;
    entity.skill = skill;
    entity.baseModel = baseModel;
    entity.status = 'pending';
    entity.currentTopic = 0;
    entity.examRounds = 0;
    entity.config = config;

    const validation = entity.validate();
    if (!validation.success) {
      return createGenomeAcademySessionResultFromParams(params, {
        success: false,
        error: `Entity validation failed: ${validation.error}`,
        academySessionId: '' as UUID,
        teacherHandle: '',
        studentHandle: '',
      });
    }

    const createResult = await DataCreate.execute({
      collection: AcademySessionEntity.collection,
      data: entity,
    });

    if (!createResult.success) {
      return createGenomeAcademySessionResultFromParams(params, {
        success: false,
        error: `Failed to create academy session entity: ${createResult.error ?? 'unknown'}`,
        academySessionId: '' as UUID,
        teacherHandle: '',
        studentHandle: '',
      });
    }

    const sessionId = entity.id;
    console.log(`   Session created: ${sessionId}`);

    // 2. Build teacher pipeline
    const teacherPipeline = buildTeacherPipeline({
      sessionId,
      skill,
      personaName,
      baseModel,
      config,
    });

    // 3. Build student pipeline
    const studentPipeline = buildStudentPipeline({
      sessionId,
      personaId,
      personaName,
      baseModel,
      config,
    });

    // 4. Submit teacher sentinel
    // PipelineStep[] (Rust bindings) → SentinelStep[] (TS definitions) — structurally compatible wire types
    const teacherSteps = teacherPipeline.steps as unknown as SentinelStep[];

    const teacherResult = await Commands.execute<PipelineSentinelParams, SentinelRunResult>('sentinel/run', {
      type: 'pipeline',
      definition: {
        type: 'pipeline',
        name: `academy-teacher-${skill}`,
        description: `Teacher sentinel for Academy session: ${skill}`,
        version: '1.0',
        steps: teacherSteps,
        loop: { type: 'once' },
        tags: ['academy', 'teacher', skill],
      },
      parentPersonaId: personaId,
      sentinelName: `academy-teacher-${skill}`,
    });

    const teacherHandle = teacherResult.handle ?? '';
    console.log(`   Teacher sentinel started: ${teacherHandle}`);

    // 5. Submit student sentinel
    const studentSteps = studentPipeline.steps as unknown as SentinelStep[];

    const studentResult = await Commands.execute<PipelineSentinelParams, SentinelRunResult>('sentinel/run', {
      type: 'pipeline',
      definition: {
        type: 'pipeline',
        name: `academy-student-${skill}`,
        description: `Student sentinel for Academy session: ${skill} (persona: ${personaName})`,
        version: '1.0',
        steps: studentSteps,
        loop: { type: 'once' },
        tags: ['academy', 'student', skill],
      },
      parentPersonaId: personaId,
      sentinelName: `academy-student-${skill}`,
    });

    const studentHandle = studentResult.handle ?? '';
    console.log(`   Student sentinel started: ${studentHandle}`);

    // 6. Update session with handles
    await DataUpdate.execute({
      collection: AcademySessionEntity.collection,
      id: sessionId,
      data: {
        teacherHandle,
        studentHandle,
        status: 'curriculum',
      },
    });

    console.log(`✅ ACADEMY SESSION: Both sentinels running for "${skill}"`);

    return createGenomeAcademySessionResultFromParams(params, {
      success: true,
      academySessionId: sessionId,
      teacherHandle,
      studentHandle,
    });
  }
}
