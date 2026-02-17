/**
 * Genome Dataset Synthesize Command - Shared Types
 *
 * Uses an LLM to synthesize training data for a given topic/skill.
 * Generates Q&A pairs in the persona's voice, saved as JSONL
 * compatible with genome/train.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Genome Dataset Synthesize Command Parameters
 */
export interface GenomeDatasetSynthesizeParams extends CommandParams {
  /** Topic to generate training data about */
  topic: string;
  /** Parent skill domain (e.g., "typescript", "ethical-reasoning") */
  skill: string;
  /** Student persona name (for voice matching in generated data) */
  personaName: string;
  /** Number of training examples to generate (default: 20) */
  exampleCount?: number;
  /** Difficulty level for generated examples */
  difficulty?: 'beginner' | 'intermediate' | 'advanced';
  /** LLM model for generation */
  model?: string;
  /** LLM provider for generation */
  provider?: string;
  /** Override default output path */
  outputPath?: string;
}

/**
 * Factory function for creating GenomeDatasetSynthesizeParams
 */
export const createGenomeDatasetSynthesizeParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    topic: string;
    skill: string;
    personaName: string;
    exampleCount?: number;
    difficulty?: 'beginner' | 'intermediate' | 'advanced';
    model?: string;
    provider?: string;
    outputPath?: string;
  }
): GenomeDatasetSynthesizeParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  exampleCount: data.exampleCount ?? 20,
  difficulty: data.difficulty ?? 'intermediate',
  ...data
});

/**
 * Genome Dataset Synthesize Command Result
 */
export interface GenomeDatasetSynthesizeResult extends CommandResult {
  success: boolean;
  /** Absolute path to the generated JSONL file */
  datasetPath: string;
  /** Number of training examples generated */
  exampleCount: number;
  /** Topic the data was generated for */
  topic: string;
  /** Model that generated the data */
  generatedBy: string;
  error?: string;
}

/**
 * Factory function for creating GenomeDatasetSynthesizeResult with defaults
 */
export const createGenomeDatasetSynthesizeResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    datasetPath?: string;
    exampleCount?: number;
    topic?: string;
    generatedBy?: string;
    error?: string;
  }
): GenomeDatasetSynthesizeResult => createPayload(context, sessionId, {
  datasetPath: data.datasetPath ?? '',
  exampleCount: data.exampleCount ?? 0,
  topic: data.topic ?? '',
  generatedBy: data.generatedBy ?? '',
  ...data
});

/**
 * Smart inheritance from params — auto-inherits context and sessionId
 */
export const createGenomeDatasetSynthesizeResultFromParams = (
  params: GenomeDatasetSynthesizeParams,
  differences: Omit<GenomeDatasetSynthesizeResult, 'context' | 'sessionId' | 'userId'>
): GenomeDatasetSynthesizeResult => transformPayload(params, differences);

/**
 * Genome Dataset Synthesize — Type-safe command executor
 */
export const GenomeDatasetSynthesize = {
  execute(params: CommandInput<GenomeDatasetSynthesizeParams>): Promise<GenomeDatasetSynthesizeResult> {
    return Commands.execute<GenomeDatasetSynthesizeParams, GenomeDatasetSynthesizeResult>(
      'genome/dataset-synthesize',
      params as Partial<GenomeDatasetSynthesizeParams>
    );
  },
  commandName: 'genome/dataset-synthesize' as const,
} as const;
