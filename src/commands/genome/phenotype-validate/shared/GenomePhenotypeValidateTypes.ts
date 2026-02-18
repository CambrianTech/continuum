/**
 * Genome Phenotype Validate Command - Shared Types
 *
 * Scores and compares pre-training vs post-training responses
 * using LLM-as-judge to determine if training improved the model.
 * Returns improvement metrics and quality gate pass/fail.
 */

import type { CommandParams, CommandResult, CommandInput } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';

/**
 * Per-question validation result
 */
export interface PhenotypeQuestionResult {
  question: string;
  expectedAnswer: string;
  baselineAnswer: string;
  adaptedAnswer: string;
  baselineScore: number;     // 0-100
  adaptedScore: number;      // 0-100
}

/**
 * Genome Phenotype Validate Command Parameters
 */
export interface GenomePhenotypeValidateParams extends CommandParams {
  /** Exam questions (JSON array of { question, expectedAnswer }) */
  questions: Array<{ question: string; expectedAnswer: string }>;
  /** Student responses BEFORE training (pre-test baseline) */
  baselineResponses: Array<{ questionIndex: number; studentAnswer: string }>;
  /** Student responses AFTER training (post-test) */
  adaptedResponses: Array<{ questionIndex: number; studentAnswer: string }>;
  /** Minimum score improvement (percentage points) to pass quality gate. Default: 5 */
  improvementThreshold?: number;
  /** LLM model for judging */
  model?: string;
  /** LLM provider for judging */
  provider?: string;
}

/**
 * Genome Phenotype Validate Command Result
 */
export interface GenomePhenotypeValidateResult extends CommandResult {
  success: boolean;
  /** Average score across all questions BEFORE training (0-100) */
  baselineScore: number;
  /** Average score across all questions AFTER training (0-100) */
  adaptedScore: number;
  /** Score improvement in percentage points */
  improvement: number;
  /** Whether the improvement meets the quality gate threshold */
  passedQualityGate: boolean;
  /** Per-question breakdown */
  questionResults: PhenotypeQuestionResult[];
  /** Human-readable summary */
  summary: string;
  /** Model that performed the judging */
  judgedBy: string;
  error?: string;
}

/**
 * Smart inheritance from params — auto-inherits context and sessionId
 */
export const createGenomePhenotypeValidateResultFromParams = (
  params: GenomePhenotypeValidateParams,
  differences: Omit<GenomePhenotypeValidateResult, 'context' | 'sessionId' | 'userId'>
): GenomePhenotypeValidateResult => transformPayload(params, differences);

/**
 * Genome Phenotype Validate — Type-safe command executor
 */
export const GenomePhenotypeValidate = {
  execute(params: CommandInput<GenomePhenotypeValidateParams>): Promise<GenomePhenotypeValidateResult> {
    return Commands.execute<GenomePhenotypeValidateParams, GenomePhenotypeValidateResult>(
      'genome/phenotype-validate',
      params as Partial<GenomePhenotypeValidateParams>
    );
  },
  commandName: 'genome/phenotype-validate' as const,
} as const;
