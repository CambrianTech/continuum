/**
 * Embedding Generate Command Types
 *
 * Low-level primitive for generating embeddings from text
 */

import type { CommandParams, CommandResult } from '../../../../../system/core/types/JTAGTypes';

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
