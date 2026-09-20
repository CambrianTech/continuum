/**
 * Genome Academy Session Restart Command - Server Implementation
 *
 * Reads an existing AcademySessionEntity, clones its configuration,
 * and delegates to genome/academy-session to create a fresh session
 * with new teacher/student sentinels.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import { Commands } from '@system/core/shared/Commands';
import { DataRead } from '@commands/data/read/shared/DataReadTypes';
import { AcademySessionEntity } from '@system/genome/entities/AcademySessionEntity';
import type { GenomeAcademySessionParams, GenomeAcademySessionResult } from '@commands/genome/academy-session/shared/GenomeAcademySessionTypes';
import type { GenomeAcademySessionRestartParams, GenomeAcademySessionRestartResult } from '../shared/GenomeAcademySessionRestartTypes';
import { createGenomeAcademySessionRestartResultFromParams } from '../shared/GenomeAcademySessionRestartTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export class GenomeAcademySessionRestartServerCommand extends CommandBase<GenomeAcademySessionRestartParams, GenomeAcademySessionRestartResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/academy-session-restart', context, subpath, commander);
  }

  async execute(params: GenomeAcademySessionRestartParams): Promise<GenomeAcademySessionRestartResult> {
    if (!params.sessionId) {
      throw new ValidationError('sessionId', 'Missing required parameter: the original session ID to restart from.');
    }

    // 1. Read the original session
    const readResult = await DataRead.execute<AcademySessionEntity>({
      collection: AcademySessionEntity.collection,
      id: params.sessionId as UUID,
      dbHandle: 'default',
    });

    if (!readResult.success || !readResult.found || !readResult.data) {
      throw new ValidationError('sessionId', `Academy session not found: ${params.sessionId}`);
    }

    const original = readResult.data;
    console.log(`🔄 ACADEMY RESTART: Cloning session ${params.sessionId} (skill="${original.skill}", mode="${original.config.teacherProvider ?? 'knowledge'}")`);

    // 2. Build params for a fresh session from the original's config
    const config = original.config;
    const newSessionParams: Partial<GenomeAcademySessionParams> = {
      personaId: original.personaId,
      personaName: original.personaName,
      skill: original.skill,
      baseModel: original.baseModel,
      maxTopicAttempts: config.maxTopicAttempts,
      passingScore: config.passingScore,
      epochs: config.epochs,
      rank: config.rank,
      questionsPerExam: config.questionsPerExam,
      examplesPerTopic: config.examplesPerTopic,
      topicsPerSession: config.topicsPerSession,
      learningRate: config.learningRate,
      batchSize: config.batchSize,
      model: config.teacherModel,
      provider: config.teacherProvider,
      studentModel: config.studentModel,
      studentProvider: config.studentProvider,
      userId: params.userId,
    };

    // 3. Delegate to genome/academy-session to create the new session
    const result = await Commands.execute<GenomeAcademySessionParams, GenomeAcademySessionResult>(
      'genome/academy-session',
      newSessionParams,
    );

    if (!result.success) {
      return createGenomeAcademySessionRestartResultFromParams(params, {
        success: false,
        newSessionId: '',
        teacherHandle: '',
        studentHandle: '',
        error: result.error ?? 'Failed to create new academy session',
      });
    }

    console.log(`✅ ACADEMY RESTART: New session ${result.academySessionId} (teacher=${result.teacherHandle}, student=${result.studentHandle})`);

    return createGenomeAcademySessionRestartResultFromParams(params, {
      success: true,
      newSessionId: result.academySessionId,
      teacherHandle: result.teacherHandle,
      studentHandle: result.studentHandle,
    });
  }
}
