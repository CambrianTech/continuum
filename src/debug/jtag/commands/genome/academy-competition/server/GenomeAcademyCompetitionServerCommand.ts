/**
 * Genome Academy Competition Command — Server Implementation
 *
 * Creates a CompetitionEntity and spawns:
 * - 1 Teacher Sentinel (shared curriculum, exams, grading)
 * - N Student Sentinels (one per competing persona)
 *
 * All students share the same curriculum and exam questions from the teacher.
 * Each student gets their own AcademySession for independent training/grading.
 * Rankings are computed from exam scores when all students complete.
 *
 * Returns immediately with competition ID and all sentinel handles.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type {
  GenomeAcademyCompetitionParams,
  GenomeAcademyCompetitionResult,
  CompetitorHandle,
} from '../shared/GenomeAcademyCompetitionTypes';
import { createGenomeAcademyCompetitionResultFromParams } from '../shared/GenomeAcademyCompetitionTypes';
import { CompetitionEntity } from '@system/genome/entities/CompetitionEntity';
import { AcademySessionEntity } from '@system/genome/entities/AcademySessionEntity';
import {
  DEFAULT_ACADEMY_CONFIG,
  type AcademyConfig,
} from '@system/genome/shared/AcademyTypes';
import {
  DEFAULT_COMPETITION_CONFIG,
  type CompetitionConfig,
  type CompetitorEntry,
} from '@system/genome/shared/CompetitionTypes';
import { buildTeacherPipeline } from '@system/sentinel/pipelines/TeacherPipeline';
import { buildStudentPipeline } from '@system/sentinel/pipelines/StudentPipeline';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { SentinelStep } from '@system/sentinel/SentinelDefinition';
import { DataCreate } from '@commands/data/create/shared/DataCreateTypes';
import { DataUpdate } from '@commands/data/update/shared/DataUpdateTypes';
import type { PipelineSentinelParams, SentinelRunResult } from '@commands/sentinel/run/shared/SentinelRunTypes';
import { Commands } from '@system/core/shared/Commands';

export class GenomeAcademyCompetitionServerCommand extends CommandBase<GenomeAcademyCompetitionParams, GenomeAcademyCompetitionResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/academy-competition', context, subpath, commander);
  }

  async execute(params: GenomeAcademyCompetitionParams): Promise<GenomeAcademyCompetitionResult> {
    const { skill, competitors } = params;
    const baseModel = params.baseModel ?? 'smollm2:135m';

    console.log(`\u{1F3C6} COMPETITION: skill="${skill}", competitors=${competitors?.length ?? 0}, model="${baseModel}"`);

    // --- Validation ---
    if (!skill?.trim()) {
      throw new ValidationError('skill', 'Missing required parameter. See genome/academy-competition README.');
    }
    if (!competitors || !Array.isArray(competitors) || competitors.length < 2) {
      throw new ValidationError('competitors', 'At least 2 competitors required. Each needs { personaId, personaName }.');
    }
    for (const c of competitors) {
      if (!c.personaId?.trim()) {
        throw new ValidationError('competitors[].personaId', `Missing personaId for competitor "${c.personaName ?? 'unknown'}"`);
      }
      if (!c.personaName?.trim()) {
        throw new ValidationError('competitors[].personaName', `Missing personaName for competitor with id "${c.personaId}"`);
      }
    }

    // Check for duplicate persona IDs
    const uniqueIds = new Set(competitors.map(c => c.personaId));
    if (uniqueIds.size !== competitors.length) {
      throw new ValidationError('competitors', 'Duplicate personaId found. Each competitor must be unique.');
    }

    // --- Build config ---
    const competitionConfig: CompetitionConfig = {
      ...DEFAULT_COMPETITION_CONFIG,
      ...(params.maxTopicAttempts !== undefined && { maxTopicAttempts: params.maxTopicAttempts }),
      ...(params.passingScore !== undefined && { passingScore: params.passingScore }),
      ...(params.epochs !== undefined && { epochs: params.epochs }),
      ...(params.rank !== undefined && { rank: params.rank }),
      ...(params.tournamentRounds !== undefined && { tournamentRounds: params.tournamentRounds }),
      ...(params.model && { teacherModel: params.model }),
      ...(params.provider && { teacherProvider: params.provider }),
    };

    const academyConfig: AcademyConfig = {
      ...DEFAULT_ACADEMY_CONFIG,
      maxTopicAttempts: competitionConfig.maxTopicAttempts,
      passingScore: competitionConfig.passingScore,
      epochs: competitionConfig.epochs,
      rank: competitionConfig.rank,
      ...(params.model && { teacherModel: params.model }),
      ...(params.provider && { teacherProvider: params.provider }),
    };

    // --- 1. Create CompetitionEntity ---
    const entity = new CompetitionEntity();
    entity.skill = skill;
    entity.baseModel = baseModel;
    entity.status = 'pending';
    entity.config = competitionConfig;
    entity.competitors = competitors.map(c => ({
      personaId: c.personaId,
      personaName: c.personaName,
      studentHandle: '',
      sessionId: '' as UUID,
      topicScores: [],
      topicsPassed: 0,
      totalAttempts: 0,
      averageScore: 0,
      rank: 0,
      totalTrainingTimeMs: 0,
      layerIds: [],
    }));

    const validation = entity.validate();
    if (!validation.success) {
      return createGenomeAcademyCompetitionResultFromParams(params, {
        success: false,
        error: `Entity validation failed: ${validation.error}`,
        competitionId: '' as UUID,
        teacherHandle: '',
        competitorHandles: [],
      });
    }

    const createResult = await DataCreate.execute({
      collection: CompetitionEntity.collection,
      data: entity,
    });

    if (!createResult.success) {
      return createGenomeAcademyCompetitionResultFromParams(params, {
        success: false,
        error: `Failed to create competition entity: ${createResult.error ?? 'unknown'}`,
        competitionId: '' as UUID,
        teacherHandle: '',
        competitorHandles: [],
      });
    }

    const competitionId = entity.id;
    console.log(`   Competition created: ${competitionId}`);

    // --- 2. Create AcademySession per competitor ---
    // Each competitor gets their own session so the student pipeline
    // can track per-persona training independently.
    const competitorHandles: CompetitorHandle[] = [];
    const updatedCompetitors: CompetitorEntry[] = [];

    // Use the first competitor's session for the shared teacher pipeline
    // (teacher events are scoped by sessionId — all students share it)
    const sharedSessionId = entity.id; // Use competition ID as shared session scope

    // --- 3. Build and submit shared teacher sentinel ---
    const teacherPipeline = buildTeacherPipeline({
      sessionId: sharedSessionId,
      skill,
      personaName: `competition-${competitors.length}-personas`,
      baseModel,
      config: academyConfig,
    });

    // PipelineStep[] (Rust bindings) → SentinelStep[] (TS definitions) — structurally compatible wire types
    const teacherSteps = teacherPipeline.steps as unknown as SentinelStep[];

    const teacherResult = await Commands.execute<PipelineSentinelParams, SentinelRunResult>('sentinel/run', {
      type: 'pipeline',
      definition: {
        type: 'pipeline',
        name: `competition-teacher-${skill}`,
        description: `Shared teacher sentinel for competition: ${skill} (${competitors.length} competitors)`,
        version: '1.0',
        steps: teacherSteps,
        loop: { type: 'once' },
        tags: ['competition', 'teacher', skill],
      },
      sentinelName: `competition-teacher-${skill}`,
    });

    const teacherHandle = teacherResult.handle ?? '';
    console.log(`   Teacher sentinel started: ${teacherHandle}`);

    // --- 4. Build and submit student sentinels (one per competitor) ---
    for (const competitor of competitors) {
      // Create per-competitor academy session
      const sessionEntity = new AcademySessionEntity();
      sessionEntity.personaId = competitor.personaId;
      sessionEntity.personaName = competitor.personaName;
      sessionEntity.skill = skill;
      sessionEntity.baseModel = baseModel;
      sessionEntity.status = 'pending';
      sessionEntity.currentTopic = 0;
      sessionEntity.examRounds = 0;
      sessionEntity.config = academyConfig;

      await DataCreate.execute({
        collection: AcademySessionEntity.collection,
        data: sessionEntity,
      });

      const studentSessionId = sessionEntity.id;

      // Build student pipeline scoped to the SHARED session ID
      // so it watches the same teacher events
      const studentPipeline = buildStudentPipeline({
        sessionId: sharedSessionId,
        personaId: competitor.personaId,
        personaName: competitor.personaName,
        baseModel,
        config: academyConfig,
      });

      const studentSteps = studentPipeline.steps as unknown as SentinelStep[];

      const studentResult = await Commands.execute<PipelineSentinelParams, SentinelRunResult>('sentinel/run', {
        type: 'pipeline',
        definition: {
          type: 'pipeline',
          name: `competition-student-${skill}-${competitor.personaName}`,
          description: `Student sentinel for ${competitor.personaName} in competition: ${skill}`,
          version: '1.0',
          steps: studentSteps,
          loop: { type: 'once' },
          tags: ['competition', 'student', skill, competitor.personaName],
        },
        parentPersonaId: competitor.personaId,
        sentinelName: `competition-student-${skill}-${competitor.personaName}`,
      });

      const studentHandle = studentResult.handle ?? '';
      console.log(`   Student sentinel started for ${competitor.personaName}: ${studentHandle}`);

      // Update session with handle
      await DataUpdate.execute({
        collection: AcademySessionEntity.collection,
        id: studentSessionId,
        data: { studentHandle, status: 'curriculum' },
      });

      competitorHandles.push({
        personaId: competitor.personaId,
        personaName: competitor.personaName,
        studentHandle,
        sessionId: studentSessionId,
      });

      updatedCompetitors.push({
        personaId: competitor.personaId,
        personaName: competitor.personaName,
        studentHandle,
        sessionId: studentSessionId,
        topicScores: [],
        topicsPassed: 0,
        totalAttempts: 0,
        averageScore: 0,
        rank: 0,
        totalTrainingTimeMs: 0,
        layerIds: [],
      });
    }

    // --- 5. Update competition entity with handles ---
    await DataUpdate.execute({
      collection: CompetitionEntity.collection,
      id: competitionId as UUID,
      data: {
        teacherHandle,
        status: 'curriculum',
        competitors: updatedCompetitors,
        currentRound: 1,
        startedAt: new Date().toISOString(),
      },
    });

    console.log(`\u{2705} COMPETITION: ${competitors.length} students competing on "${skill}"`);

    return createGenomeAcademyCompetitionResultFromParams(params, {
      success: true,
      competitionId: competitionId as UUID,
      teacherHandle,
      competitorHandles,
    });
  }
}
