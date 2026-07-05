/**
 * Embedding Generate Command Types
 *
 * Low-level primitive for generating embeddings from text
 */

import type { CommandParams, CommandResult, CommandInput} from '../../../../../system/core/types/JTAGTypes';
import { Commands } from '../../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Generate vector embeddings from text or code for use in semantic search, similarity matching, and RAG retrieval.
 */
export interface EmbeddingGenerateParams extends CommandParams {
  /** Text to generate embedding for (or array of texts) */
  input: string | string[];

  /** Model to use (default: auto-select based on content type) */
  model?: string;

  /** Content type hint for model selection */
  contentType?: 'code' | 'text' | 'documentation';

  /** Provider to use (default: 'candle') */
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

/**
 * Factory function for creating AiEmbeddingGenerateParams
 */
export const createAiEmbeddingGenerateParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<EmbeddingGenerateParams, 'context' | 'sessionId' | 'userId'>
): EmbeddingGenerateParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating AiEmbeddingGenerateResult with defaults
 */
export const createAiEmbeddingGenerateResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<EmbeddingGenerateResult, 'context' | 'sessionId' | 'userId'>
): EmbeddingGenerateResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart ai/embedding/generate-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createAiEmbeddingGenerateResultFromParams = (
  params: EmbeddingGenerateParams,
  differences: Omit<EmbeddingGenerateResult, 'context' | 'sessionId' | 'userId'>
): EmbeddingGenerateResult => transformPayload(params, differences);

