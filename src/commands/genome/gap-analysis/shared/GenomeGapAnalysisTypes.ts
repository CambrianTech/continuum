/**
 * Genome Gap Analysis Command — Shared Types
 *
 * Analyzes performance gaps for competitors in an Academy competition.
 * Reads exam results from the database, computes per-topic gaps relative
 * to the field, and returns prioritized remediation recommendations.
 */

import type { CommandParams, CommandResult, CommandInput } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { GapAnalysis } from '@system/genome/shared/CompetitionTypes';

/**
 * Genome Gap Analysis Command Parameters
 */
export interface GenomeGapAnalysisParams extends CommandParams {
  /** Competition ID to analyze */
  competitionId: UUID;

  /** Optional: analyze only this persona (default: all competitors) */
  personaId?: UUID;
}

/**
 * Genome Gap Analysis Command Result
 */
export interface GenomeGapAnalysisResult extends CommandResult {
  success: boolean;

  /** Per-persona gap analysis */
  analyses: GapAnalysis[];

  /** Competition skill */
  skill: string;

  /** Total topics analyzed */
  totalTopics: number;

  error?: string;
}

/**
 * Factory: create result from params
 */
export const createGenomeGapAnalysisResultFromParams = (
  params: GenomeGapAnalysisParams,
  differences: Omit<GenomeGapAnalysisResult, 'context' | 'sessionId' | 'userId'>
): GenomeGapAnalysisResult => transformPayload(params, differences);

/**
 * Type-safe command executor
 */
export const GenomeGapAnalysis = {
  execute(params: CommandInput<GenomeGapAnalysisParams>): Promise<GenomeGapAnalysisResult> {
    return Commands.execute<GenomeGapAnalysisParams, GenomeGapAnalysisResult>(
      'genome/gap-analysis',
      params as Partial<GenomeGapAnalysisParams>
    );
  },
  commandName: 'genome/gap-analysis' as const,
} as const;
