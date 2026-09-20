/**
 * Genome Academy Session List Command - Shared Types
 *
 * Lists Academy sessions with optional filters for persona, status, and skill.
 */

import type { CommandParams, CommandResult, CommandInput } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { AcademySessionMode, AcademySessionStatus } from '@system/genome/shared/AcademyTypes';

/**
 * Genome Academy Session List Command Parameters
 */
export interface GenomeAcademySessionListParams extends CommandParams {
  /** Filter by student persona ID */
  personaId?: string;
  /** Filter by session status (e.g., 'pending', 'training', 'complete', 'failed') */
  status?: string;
  /** Filter by skill name */
  skill?: string;
  /** Maximum number of sessions to return (default: 50) */
  limit?: number;
}

/**
 * Summary of a single Academy session
 */
export interface AcademySessionSummary {
  id: UUID;
  personaId: UUID;
  personaName: string;
  skill: string;
  mode: AcademySessionMode;
  status: AcademySessionStatus;
  baseModel: string;
  createdAt: string;
  updatedAt: string;
  teacherHandle?: string;
  studentHandle?: string;
  metrics?: {
    topicsPassed: number;
    topicsFailed: number;
    totalTrainingTime: number;
    averageExamScore: number;
    layerIds: UUID[];
  };
}

/**
 * Genome Academy Session List Command Result
 */
export interface GenomeAcademySessionListResult extends CommandResult {
  success: boolean;
  /** Array of matching Academy sessions */
  sessions: AcademySessionSummary[];
  error?: string;
}

/**
 * Smart inheritance from params - auto-inherits context and sessionId
 */
export const createGenomeAcademySessionListResultFromParams = (
  params: GenomeAcademySessionListParams,
  differences: Omit<GenomeAcademySessionListResult, 'context' | 'sessionId' | 'userId'>
): GenomeAcademySessionListResult => transformPayload(params, differences);

/**
 * Genome Academy Session List - Type-safe command executor
 */
export const GenomeAcademySessionList = {
  execute(params: CommandInput<GenomeAcademySessionListParams>): Promise<GenomeAcademySessionListResult> {
    return Commands.execute<GenomeAcademySessionListParams, GenomeAcademySessionListResult>(
      'genome/academy-session-list',
      params as Partial<GenomeAcademySessionListParams>
    );
  },
  commandName: 'genome/academy-session-list' as const,
} as const;
