/**
 * OpenAILoRAAdapter - Remote API fine-tuning adapter (Phase 7.1)
 *
 * Philosophy: "Start simple, expand systematically"
 * - Phase 7.1: Basic OpenAI API integration
 * - Phase 7.2+: Advanced features, monitoring, optimization
 *
 * REMOTE API STRATEGY:
 * - Uses OpenAI API for cloud-based LoRA training
 * - Established provider with strong documentation
 * - No local GPU required
 * - Higher cost than DeepSeek ($15/1M input vs $0.55/1M)
 * - Supports GPT-3.5-turbo, GPT-4, GPT-4-turbo models
 *
 * SERVER-ONLY: Uses Node.js for HTTP requests and file system
 */

import { BaseLoRATrainer } from '../../shared/BaseLoRATrainer';
import type {
  LoRATrainingRequest,
  LoRATrainingResult,
  FineTuningCapabilities,
  FineTuningStrategy
} from '../../shared/FineTuningTypes';

/**
 * OpenAI LoRA Adapter - Remote API training with OpenAI
 *
 * Current Status: MVP stub (interface only)
 * Full Implementation: Phase 7.1+
 */
export class OpenAILoRAAdapter extends BaseLoRATrainer {
  readonly providerId = 'openai';

  /**
   * Check if OpenAI supports fine-tuning
   *
   * MVP: Returns false (not implemented yet)
   * Phase 7.1+: Check if API key configured
   */
  supportsFineTuning(): boolean {
    // MVP: Not yet implemented
    return false;

    // TODO Phase 7.1: Check for OpenAI API key
    // return !!process.env.OPENAI_API_KEY;
  }

  /**
   * Get fine-tuning capabilities
   *
   * OpenAI capabilities (remote API training):
   * - LoRA rank: Fixed by provider (not directly configurable)
   * - Epochs: 1-50 (default: 3)
   * - Higher cost: $15/1M input tokens, $60/1M output (27x more than DeepSeek)
   * - No GPU required (cloud-based)
   * - Requires internet connection
   * - Supports GPT-3.5-turbo, GPT-4, GPT-4-turbo models
   */
  getFineTuningCapabilities(): FineTuningCapabilities {
    return {
      // LoRA parameters (fixed by provider)
      minRank: 16,
      maxRank: 64,
      defaultRank: 32,
      minAlpha: 16,
      maxAlpha: 64,
      defaultAlpha: 32,

      // Training parameters
      minEpochs: 1,
      maxEpochs: 50,
      defaultEpochs: 3,
      minLearningRate: 0.00001,
      maxLearningRate: 0.001,
      defaultLearningRate: 0.0001,
      minBatchSize: 1,
      maxBatchSize: 32,
      defaultBatchSize: 4,

      // Cost (higher than DeepSeek - 27x more expensive)
      // Average example = ~100 tokens input + ~50 tokens output = ~$0.00405 per example
      costPerExample: 0.00405,

      // Performance (API latency + training time)
      estimatedTrainingTime: 800, // 800ms per example per epoch (API overhead)

      // Model support (OpenAI models)
      supportedBaseModels: [
        'gpt-3.5-turbo',
        'gpt-4',
        'gpt-4-turbo',
        'gpt-4o',
        'gpt-4o-mini'
      ],

      // Requirements
      requiresGPU: false, // Cloud-based training
      requiresInternet: true // API calls
    };
  }

  /**
   * Train LoRA adapter
   *
   * MVP: Throws "not implemented" error
   * Phase 7.1+: Upload dataset to OpenAI API and monitor training job
   *
   * @param request Training configuration
   * @returns Training result with model ID
   */
  async trainLoRA(request: LoRATrainingRequest): Promise<LoRATrainingResult> {
    // Validate request first
    this.validateRequest(request);

    // MVP: Not implemented yet
    throw new Error(
      'OpenAI LoRA training not implemented yet (Phase 7.0 MVP). ' +
      'Full implementation in Phase 7.1+ will use OpenAI API for cloud training.'
    );

    // TODO Phase 7.1: Implement OpenAI API training
    // const result = await this.trainWithOpenAIAPI(request);
    // return result;
  }

  /**
   * Get training strategy (remote API)
   */
  getFineTuningStrategy(): FineTuningStrategy {
    return 'remote-api';
  }

  /**
   * Estimate training cost (OpenAI pricing)
   *
   * Pricing: $15/1M input tokens, $60/1M output tokens
   * Assumption: Average example = 100 tokens input + 50 tokens output
   */
  estimateTrainingCost(exampleCount: number): number {
    const capabilities = this.getFineTuningCapabilities();

    // Use capability metadata for cost estimation
    if (capabilities.costPerExample) {
      return exampleCount * capabilities.costPerExample;
    }

    // Fallback: Conservative estimate
    // 100 tokens input * $15/1M = $0.0015
    // 50 tokens output * $60/1M = $0.003
    // Total per example = ~$0.00405
    return exampleCount * 0.00405;
  }

  /**
   * Estimate training time
   *
   * Assumptions:
   * - API latency + training time: ~800ms per example per epoch
   * - Includes upload, processing, download
   */
  estimateTrainingTime(exampleCount: number, epochs: number): number {
    const capabilities = this.getFineTuningCapabilities();

    // Use API estimate from capabilities
    if (capabilities.estimatedTrainingTime) {
      return exampleCount * epochs * capabilities.estimatedTrainingTime;
    }

    // Fallback: Conservative estimate
    return exampleCount * epochs * 800; // 800ms per example per epoch
  }

  // ==================== FUTURE IMPLEMENTATION (Phase 7.1+) ====================

  /**
   * TODO Phase 7.1: Train LoRA adapter with OpenAI API
   *
   * Implementation steps:
   * 1. Export dataset to JSONL
   * 2. Upload dataset to OpenAI API
   * 3. Create fine-tuning job via API
   * 4. Poll job status until complete
   * 5. Get trained model ID
   * 6. Save adapter metadata
   * 7. Return result with model ID and metrics
   *
   * @private
   */
  /*
  private async trainWithOpenAIAPI(request: LoRATrainingRequest): Promise<LoRATrainingResult> {
    const startTime = Date.now();

    // 1. Export dataset to JSONL
    const datasetPath = await this.exportDatasetToJSONL(request.dataset);

    // 2. Upload dataset to OpenAI
    const fileId = await this.uploadDataset(datasetPath);

    // 3. Create fine-tuning job
    const jobId = await this.createFineTuningJob(request, fileId);

    // 4. Monitor job progress
    const metrics = await this.monitorTrainingJob(jobId);

    // 5. Get trained model ID
    const modelId = await this.getTrainedModelId(jobId);

    // 6. Save adapter metadata
    await this.saveAdapterMetadata(request, modelId, metrics);

    // 7. Clean up temp files
    await this.cleanupTempFiles(datasetPath);

    const trainingTime = Date.now() - startTime;

    return {
      success: true,
      modelId,
      metrics: {
        trainingTime,
        finalLoss: metrics.finalLoss,
        examplesProcessed: request.dataset.examples.length,
        epochs: request.epochs || 3
      },
      timestamp: Date.now()
    };
  }
  */

  /**
   * TODO Phase 7.1: Export dataset to JSONL file
   *
   * @private
   */
  /*
  private async exportDatasetToJSONL(dataset: TrainingDataset): Promise<string> {
    const tempPath = path.join(os.tmpdir(), `openai-training-${Date.now()}.jsonl`);
    const jsonl = TrainingDatasetBuilder.exportToJSONL(dataset);
    await fs.promises.writeFile(tempPath, jsonl, 'utf-8');
    return tempPath;
  }
  */

  /**
   * TODO Phase 7.1: Upload dataset to OpenAI API
   *
   * @private
   */
  /*
  private async uploadDataset(datasetPath: string): Promise<string> {
    const apiKey = process.env.OPENAI_API_KEY;
    if (!apiKey) {
      throw new Error('OPENAI_API_KEY not configured');
    }

    // Upload dataset via OpenAI API
    // POST https://api.openai.com/v1/files
    const formData = new FormData();
    formData.append('file', fs.createReadStream(datasetPath));
    formData.append('purpose', 'fine-tune');

    const response = await fetch('https://api.openai.com/v1/files', {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${apiKey}`
      },
      body: formData
    });

    const data = await response.json();
    return data.id; // File ID
  }
  */

  /**
   * TODO Phase 7.1: Create fine-tuning job via API
   *
   * @private
   */
  /*
  private async createFineTuningJob(
    request: LoRATrainingRequest,
    fileId: string
  ): Promise<string> {
    const apiKey = process.env.OPENAI_API_KEY;

    // Create fine-tuning job
    // POST https://api.openai.com/v1/fine_tuning/jobs
    const response = await fetch('https://api.openai.com/v1/fine_tuning/jobs', {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${apiKey}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        training_file: fileId,
        model: request.baseModel,
        hyperparameters: {
          n_epochs: request.epochs || 3,
          learning_rate_multiplier: request.learningRate || 0.0001,
          batch_size: request.batchSize || 4
        }
      })
    });

    const data = await response.json();
    return data.id; // Job ID
  }
  */

  /**
   * TODO Phase 7.1: Monitor training job until complete
   *
   * @private
   */
  /*
  private async monitorTrainingJob(jobId: string): Promise<TrainingMetrics> {
    const apiKey = process.env.OPENAI_API_KEY;

    // Poll job status every 10 seconds
    while (true) {
      const response = await fetch(`https://api.openai.com/v1/fine_tuning/jobs/${jobId}`, {
        headers: { 'Authorization': `Bearer ${apiKey}` }
      });

      const job = await response.json();

      if (job.status === 'succeeded') {
        return {
          finalLoss: job.trained_tokens ? 0.5 : 1.0, // OpenAI doesn't expose loss
          trainingSteps: job.trained_tokens,
          examplesProcessed: job.training_file_count
        };
      } else if (job.status === 'failed') {
        throw new Error(`Training failed: ${job.error}`);
      }

      // Wait 10 seconds before next poll
      await new Promise(resolve => setTimeout(resolve, 10000));
    }
  }
  */

  /**
   * TODO Phase 7.1: Get trained model ID
   *
   * @private
   */
  /*
  private async getTrainedModelId(jobId: string): Promise<string> {
    const apiKey = process.env.OPENAI_API_KEY;

    const response = await fetch(`https://api.openai.com/v1/fine_tuning/jobs/${jobId}`, {
      headers: { 'Authorization': `Bearer ${apiKey}` }
    });

    const job = await response.json();
    return job.fine_tuned_model; // Model ID for inference
  }
  */

  /**
   * TODO Phase 7.1: Save adapter metadata
   *
   * @private
   */
  /*
  private async saveAdapterMetadata(
    request: LoRATrainingRequest,
    modelId: string,
    metrics: TrainingMetrics
  ): Promise<void> {
    const metadataPath = path.join(
      '.continuum/genome/adapters',
      `${request.baseModel}-${request.traitType}-${Date.now()}.json`
    );

    await fs.promises.mkdir(path.dirname(metadataPath), { recursive: true });

    await fs.promises.writeFile(
      metadataPath,
      JSON.stringify({
        modelId,
        baseModel: request.baseModel,
        traitType: request.traitType,
        personaId: request.personaId,
        personaName: request.personaName,
        metrics,
        createdAt: Date.now()
      }, null, 2)
    );
  }
  */

  /**
   * TODO Phase 7.1: Clean up temporary files
   *
   * @private
   */
  /*
  private async cleanupTempFiles(...paths: string[]): Promise<void> {
    for (const filePath of paths) {
      try {
        await fs.promises.unlink(filePath);
      } catch (error) {
        console.warn(`Failed to clean up temp file: ${filePath}`, error);
      }
    }
  }
  */
}
