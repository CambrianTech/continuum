/**
 * Embedding Generate Server Command
 *
 * Server-side embedding generation using AI provider adapters
 */

import { EmbeddingGenerateCommand } from '../shared/EmbeddingGenerateCommand';
import type { JTAGContext } from '../../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { EmbeddingGenerateParams, EmbeddingGenerateResult } from '../shared/EmbeddingGenerateTypes';
import { AIProviderDaemon } from '../../../../../daemons/ai-provider-daemon/shared/AIProviderDaemon';

export class EmbeddingGenerateServerCommand extends EmbeddingGenerateCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/embedding/generate', context, subpath, commander);
  }

  async execute(params: EmbeddingGenerateParams): Promise<EmbeddingGenerateResult> {
    const startTime = Date.now();

    try {
      // Select model based on content type
      const model = params.model ?? this.selectModelForContentType(params.contentType);
      const provider = params.provider ?? 'candle';

      console.log(`🔢 Generating embedding(s) with ${provider}/${model}`);

      // Generate embeddings via AIProviderDaemon
      const result = await AIProviderDaemon.createEmbedding({
        input: params.input,
        model,
        preferredProvider: provider,
        context: this.context
      });

      const durationMs = Date.now() - startTime;

      console.log(`✅ Generated ${result.embeddings.length} embedding(s) in ${durationMs}ms (${result.embeddings[0]?.length || 0} dimensions)`);

      return {
        success: true,
        embeddings: result.embeddings,
        model: result.model,
        provider: result.provider,
        dimensions: result.embeddings[0]?.length || 0,
        durationMs,
        usage: result.usage ? {
          inputTokens: result.usage.inputTokens,
          totalTokens: result.usage.totalTokens
        } : undefined,
        context: this.context,
        sessionId: params.sessionId
      };
    } catch (error) {
      const durationMs = Date.now() - startTime;

      console.error(`❌ Embedding generation failed after ${durationMs}ms`);
      console.error(`   Error: ${error instanceof Error ? error.message : String(error)}`);

      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
        embeddings: [],
        model: params.model ?? 'unknown',
        provider: params.provider ?? 'candle',
        dimensions: 0,
        durationMs,
        context: this.context,
        sessionId: params.sessionId
      };
    }
  }

  /**
   * Select appropriate embedding model based on content type
   */
  private selectModelForContentType(contentType?: 'code' | 'text' | 'documentation'): string {
    switch (contentType) {
      case 'code':
        return 'qwen3-embedding';  // Trained on GitHub repos
      case 'text':
      case 'documentation':
        return 'nomic-embed-text';  // Optimized for natural language
      default:
        return 'nomic-embed-text';  // Default to text model
    }
  }
}
