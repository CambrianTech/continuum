/**
 * Backfill Vectors Server Command
 * Batch generates embeddings for existing records in a collection
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type {
  BackfillVectorsParams,
  BackfillVectorsResult
} from '../shared/BackfillVectorsCommandTypes';
import { createBackfillVectorsResultFromParams } from '../shared/BackfillVectorsCommandTypes';
import { ORM } from '../../../../daemons/data-daemon/shared/ORM';
import { DEFAULT_EMBEDDING_MODELS } from '../../../../daemons/data-daemon/shared/VectorSearchTypes';

const DEFAULT_CONFIG = {
  backfill: {
    defaultModel: 'all-minilm',
    defaultProvider: 'candle',
    defaultBatchSize: 100,
    skipExisting: true
  }
} as const;

export class BackfillVectorsServerCommand extends CommandBase<BackfillVectorsParams, BackfillVectorsResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-backfill-vectors', context, subpath, commander);
  }

  async execute(params: BackfillVectorsParams): Promise<BackfillVectorsResult> {
    const startTime = Date.now();
    console.debug(
      `🔄 BACKFILL-VECTORS: Starting batch embedding generation for collection '${params.collection}' (field: ${params.textField})`
    );

    try {
      // Validate required parameters
      if (!params.collection || params.collection.trim().length === 0) {
        return createBackfillVectorsResultFromParams(params, {
          success: false,
          error: 'Collection parameter is required and cannot be empty'
        });
      }

      if (!params.textField || params.textField.trim().length === 0) {
        return createBackfillVectorsResultFromParams(params, {
          success: false,
          error: 'textField parameter is required and cannot be empty'
        });
      }

      // Prepare embedding model
      const modelName = params.model || DEFAULT_CONFIG.backfill.defaultModel;
      const providerName = params.provider || DEFAULT_CONFIG.backfill.defaultProvider;
      const batchSize = params.batchSize || DEFAULT_CONFIG.backfill.defaultBatchSize;
      const skipExisting = params.skipExisting ?? DEFAULT_CONFIG.backfill.skipExisting;

      const embeddingModel = DEFAULT_EMBEDDING_MODELS[modelName] || {
        ...DEFAULT_EMBEDDING_MODELS['all-minilm'],
        name: modelName,
        provider: providerName as any
      };

      console.debug(
        `🔄 BACKFILL-VECTORS: Using model ${embeddingModel.name} (${embeddingModel.provider}), batchSize: ${batchSize}, skipExisting: ${skipExisting}`
      );

      // Progress tracking
      let lastProgressUpdate = Date.now();
      const progressCallback = (progress: {
        total: number;
        processed: number;
        failed: number;
        elapsedTime: number;
        estimatedRemaining?: number;
      }) => {
        const now = Date.now();
        // Log progress every 5 seconds to avoid flooding
        if (now - lastProgressUpdate > 5000) {
          console.debug(
            `🔄 BACKFILL-VECTORS: Progress: ${progress.processed}/${progress.total} (${((progress.processed / progress.total) * 100).toFixed(1)}%)`,
            progress.estimatedRemaining ? `ETA: ${Math.round(progress.estimatedRemaining / 1000)}s` : ''
          );
          lastProgressUpdate = now;
        }
      };

      // Execute backfill via DataDaemon
      const backfillResult = await ORM.backfillVectors(
        {
          collection: params.collection,
          textField: params.textField,
          filter: params.filter,
          batchSize,
          model: embeddingModel
        },
        progressCallback
      );

      if (!backfillResult.success || !backfillResult.data) {
        console.error(`❌ BACKFILL-VECTORS: Failed:`, backfillResult.error);
        return createBackfillVectorsResultFromParams(params, {
          success: false,
          error: backfillResult.error || 'Backfill vectors operation failed'
        });
      }

      const elapsedTime = Date.now() - startTime;

      // Calculate vectors created/skipped from progress data
      const progress = backfillResult.data;
      const vectorsCreated = progress.processed - progress.failed;
      const vectorsSkipped = progress.total - progress.processed;
      const vectorsFailed = progress.failed;

      console.debug(
        `✅ BACKFILL-VECTORS: Completed in ${elapsedTime}ms`,
        `(created: ${vectorsCreated}, skipped: ${vectorsSkipped}, failed: ${vectorsFailed})`
      );

      return createBackfillVectorsResultFromParams(params, {
        success: true,
        vectorsCreated,
        vectorsSkipped,
        vectorsFailed,
        elapsedTime,
        model: {
          name: embeddingModel.name,
          dimensions: embeddingModel.dimensions,
          provider: embeddingModel.provider
        }
      });

    } catch (error: unknown) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ BACKFILL-VECTORS: Execution failed:`, errorMessage);
      return createBackfillVectorsResultFromParams(params, {
        success: false,
        error: `Backfill vectors failed: ${errorMessage}`
      });
    }
  }
}
