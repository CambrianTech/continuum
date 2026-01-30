/**
 * Embedding Generate Command Types
 *
 * Low-level primitive for generating embeddings from text
 */

import type { CommandParams, CommandResult, CommandInput} from '../../../../../system/core/types/JTAGTypes';
import { Commands } from '../../../../../system/core/shared/Commands';

/**
 * Parameters for ai/embedding/generate command
 */
export interface EmbeddingGenerateParams extends CommandParams {
  /** Text to generate embedding for (or array of texts) */
  input: string | string[];

  /** Model to use (default: auto-select based on content type) */
  model?: string;

  /** Content type hint for model selection */
  contentType?: 'code' | 'text' | 'documentation';

  /** Provider to use (default: 'ollama') */
  provider?: string;
}

/**
 * Result from ai/embedding/generate command
 */
export interface EmbeddingGenerateResult extends CommandResult {
  readonly success: boolean;
  readonly error?: string;

  /** Generated embeddings (one per input) */
  readonly embeddings: number[][];

  /** Model used */
  readonly model: string;

  /** Provider used */
  readonly provider: string;

  /** Embedding dimensions */
  readonly dimensions: number;

  /** Generation time in ms */
  readonly durationMs: number;

  /** Token usage */
  readonly usage?: {
    inputTokens: number;
    totalTokens: number;
  };
}

/**
 * EmbeddingGenerate — Type-safe command executor
 *
 * Usage:
 *   import { EmbeddingGenerate } from '...shared/EmbeddingGenerateTypes';
 *   const result = await EmbeddingGenerate.execute({ ... });
 */
export const EmbeddingGenerate = {
  execute(params: CommandInput<EmbeddingGenerateParams>): Promise<EmbeddingGenerateResult> {
    return Commands.execute<EmbeddingGenerateParams, EmbeddingGenerateResult>('ai/embedding/generate', params as Partial<EmbeddingGenerateParams>);
  },
  commandName: 'ai/embedding/generate' as const,
} as const;
