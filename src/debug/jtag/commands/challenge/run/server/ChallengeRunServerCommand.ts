/**
 * Challenge Run Command - Server Implementation
 *
 * Runs a coding challenge:
 * 1. Loads challenge (by ID, sequence number, or next unbeaten)
 * 2. Sets up fresh workspace with challenge files
 * 3. Executes via CodingChallengeRunner → CodeAgentOrchestrator
 * 4. Evaluates via CodingJudge
 * 5. Records attempt and returns results
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { ChallengeRunParams, ChallengeRunResult } from '../shared/ChallengeRunTypes';
import { createChallengeRunResultFromParams } from '../shared/ChallengeRunTypes';
import { CodingChallengeRunner } from '@system/code/server/CodingChallengeRunner';
import { CodingChallengeEntity } from '@system/data/entities/CodingChallengeEntity';
import { ALL_CHALLENGES } from '@system/code/challenges/ChallengeDefinitions';
import { Commands } from '@system/core/shared/Commands';
import { COLLECTIONS } from '@system/shared/Constants';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export class ChallengeRunServerCommand extends CommandBase<ChallengeRunParams, ChallengeRunResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('challenge/run', context, subpath, commander);
  }

  async execute(params: ChallengeRunParams): Promise<ChallengeRunResult> {
    const personaId = (params.personaId ?? params.userId) as UUID;
    if (!personaId) {
      throw new ValidationError('personaId', 'A persona ID is required to run a challenge.');
    }

    // Load or create the challenge entity
    const challenge = await this.resolveChallenge(params, personaId);

    // Run the challenge
    const runner = new CodingChallengeRunner();
    const result = await runner.run(challenge, {
      personaId,
      skipJudge: params.skipJudge ?? false,
    });

    // Persist updated challenge (with new attempt recorded)
    await this.persistChallenge(challenge);

    return createChallengeRunResultFromParams(params, {
      success: result.success,
      challengeName: challenge.name,
      difficulty: challenge.difficulty,
      status: result.attempt.status,
      score: result.attempt.score,
      feedback: result.attempt.feedback,
      durationMs: result.attempt.durationMs,
      toolCallsUsed: result.attempt.toolCallsUsed,
      filesModified: result.attempt.filesModified,
      filesCreated: result.attempt.filesCreated,
      errors: result.attempt.errors,
    });
  }

  /**
   * Resolve which challenge to run:
   * 1. By challengeId (exact match)
   * 2. By challengeNumber (sequence number)
   * 3. Next unbeaten challenge for this persona
   */
  private async resolveChallenge(params: ChallengeRunParams, personaId: UUID): Promise<CodingChallengeEntity> {
    // Try loading from database first
    if (params.challengeId) {
      return await this.loadOrCreateChallenge(params.challengeId);
    }

    if (params.challengeNumber) {
      const def = ALL_CHALLENGES.find(c => c.sequenceNumber === params.challengeNumber);
      if (!def) {
        throw new ValidationError(
          'challengeNumber',
          `No challenge with sequence number ${params.challengeNumber}. Valid: 1-${ALL_CHALLENGES.length}`,
        );
      }
      return await this.ensureChallengeEntity(def);
    }

    // Find next unbeaten challenge
    for (const def of ALL_CHALLENGES) {
      const entity = await this.ensureChallengeEntity(def);
      const best = entity.bestAttemptFor(personaId);
      if (!best || best.status !== 'passed') {
        return entity;
      }
    }

    // All beaten — run the hardest one again
    return await this.ensureChallengeEntity(ALL_CHALLENGES[ALL_CHALLENGES.length - 1]);
  }

  /**
   * Ensure a challenge definition exists as a persisted entity.
   * Creates it if it doesn't exist in the database.
   */
  private async ensureChallengeEntity(def: typeof ALL_CHALLENGES[0]): Promise<CodingChallengeEntity> {
    // Try to find existing entity by name
    try {
      const existing = await Commands.execute<any, any>('data/list', {
        collection: COLLECTIONS.CODING_CHALLENGES,
        filter: { name: def.name },
        limit: 1,
      });

      if (existing?.success && existing.items?.length > 0) {
        const entity = new CodingChallengeEntity();
        Object.assign(entity, existing.items[0]);
        return entity;
      }
    } catch {
      // Database not available — create in-memory entity
    }

    // Create new entity from definition
    const entity = new CodingChallengeEntity();
    entity.name = def.name;
    entity.description = def.description;
    entity.sequenceNumber = def.sequenceNumber;
    entity.difficulty = def.difficulty;
    entity.category = def.category;
    entity.setupFiles = def.setupFiles;
    entity.expectedOutcome = def.expectedOutcome;
    entity.evaluationCriteria = def.evaluationCriteria;
    entity.expectedFiles = def.expectedFiles;
    entity.timeLimitMs = def.timeLimitMs;
    entity.toolCallLimit = def.toolCallLimit;

    // Persist (best-effort)
    await this.persistChallenge(entity);

    return entity;
  }

  private async loadOrCreateChallenge(challengeId: string): Promise<CodingChallengeEntity> {
    try {
      const result = await Commands.execute<any, any>('data/read', {
        collection: COLLECTIONS.CODING_CHALLENGES,
        id: challengeId,
      });
      if (result?.success && result.item) {
        const entity = new CodingChallengeEntity();
        Object.assign(entity, result.item);
        return entity;
      }
    } catch {
      // Not found
    }
    throw new ValidationError('challengeId', `Challenge not found: ${challengeId}`);
  }

  private async persistChallenge(entity: CodingChallengeEntity): Promise<void> {
    try {
      if (entity.id) {
        await Commands.execute<any, any>('data/update', {
          collection: COLLECTIONS.CODING_CHALLENGES,
          id: entity.id,
          data: { ...entity },
        });
      } else {
        await Commands.execute<any, any>('data/create', {
          collection: COLLECTIONS.CODING_CHALLENGES,
          data: { ...entity },
        });
      }
    } catch {
      // Best-effort persistence
    }
  }
}
