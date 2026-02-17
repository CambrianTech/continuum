/**
 * Genome Academy Competition Command — Shared Types
 *
 * Launches a multi-persona competition: 1 teacher sentinel generates a shared
 * curriculum, N student sentinels compete on the same exam questions.
 * Rankings computed from exam scores across all topics.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * A competitor definition — persona to enter into the competition
 */
export interface CompetitorDef {
  personaId: UUID;
  personaName: string;
}

/**
 * Genome Academy Competition Command Parameters
 */
export interface GenomeAcademyCompetitionParams extends CommandParams {
  /** Skill to compete on (e.g., "typescript-generics") */
  skill: string;

  /** Array of competitors (minimum 2) */
  competitors: CompetitorDef[];

  /** Base model for training (default: LOCAL_MODELS.DEFAULT) */
  baseModel?: string;

  /** Maximum attempts per topic before failure (default: 3) */
  maxTopicAttempts?: number;

  /** Score required to pass exams, 0-100 (default: 70) */
  passingScore?: number;

  /** Training epochs per round (default: 3) */
  epochs?: number;

  /** LoRA rank (default: 32) */
  rank?: number;

  /** Number of tournament rounds (default: 1) */
  tournamentRounds?: number;

  /** Teacher LLM model */
  model?: string;

  /** Teacher LLM provider */
  provider?: string;
}

/**
 * Per-competitor handle info in the result
 */
export interface CompetitorHandle {
  personaId: UUID;
  personaName: string;
  studentHandle: string;
  sessionId: UUID;
}

/**
 * Genome Academy Competition Command Result
 */
export interface GenomeAcademyCompetitionResult extends CommandResult {
  success: boolean;

  /** The created competition entity ID */
  competitionId: UUID;

  /** Sentinel handle for the shared teacher pipeline */
  teacherHandle: string;

  /** Per-competitor handles */
  competitorHandles: CompetitorHandle[];

  error?: string;
}

/**
 * Factory: create result from params (inherits context + sessionId)
 */
export const createGenomeAcademyCompetitionResultFromParams = (
  params: GenomeAcademyCompetitionParams,
  differences: Omit<GenomeAcademyCompetitionResult, 'context' | 'sessionId' | 'userId'>
): GenomeAcademyCompetitionResult => transformPayload(params, differences);

/**
 * Type-safe command executor
 */
export const GenomeAcademyCompetition = {
  execute(params: CommandInput<GenomeAcademyCompetitionParams>): Promise<GenomeAcademyCompetitionResult> {
    return Commands.execute<GenomeAcademyCompetitionParams, GenomeAcademyCompetitionResult>(
      'genome/academy-competition',
      params as Partial<GenomeAcademyCompetitionParams>
    );
  },
  commandName: 'genome/academy-competition' as const,
} as const;
