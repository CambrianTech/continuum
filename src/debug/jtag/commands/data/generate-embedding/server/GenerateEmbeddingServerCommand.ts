/**
 * Generate Embedding Command - Server Implementation
 *
 * Generates vector embedding for text using specified model.
 * Delegates to DataDaemon which uses VectorSearchAdapterBase.
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { GenerateEmbeddingParams, GenerateEmbeddingResult } from '../shared/GenerateEmbeddingCommandTypes';
import { createGenerateEmbeddingResultFromParams } from '../shared/GenerateEmbeddingCommandTypes';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import { DEFAULT_EMBEDDING_MODELS } from '../../../../daemons/data-daemon/shared/VectorSearchTypes';

const DEFAULT_CONFIG = {
  embedding: {
    defaultModel: 'all-minilm',
    defaultProvider: 'ollama'
  }
} as const;

export class GenerateEmbeddingServerCommand extends CommandBase<GenerateEmbeddingParams, GenerateEmbeddingResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-generate-embedding', context, subpath, commander);
  }

  async execute(params: GenerateEmbeddingParams): Promise<GenerateEmbeddingResult> {
    const startTime = Date.now();
    console.debug(`🧬 GENERATE-EMBEDDING: Generating embedding for text (${params.text.length} chars)`);

    try {
      // Validate required parameters
      if (!params.text || params.text.trim().length === 0) {
        return createGenerateEmbeddingResultFromParams(params, {
          success: false,
          error: 'Text parameter is required and cannot be empty'
        });
      }

      // Prepare embedding model
      const modelName = params.model || DEFAULT_CONFIG.embedding.defaultModel;
      const providerName = params.provider || DEFAULT_CONFIG.embedding.defaultProvider;

      const embeddingModel = DEFAULT_EMBEDDING_MODELS[modelName] || {
        ...DEFAULT_EMBEDDING_MODELS['all-minilm'],
        name: modelName,
        provider: providerName as any
      };

      console.debug(`🧬 GENERATE-EMBEDDING: Using model ${embeddingModel.name} (${embeddingModel.provider})`);

      // Generate embedding via DataDaemon
      const embeddingResult = await DataDaemon.generateEmbedding({
        text: params.text,
        model: embeddingModel
      });

      if (!embeddingResult.success || !embeddingResult.data) {
        console.error(`❌ GENERATE-EMBEDDING: Failed:`, embeddingResult.error);
        return createGenerateEmbeddingResultFromParams(params, {
          success: false,
          error: embeddingResult.error || 'Embedding generation failed'
        });
      }

      const generationTime = Date.now() - startTime;
      console.debug(
        `✅ GENERATE-EMBEDDING: Generated ${embeddingResult.data.embedding.length}-dimensional vector in ${generationTime}ms`,
        `(model: ${embeddingResult.data.model.name})`
      );

      return createGenerateEmbeddingResultFromParams(params, {
        success: true,
        embedding: embeddingResult.data.embedding,
        model: embeddingResult.data.model,
        tokenCount: embeddingResult.data.tokenCount,
        generationTime
      });

    } catch (error: unknown) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ GENERATE-EMBEDDING: Execution failed:`, errorMessage);
      return createGenerateEmbeddingResultFromParams(params, {
        success: false,
        error: `Embedding generation failed: ${errorMessage}`
      });
    }
  }
}
