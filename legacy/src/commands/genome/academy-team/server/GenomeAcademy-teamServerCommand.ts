/**
 * Genome Academy Team Command - Server Implementation
 *
 * Creates a TeamProjectEntity and spawns:
 * - 1 Teacher Sentinel (TeamTeacherPipeline) — orchestrates planning, training, building, review
 * - N Student Sentinels (TeamStudentPipeline) — one per team member, scoped to their role
 *
 * Returns immediately with project ID and sentinel handles.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { GenomeAcademyTeamParams, GenomeAcademyTeamResult } from '../shared/GenomeAcademy-teamTypes';
import { createGenomeAcademyTeamResultFromParams } from '../shared/GenomeAcademy-teamTypes';
import { Commands } from '@system/core/shared/Commands';
import { TeamProjectEntity } from '@system/genome/entities/TeamProjectEntity';
import { DEFAULT_TEAM_PROJECT_CONFIG } from '@system/genome/shared/TeamProjectTypes';
import type { TeamMember, TeamProjectConfig } from '@system/genome/shared/TeamProjectTypes';
import { resolveTeacherLlmConfig } from '@system/genome/shared/AcademyTypes';
import { buildTeamTeacherPipeline } from '@system/sentinel/pipelines/TeamTeacherPipeline';
import { buildTeamStudentPipeline } from '@system/sentinel/pipelines/TeamStudentPipeline';
import { DataCreate } from '@commands/data/create/shared/DataCreateTypes';
import { LOCAL_MODELS } from '@system/shared/Constants';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { SentinelStep } from '@system/sentinel/SentinelDefinition';
import type { PipelineSentinelParams, SentinelRunResult } from '@commands/sentinel/run/shared/SentinelRunTypes';

export class GenomeAcademyTeamServerCommand extends CommandBase<GenomeAcademyTeamParams, GenomeAcademyTeamResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/academy-team', context, subpath, commander);
  }

  async execute(params: GenomeAcademyTeamParams): Promise<GenomeAcademyTeamResult> {
    const { projectDescription, skill } = params;
    const baseModel = params.baseModel ?? LOCAL_MODELS.DEFAULT;

    if (!projectDescription) {
      throw new ValidationError('projectDescription', 'Required: what should the team build?');
    }
    if (!skill) {
      throw new ValidationError('skill', 'Required: skill domain (e.g., "game-development")');
    }

    // Build config
    const config: TeamProjectConfig = {
      ...DEFAULT_TEAM_PROJECT_CONFIG,
      ...(params.model && { teacherModel: params.model }),
      ...(params.provider && { teacherProvider: params.provider }),
      ...(params.epochs !== undefined && { epochs: params.epochs }),
      ...(params.buildMilestones !== undefined && { buildMilestones: params.buildMilestones }),
    };

    // Validate teacher model upfront — throws if missing
    resolveTeacherLlmConfig(config);

    // Resolve team members
    const team = params.team as Array<{ personaId: string; personaName: string; role: string; roleDescription?: string }> | undefined;
    if (!team || team.length === 0) {
      throw new ValidationError('team', 'Required: at least 1 team member. Provide [{ personaId, personaName, role, roleDescription }].');
    }

    console.log(`🏗️ TEAM PROJECT: "${projectDescription}" (${skill})`);
    console.log(`   Team: ${team.map(m => `${m.personaName} (${m.role})`).join(', ')}`);

    // Build TeamMember array
    const members: TeamMember[] = team.map(m => ({
      personaId: m.personaId as UUID,
      personaName: m.personaName,
      role: m.role,
      roleDescription: m.roleDescription ?? m.role,
      studentHandle: '',
      sessionId: '' as UUID,
      status: 'pending' as const,
      roleScore: 0,
      projectContribution: '',
      topicsPassed: 0,
      averageScore: 0,
      layerIds: [],
    }));

    // Create TeamProjectEntity
    const entity = new TeamProjectEntity();
    entity.projectDescription = projectDescription;
    entity.skill = skill;
    entity.baseModel = baseModel;
    entity.status = 'pending';
    entity.members = members;
    entity.config = config;

    const validation = entity.validate();
    if (!validation.success) {
      throw new ValidationError('team', `Validation: ${validation.error}`);
    }

    const createResult = await DataCreate.execute({
      dbHandle: 'default',
      collection: TeamProjectEntity.collection,
      data: entity,
    });

    if (!createResult.success) {
      throw new Error(`Failed to create team project: ${createResult.error ?? 'unknown'}`);
    }

    const projectId = entity.id;
    console.log(`   Project created: ${projectId}`);

    // Spawn teacher sentinel
    const teacherPipeline = buildTeamTeacherPipeline({
      projectId,
      projectDescription,
      skill,
      members,
      config,
    });

    const teacherResult = await Commands.execute<PipelineSentinelParams, SentinelRunResult>('sentinel/run', {
      type: 'pipeline',
      definition: {
        type: 'pipeline',
        name: teacherPipeline.name ?? `team-teacher-${skill}`,
        description: `Team teacher for: ${projectDescription}`,
        version: '1.0',
        steps: teacherPipeline.steps as unknown as SentinelStep[],
        loop: { type: 'once' },
        tags: ['academy', 'team-teacher', skill],
      },
      sentinelName: teacherPipeline.name,
      timeout: 0, // No timeout — runs to completion
      userId: params.userId,
    });

    const teacherHandle: string = teacherResult.handle ?? '';
    console.log(`   Teacher sentinel: ${teacherHandle}`);

    // Spawn student sentinels (one per member)
    const memberHandles: Array<{ personaId: string; personaName: string; role: string; studentHandle: string }> = [];

    for (const member of members) {
      const studentPipeline = buildTeamStudentPipeline({
        projectId,
        personaId: member.personaId,
        personaName: member.personaName,
        role: member.role,
        roleDescription: member.roleDescription,
        baseModel,
        config,
      });

      const studentResult = await Commands.execute<PipelineSentinelParams, SentinelRunResult>('sentinel/run', {
        type: 'pipeline',
        definition: {
          type: 'pipeline',
          name: studentPipeline.name ?? `team-student-${member.personaName}-${member.role}`,
          description: `Team student ${member.personaName} (${member.role})`,
          version: '1.0',
          steps: studentPipeline.steps as unknown as SentinelStep[],
          loop: { type: 'once' },
          tags: ['academy', 'team-student', skill, member.role],
        },
        parentPersonaId: member.personaId,
        sentinelName: studentPipeline.name,
        timeout: 0,
        userId: params.userId,
      });

      const studentHandle: string = studentResult.handle ?? '';
      member.studentHandle = studentHandle;
      memberHandles.push({
        personaId: member.personaId,
        personaName: member.personaName,
        role: member.role,
        studentHandle,
      });

      console.log(`   Student sentinel (${member.personaName}/${member.role}): ${studentHandle}`);
    }

    console.log(`✅ TEAM PROJECT: ${members.length} students + 1 teacher running`);

    return createGenomeAcademyTeamResultFromParams(params, {
      success: true,
      teamProjectId: projectId,
      teacherHandle,
      memberHandles,
    });
  }
}
