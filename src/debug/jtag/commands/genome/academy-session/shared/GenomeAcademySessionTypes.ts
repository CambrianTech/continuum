/**
 * Genome Academy Session Command - Shared Types
 *
 * Entry point for the Academy Dojo system. Creates an AcademySessionEntity
 * and spawns dual sentinels (teacher + student) for autonomous skill training.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Genome Academy Session Command Parameters
 */
export interface GenomeAcademySessionParams extends CommandParams {
  /** The student persona ID */
  personaId: UUID;
  /** Student persona display name */
  personaName: string;
  /** Skill to teach (e.g., "typescript-generics", "ethical-reasoning") */
  skill: string;
  /** Base model for training (default: "smollm2:135m") */
  baseModel?: string;
  /** Maximum attempts per topic before failure (default: 3) */
  maxTopicAttempts?: number;
  /** Score required to pass exams, 0-100 (default: 70) */
  passingScore?: number;
  /** Training epochs per round (default: 3) */
  epochs?: number;
  /** LoRA rank (default: 32) */
  rank?: number;
  /** Teacher LLM model */
  model?: string;
  /** Teacher LLM provider */
  provider?: string;
}

/**
 * Factory function for creating GenomeAcademySessionParams
 */
export const createGenomeAcademySessionParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    personaId: UUID;
    personaName: string;
    skill: string;
    baseModel?: string;
    maxTopicAttempts?: number;
    passingScore?: number;
    epochs?: number;
    rank?: number;
    model?: string;
    provider?: string;
  }
): GenomeAcademySessionParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  baseModel: data.baseModel ?? 'smollm2:135m',
  maxTopicAttempts: data.maxTopicAttempts ?? 3,
  passingScore: data.passingScore ?? 70,
  epochs: data.epochs ?? 3,
  rank: data.rank ?? 32,
  ...data
});

/**
 * Genome Academy Session Command Result
 */
export interface GenomeAcademySessionResult extends CommandResult {
  success: boolean;
  /** The created Academy session ID */
  academySessionId: UUID;
  /** Sentinel handle for the teacher pipeline */
  teacherHandle: string;
  /** Sentinel handle for the student pipeline */
  studentHandle: string;
  error?: string;
}

/**
 * Factory function for creating GenomeAcademySessionResult with defaults
 */
export const createGenomeAcademySessionResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    academySessionId?: UUID;
    teacherHandle?: string;
    studentHandle?: string;
    error?: string;
  }
): GenomeAcademySessionResult => createPayload(context, sessionId, {
  academySessionId: data.academySessionId ?? '' as UUID,
  teacherHandle: data.teacherHandle ?? '',
  studentHandle: data.studentHandle ?? '',
  ...data
});

/**
 * Smart inheritance from params — auto-inherits context and sessionId
 */
export const createGenomeAcademySessionResultFromParams = (
  params: GenomeAcademySessionParams,
  differences: Omit<GenomeAcademySessionResult, 'context' | 'sessionId' | 'userId'>
): GenomeAcademySessionResult => transformPayload(params, differences);

/**
 * Genome Academy Session — Type-safe command executor
 */
export const GenomeAcademySession = {
  execute(params: CommandInput<GenomeAcademySessionParams>): Promise<GenomeAcademySessionResult> {
    return Commands.execute<GenomeAcademySessionParams, GenomeAcademySessionResult>(
      'genome/academy-session',
      params as Partial<GenomeAcademySessionParams>
    );
  },
  commandName: 'genome/academy-session' as const,
} as const;
