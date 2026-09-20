/**
 * Genome Academy Session Detail Command - Shared Types
 *
 * Retrieves full detail for a single Academy session, including
 * curriculum topics, examination results, and adapter IDs.
 */

import type { CommandParams, CommandResult, CommandInput } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { AcademySessionStatus, AcademyConfig } from '@system/genome/shared/AcademyTypes';

/**
 * Full session detail returned by the command
 */
export interface AcademySessionDetail {
  id: UUID;
  personaId: UUID;
  personaName: string;
  skill: string;
  mode: string;
  status: AcademySessionStatus;
  baseModel: string;
  createdAt: string;
  updatedAt: string;
  teacherHandle?: string;
  studentHandle?: string;
  config: AcademyConfig;
  metrics?: {
    topicsPassed: number;
    topicsFailed: number;
    totalTrainingTime: number;
    averageExamScore: number;
    layerIds: UUID[];
  };
}

/**
 * Summary of a curriculum topic and its exam outcomes
 */
export interface CurriculumTopicSummary {
  name: string;
  passed: boolean;
  examScores: number[];
}

/**
 * Summary of a single examination attempt
 */
export interface ExaminationResult {
  topicIndex: number;
  round: number;
  score: number;
  passed: boolean;
}

/**
 * Genome Academy Session Detail Command Parameters
 */
export interface GenomeAcademySessionDetailParams extends CommandParams {
  /** The Academy session ID to retrieve */
  sessionId: string;
}

/**
 * Genome Academy Session Detail Command Result
 */
export interface GenomeAcademySessionDetailResult extends CommandResult {
  success: boolean;
  /** Full session detail */
  session?: AcademySessionDetail;
  /** Curriculum topics with pass/fail and scores */
  curricula: CurriculumTopicSummary[];
  /** All examination attempts for this session */
  examinations: ExaminationResult[];
  /** Adapter IDs produced by this session */
  adapterIds: string[];
  error?: string;
}

/**
 * Smart inheritance from params -- auto-inherits context and sessionId
 */
export const createGenomeAcademySessionDetailResultFromParams = (
  params: GenomeAcademySessionDetailParams,
  differences: Omit<GenomeAcademySessionDetailResult, 'context' | 'sessionId' | 'userId'>
): GenomeAcademySessionDetailResult => transformPayload(params, differences);

/**
 * GenomeAcademySessionDetail -- Type-safe command executor
 */
export const GenomeAcademySessionDetail = {
  execute(params: CommandInput<GenomeAcademySessionDetailParams>): Promise<GenomeAcademySessionDetailResult> {
    return Commands.execute<GenomeAcademySessionDetailParams, GenomeAcademySessionDetailResult>(
      'genome/academy-session-detail',
      params as Partial<GenomeAcademySessionDetailParams>
    );
  },
  commandName: 'genome/academy-session-detail' as const,
} as const;
