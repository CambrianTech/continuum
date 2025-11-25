/**
 * Generate Embedding Command - Shared Types
 *
 * Generates vector embedding for text using specified model.
 */

import type { JTAGPayload, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { VectorEmbedding, EmbeddingModel } from '../../../../daemons/data-daemon/shared/VectorSearchTypes';

/**
 * Generate embedding command parameters
 */
export interface GenerateEmbeddingParams extends JTAGPayload {
  readonly text: string;
  readonly model?: string;     // Model name: 'all-minilm' | 'nomic-embed-text' | 'text-embedding-3-small'
  readonly provider?: string;  // Provider: 'ollama' | 'openai' | 'huggingface'
}

/**
 * Generate embedding command result
 */
export interface GenerateEmbeddingResult extends JTAGPayload {
  readonly success: boolean;
  readonly embedding?: VectorEmbedding;
  readonly model?: {
    readonly name: string;
    readonly dimensions: number;
    readonly provider: string;
    readonly maxTokens?: number;
  };
  readonly tokenCount?: number;
  readonly generationTime?: number;  // ms
  readonly error?: string;
}

export const createGenerateEmbeddingParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<GenerateEmbeddingParams, 'context' | 'sessionId'>
): GenerateEmbeddingParams => createPayload(context, sessionId, data);

export const createGenerateEmbeddingResultFromParams = (
  params: GenerateEmbeddingParams,
  differences: Omit<Partial<GenerateEmbeddingResult>, 'context' | 'sessionId'>
): GenerateEmbeddingResult => transformPayload(params, {
  success: false,
  ...differences
});
