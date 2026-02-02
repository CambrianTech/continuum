/**
 * Challenge List Command - Server Implementation
 *
 * Lists available coding challenges with difficulty, status, and best scores.
 * Loads challenge definitions and enriches with attempt data from the database.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ChallengeListParams, ChallengeListResult, ChallengeSummary } from '../shared/ChallengeListTypes';
import { createChallengeListResultFromParams } from '../shared/ChallengeListTypes';
import { ALL_CHALLENGES } from '@system/code/challenges/ChallengeDefinitions';
import { CodingChallengeEntity } from '@system/data/entities/CodingChallengeEntity';
import { Commands } from '@system/core/shared/Commands';
import { COLLECTIONS } from '@system/shared/Constants';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export class ChallengeListServerCommand extends CommandBase<ChallengeListParams, ChallengeListResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('challenge/list', context, subpath, commander);
  }

  async execute(params: ChallengeListParams): Promise<ChallengeListResult> {
    const personaId = (params.personaId ?? params.userId) as UUID | undefined;

    // Filter definitions by difficulty if specified
    let definitions = ALL_CHALLENGES;
    if (params.difficulty) {
      definitions = definitions.filter(d => d.difficulty === params.difficulty);
    }

    // Load persisted entities for attempt data (best-effort)
    const entityMap = await this.loadPersistedEntities();

    // Build summaries
    const challenges: ChallengeSummary[] = definitions.map(def => {
      const entity = entityMap.get(def.name);

      const summary: ChallengeSummary = {
        name: def.name,
        sequenceNumber: def.sequenceNumber,
        difficulty: def.difficulty,
        category: def.category,
        description: def.description,
        timeLimitMs: def.timeLimitMs,
        toolCallLimit: def.toolCallLimit,
        totalAttempts: entity?.totalAttempts ?? 0,
        totalPasses: entity?.totalPasses ?? 0,
        highScore: entity?.highScore ?? 0,
        passRate: entity?.passRate ?? 0,
      };

      // Add persona-specific data if requested
      if (personaId && entity) {
        const best = entity.bestAttemptFor(personaId);
        if (best) {
          summary.personaBestScore = best.score;
          summary.personaBestStatus = best.status;
          summary.personaAttempts = entity.attempts.filter(a => a.personaId === personaId).length;
        }
      }

      return summary;
    });

    // Count completed challenges for persona
    let completedByPersona = 0;
    if (personaId) {
      for (const def of ALL_CHALLENGES) {
        const entity = entityMap.get(def.name);
        if (entity) {
          const best = entity.bestAttemptFor(personaId);
          if (best?.status === 'passed') {
            completedByPersona++;
          }
        }
      }
    }

    return createChallengeListResultFromParams(params, {
      success: true,
      challenges,
      totalChallenges: definitions.length,
      completedByPersona,
    });
  }

  /**
   * Load all persisted challenge entities from the database.
   * Returns a map keyed by challenge name for easy lookup.
   */
  private async loadPersistedEntities(): Promise<Map<string, CodingChallengeEntity>> {
    const map = new Map<string, CodingChallengeEntity>();

    try {
      const result = await Commands.execute<any, any>('data/list', {
        collection: COLLECTIONS.CODING_CHALLENGES,
        limit: 100,
      });

      if (result?.success && Array.isArray(result.items)) {
        for (const item of result.items) {
          const entity = new CodingChallengeEntity();
          Object.assign(entity, item);
          map.set(entity.name, entity);
        }
      }
    } catch {
      // Database not available — return empty map (all stats will be zero)
    }

    return map;
  }
}
