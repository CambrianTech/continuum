/**
 * Generate Embedding Command - Shared Types
 *
 * Generates vector embedding for text using specified model.
 */

import type { CommandParams, JTAGPayload, JTAGContext, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { VectorEmbedding, EmbeddingModel } from '../../../../daemons/data-daemon/shared/VectorSearchTypes';
import { Commands } from '../../../../system/core/shared/Commands';

/**
 * Generate embedding command parameters
 */
export interface GenerateEmbeddingParams extends CommandParams {
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

/**
 * GenerateEmbedding — Type-safe command executor
 *
 * Usage:
 *   import { GenerateEmbedding } from '...shared/GenerateEmbeddingTypes';
 *   const result = await GenerateEmbedding.execute({ ... });
 */
export const GenerateEmbedding = {
  execute(params: CommandInput<GenerateEmbeddingParams>): Promise<GenerateEmbeddingResult> {
    return Commands.execute<GenerateEmbeddingParams, GenerateEmbeddingResult>('data/generate-embedding', params as Partial<GenerateEmbeddingParams>);
  },
  commandName: 'data/generate-embedding' as const,
} as const;
