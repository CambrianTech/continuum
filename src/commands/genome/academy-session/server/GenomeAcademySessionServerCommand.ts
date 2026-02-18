/**
 * Genome Academy Session Command - Server Implementation
 *
 * Creates an AcademySessionEntity and spawns dual sentinels:
 * - Teacher Sentinel: designs curriculum, synthesizes training data, generates exams, grades
 * - Student Sentinel: trains on data, takes exams, proves mastery
 *
 * Returns immediately with session ID and sentinel handles.
 */

import * as fs from 'fs';
import * as path from 'path';
import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { GenomeAcademySessionParams, GenomeAcademySessionResult } from '../shared/GenomeAcademySessionTypes';
import { createGenomeAcademySessionResultFromParams } from '../shared/GenomeAcademySessionTypes';
import { Commands } from '@system/core/shared/Commands';
import { AcademySessionEntity } from '@system/genome/entities/AcademySessionEntity';
import { DEFAULT_ACADEMY_CONFIG } from '@system/genome/shared/AcademyTypes';
import type { AcademyConfig, ProjectSpec } from '@system/genome/shared/AcademyTypes';
import { buildTeacherPipeline } from '@system/sentinel/pipelines/TeacherPipeline';
import { buildStudentPipeline } from '@system/sentinel/pipelines/StudentPipeline';
import { buildCodingTeacherPipeline } from '@system/sentinel/pipelines/CodingTeacherPipeline';
import { buildCodingStudentPipeline } from '@system/sentinel/pipelines/CodingStudentPipeline';
import { buildProjectTeacherPipeline } from '@system/sentinel/pipelines/ProjectTeacherPipeline';
import { buildProjectStudentPipeline } from '@system/sentinel/pipelines/ProjectStudentPipeline';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { SentinelStep } from '@system/sentinel/SentinelDefinition';
import { DataCreate } from '@commands/data/create/shared/DataCreateTypes';
import { DataUpdate } from '@commands/data/update/shared/DataUpdateTypes';
import type { PipelineSentinelParams, SentinelRunResult } from '@commands/sentinel/run/shared/SentinelRunTypes';
import { LOCAL_MODELS } from '@system/shared/Constants';

export class GenomeAcademySessionServerCommand extends CommandBase<GenomeAcademySessionParams, GenomeAcademySessionResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/academy-session', context, subpath, commander);
  }

  async execute(params: GenomeAcademySessionParams): Promise<GenomeAcademySessionResult> {
    const { personaId, personaName, skill } = params;
    const mode = params.mode ?? 'knowledge';
    const baseModel = params.baseModel ?? LOCAL_MODELS.DEFAULT;

    console.log(`🎓 ACADEMY SESSION [${mode}]: persona="${personaName}", skill="${skill}", model="${baseModel}"`);

    if (!personaId) {
      throw new ValidationError('personaId', 'Missing required parameter. See genome/academy-session README.');
    }
    if (!personaName) {
      throw new ValidationError('personaName', 'Missing required parameter. See genome/academy-session README.');
    }
    if (!skill) {
      throw new ValidationError('skill', 'Missing required parameter. See genome/academy-session README.');
    }

    // Coding mode requires challenge params
    if (mode === 'coding') {
      if (!params.challengeDir) {
        throw new ValidationError('challengeDir', 'Required for coding mode. Path to challenge directory.');
      }
      if (!params.sourceFile) {
        throw new ValidationError('sourceFile', 'Required for coding mode. Buggy source file (relative to challengeDir).');
      }
      if (!params.testFile) {
        throw new ValidationError('testFile', 'Required for coding mode. Test file (relative to challengeDir).');
      }
    }

    // Project mode requires projectDir
    if (mode === 'project') {
      if (!params.projectDir) {
        throw new ValidationError('projectDir', 'Required for project mode. Path to project directory containing project.json.');
      }
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

    // 2. Build pipelines based on mode
    let pipelineResult: { teacherPipeline: ReturnType<typeof buildTeacherPipeline>; studentPipeline: ReturnType<typeof buildStudentPipeline> };
    if (mode === 'project') {
      pipelineResult = this.buildProjectPipelines(sessionId, personaId, personaName, skill, baseModel, config, params);
    } else if (mode === 'coding') {
      pipelineResult = this.buildCodingPipelines(sessionId, personaId, personaName, skill, baseModel, config, params);
    } else {
      pipelineResult = this.buildKnowledgePipelines(sessionId, personaId, personaName, skill, baseModel, config);
    }
    const { teacherPipeline, studentPipeline } = pipelineResult;

    // 3. Submit teacher sentinel
    // PipelineStep[] (Rust bindings) → SentinelStep[] (TS definitions) — structurally compatible wire types
    const teacherSteps = teacherPipeline.steps as unknown as SentinelStep[];
    const modePrefixMap = { knowledge: '', coding: 'coding-', project: 'project-' } as const;
    const modePrefix = modePrefixMap[mode];
    const modeLabel = mode === 'project' ? 'Project' : mode === 'coding' ? 'Coding' : 'Knowledge';
    const teacherName = teacherPipeline.name ?? `academy-${modePrefix}teacher-${skill}`;
    const studentName = studentPipeline.name ?? `academy-${modePrefix}student-${skill}`;

    const teacherResult = await Commands.execute<PipelineSentinelParams, SentinelRunResult>('sentinel/run', {
      type: 'pipeline',
      definition: {
        type: 'pipeline',
        name: teacherName,
        description: `${modeLabel} teacher sentinel for Academy session: ${skill}`,
        version: '1.0',
        steps: teacherSteps,
        loop: { type: 'once' },
        tags: ['academy', `${modePrefix}teacher`, skill],
      },
      parentPersonaId: personaId,
      sentinelName: teacherName,
    });

    const teacherHandle = teacherResult.handle ?? '';
    console.log(`   Teacher sentinel started: ${teacherHandle}`);

    // 4. Submit student sentinel
    const studentSteps = studentPipeline.steps as unknown as SentinelStep[];

    const studentResult = await Commands.execute<PipelineSentinelParams, SentinelRunResult>('sentinel/run', {
      type: 'pipeline',
      definition: {
        type: 'pipeline',
        name: studentName,
        description: `${modeLabel} student sentinel for Academy session: ${skill} (persona: ${personaName})`,
        version: '1.0',
        steps: studentSteps,
        loop: { type: 'once' },
        tags: ['academy', `${modePrefix}student`, skill],
      },
      parentPersonaId: personaId,
      sentinelName: studentName,
    });

    const studentHandle = studentResult.handle ?? '';
    console.log(`   Student sentinel started: ${studentHandle}`);

    // 5. Update session with handles
    await DataUpdate.execute({
      collection: AcademySessionEntity.collection,
      id: sessionId,
      data: {
        teacherHandle,
        studentHandle,
        status: 'curriculum',
      },
    });

    console.log(`✅ ACADEMY SESSION [${mode}]: Both sentinels running for "${skill}"`);

    return createGenomeAcademySessionResultFromParams(params, {
      success: true,
      academySessionId: sessionId,
      teacherHandle,
      studentHandle,
    });
  }

  /**
   * Build knowledge-mode pipelines (exam-based teacher/student).
   * This is the original Academy behavior.
   */
  private buildKnowledgePipelines(
    sessionId: UUID,
    personaId: UUID,
    personaName: string,
    skill: string,
    baseModel: string,
    config: AcademyConfig,
  ) {
    const teacherPipeline = buildTeacherPipeline({
      sessionId,
      skill,
      personaName,
      baseModel,
      config,
    });

    const studentPipeline = buildStudentPipeline({
      sessionId,
      personaId,
      personaName,
      baseModel,
      config,
    });

    return { teacherPipeline, studentPipeline };
  }

  /**
   * Build coding-mode pipelines (test-suite-based teacher/student).
   * Teacher analyzes bugs + synthesizes training data.
   * Student trains LoRA + attempts code fixes scored by real tests.
   */
  private buildCodingPipelines(
    sessionId: UUID,
    personaId: UUID,
    personaName: string,
    skill: string,
    baseModel: string,
    config: AcademyConfig,
    params: GenomeAcademySessionParams,
  ) {
    const teacherPipeline = buildCodingTeacherPipeline({
      sessionId,
      skill,
      personaName,
      baseModel,
      challengeDir: params.challengeDir!,
      sourceFile: params.sourceFile!,
      testFile: params.testFile!,
      testCommand: params.testCommand,
      config,
    });

    const studentPipeline = buildCodingStudentPipeline({
      sessionId,
      personaId,
      personaName,
      baseModel,
      challengeDir: params.challengeDir!,
      sourceFile: params.sourceFile!,
      testFile: params.testFile!,
      testCommand: params.testCommand,
      config,
    });

    return { teacherPipeline, studentPipeline };
  }

  /**
   * Build project-mode pipelines (multi-milestone project teacher/student).
   * Teacher reads project.json, scaffolds working dir, orchestrates cold→train→warm per milestone.
   * Student builds cumulative code across milestones, trains LoRA on gap-targeted data.
   */
  private buildProjectPipelines(
    sessionId: UUID,
    personaId: UUID,
    personaName: string,
    skill: string,
    baseModel: string,
    config: AcademyConfig,
    params: GenomeAcademySessionParams,
  ) {
    const projectDir = params.projectDir!;
    const projectJsonPath = path.join(projectDir, 'project.json');
    const projectSpec: ProjectSpec = JSON.parse(fs.readFileSync(projectJsonPath, 'utf8'));

    console.log(`   Project: ${projectSpec.name} (${projectSpec.milestones.length} milestones)`);

    const teacherPipeline = buildProjectTeacherPipeline({
      sessionId,
      skill,
      personaName,
      baseModel,
      projectDir,
      milestones: projectSpec.milestones,
      config,
    });

    const studentPipeline = buildProjectStudentPipeline({
      sessionId,
      personaId,
      personaName,
      baseModel,
      projectDir,
      milestones: projectSpec.milestones,
      config,
    });

    return { teacherPipeline, studentPipeline };
  }
}
