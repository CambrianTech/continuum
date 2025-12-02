/**
 * DeepSeekLoRAAdapter - Remote API fine-tuning adapter (Phase 7.1)
 *
 * Philosophy: "Start simple, expand systematically"
 * - Phase 7.1: Basic DeepSeek API integration
 * - Phase 7.2+: Advanced features, monitoring, optimization
 *
 * REMOTE API STRATEGY:
 * - Uses DeepSeek API for cloud-based LoRA training
 * - 27x cheaper than OpenAI ($0.55/1M input vs $15/1M)
 * - No local GPU required
 * - Adapter files downloaded after training
 * - Supports DeepSeek models (R1, Chat, etc.)
 *
 * SERVER-ONLY: Uses Node.js for HTTP requests and file system
 */

import { BaseLoRATrainer } from '../../../../../system/genome/fine-tuning/shared/BaseLoRATrainer';
import { TrainingDatasetBuilder } from '../../../../../system/genome/fine-tuning/server/TrainingDatasetBuilder';
import type {
  LoRATrainingRequest,
  LoRATrainingResult,
  FineTuningCapabilities,
  FineTuningStrategy,
  TrainingDataset,
  TrainingStatus
} from '../../../../../system/genome/fine-tuning/shared/FineTuningTypes';
import type { UUID } from '../../../../../system/core/types/CrossPlatformUUID';
import { DeepSeekBaseConfig } from '../shared/DeepSeekBaseConfig';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import { FormData } from 'formdata-node';

/**
 * DeepSeek LoRA Adapter - Remote API training with DeepSeek
 *
 * Current Status: MVP stub (interface only)
 * Full Implementation: Phase 7.1+
 */
export class DeepSeekLoRAAdapter extends BaseLoRATrainer {
  readonly providerId = 'deepseek';
  private readonly config: DeepSeekBaseConfig;

  constructor(apiKey?: string) {
    super();
    this.config = new DeepSeekBaseConfig(apiKey);
  }

  /**
   * Check if DeepSeek supports fine-tuning
   *
   * Phase 7.1: Check if API key configured
   */
  supportsFineTuning(): boolean {
    return this.config.hasApiKey();
  }

  /**
   * Get fine-tuning capabilities
   *
   * DeepSeek capabilities (remote API training):
   * - LoRA rank: Fixed by provider (typically 8-64)
   * - Epochs: 1-20 (default: 3)
   * - Extremely low cost: $0.55/1M input tokens, $2.19/1M output
   * - No GPU required (cloud-based)
   * - Requires internet connection
   * - Supports DeepSeek models (R1, Chat, Coder)
   */
  getFineTuningCapabilities(): FineTuningCapabilities {
    return {
      supportsFineTuning: this.supportsFineTuning(),
      strategy: this.getFineTuningStrategy(),

      // LoRA parameters (typically fixed by provider)
      minRank: 8,
      maxRank: 64,
      defaultRank: 16,
      minAlpha: 8,
      maxAlpha: 64,
      defaultAlpha: 16,

      // Training parameters
      minEpochs: 1,
      maxEpochs: 20,
      defaultEpochs: 3,
      minLearningRate: 0.00001,
      maxLearningRate: 0.001,
      defaultLearningRate: 0.0001,
      minBatchSize: 1,
      maxBatchSize: 32,
      defaultBatchSize: 4,

      // Cost (extremely competitive - 27x cheaper than OpenAI)
      // Average example = ~100 tokens input + ~50 tokens output = ~$0.00015 per example
      costPerExample: 0.00015,

      // Performance (API latency + training time)
      estimatedTrainingTime: 1000, // 1000ms per example per epoch (API overhead)

      // Model support (DeepSeek models)
      supportedBaseModels: this.config.getSupportedFineTuningModels(),

      // Requirements
      requiresGPU: false, // Cloud-based training
      requiresInternet: true, // API calls

      // Genome capabilities (adapter composition)
      maxActiveLayers: 1,              // DeepSeek: single layer per inference
      supportsDownload: true,           // Can download adapter weights
      supportsLocalComposition: false,  // Downloadable but must compose locally with PEFT
      compositionMethods: []            // No native composition, use PEFT after download
    };
  }

  /**
   * Train LoRA adapter
   *
   * Phase 7.1: Upload dataset to DeepSeek API and monitor training job
   *
   * @param request Training configuration
   * @returns Training result with model ID
   */
  async trainLoRA(request: LoRATrainingRequest): Promise<LoRATrainingResult> {
    // Validate request first
    this.validateRequest(request);

    // Check API key
    const apiKey = this.config.apiKey;
    if (!apiKey) {
      return {
        success: false,
        error: 'DEEPSEEK_API_KEY not configured. Please add it to ~/.continuum/config.env'
      };
    }

    this.log('info', '🚀 DeepSeek: Starting cloud-based LoRA training...');
    const startTime = Date.now();

    try {
      // 1. Export dataset to JSONL
      this.log('info', '   Exporting dataset to JSONL...');
      const datasetPath = await this.exportDatasetToJSONL(request.dataset);
      this.log('debug', `   Dataset exported: ${datasetPath}`);

      // 2. Upload dataset to DeepSeek
      this.log('info', '   Uploading dataset to DeepSeek API...');
      const datasetId = await this.uploadDataset(datasetPath, apiKey);
      this.log('debug', `   Dataset uploaded: ${datasetId}`);

      // 3. Create fine-tuning job
      this.log('info', '   Creating fine-tuning job...');
      const jobId = await this.createFineTuningJob(request, datasetId, apiKey);
      this.log('debug', `   Job created: ${jobId}`);

      // 4. Monitor job progress
      this.log('info', '   Monitoring training progress...');
      const metrics = await this.monitorTrainingJob(jobId, apiKey);
      this.log('info', '   Training complete!');

      // 5. Get trained model ID
      const modelId = await this.getTrainedModelId(jobId, apiKey);
      this.log('debug', `   Model ID: ${modelId}`);

      // 6. Save adapter metadata
      const metadataPath = await this.saveAdapterMetadata(request, modelId, metrics);
      this.log('debug', `   Metadata saved: ${metadataPath}`);

      // 7. Clean up temp files
      await this.cleanupTempFiles(datasetPath);

      const trainingTime = Date.now() - startTime;

      return {
        success: true,
        modelId,
        modelPath: metadataPath,
        metrics: {
          trainingTime,
          finalLoss: metrics.finalLoss,
          examplesProcessed: request.dataset.examples.length,
          epochs: request.epochs ?? 3
        }
      };

    } catch (error) {
      this.log('error', `❌ DeepSeek training failed: ${error instanceof Error ? error.message : String(error)}`);
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Check training status - NOT IMPLEMENTED YET
   * TODO: Implement async handle pattern for this adapter
   */
  async checkStatus(_sessionId: UUID): Promise<TrainingStatus> {
    throw new Error(`${this.providerId}: checkStatus not implemented yet - adapter needs refactoring to async handle pattern`);
  }

  /**
   * Get training strategy (remote API)
   */
  getFineTuningStrategy(): FineTuningStrategy {
    return 'remote-api';
  }

  /**
   * Estimate training cost (DeepSeek pricing)
   *
   * Pricing: $0.55/1M input tokens, $2.19/1M output tokens
   * Assumption: Average example = 100 tokens input + 50 tokens output
   */
  estimateTrainingCost(exampleCount: number): number {
    const capabilities = this.getFineTuningCapabilities();

    // Use capability metadata for cost estimation
    if (capabilities.costPerExample) {
      return exampleCount * capabilities.costPerExample;
    }

    // Fallback: Conservative estimate
    // 100 tokens input * $0.55/1M = $0.000055
    // 50 tokens output * $2.19/1M = $0.0001095
    // Total per example = ~$0.00015
    return exampleCount * 0.00015;
  }

  /**
   * Estimate training time
   *
   * Assumptions:
   * - API latency + training time: ~1000ms per example per epoch
   * - Includes upload, processing, download
   */
  estimateTrainingTime(exampleCount: number, epochs: number): number {
    const capabilities = this.getFineTuningCapabilities();

    // Use API estimate from capabilities
    if (capabilities.estimatedTrainingTime !== undefined) {
      return exampleCount * epochs * capabilities.estimatedTrainingTime;
    }

    // Fallback: Conservative estimate
    return exampleCount * epochs * 1000; // 1000ms per example per epoch
  }

  // ==================== IMPLEMENTATION (Phase 7.1) ====================

  /**
   * Export dataset to JSONL file
   * @private
   */
  protected async exportDatasetToJSONL(dataset: TrainingDataset): Promise<string> {
    const tempPath = path.join(os.tmpdir(), `deepseek-training-${Date.now()}.jsonl`);
    const jsonl = TrainingDatasetBuilder.exportToJSONL(dataset);
    await fs.promises.writeFile(tempPath, jsonl, 'utf-8');
    return tempPath;
  }

  /**
   * Upload dataset to DeepSeek API
   * @private
   */
  private async uploadDataset(datasetPath: string, apiKey: string): Promise<string> {
    // Read file content
    const fileContent = await fs.promises.readFile(datasetPath, 'utf-8');
    const fileBlob = new Blob([fileContent], { type: 'application/jsonl' });

    // Create form data
    const formData = new FormData();
    formData.append('file', fileBlob, path.basename(datasetPath));
    formData.append('purpose', 'fine-tune');

    // Upload dataset via DeepSeek API
    const response = await globalThis.fetch(`${this.config.baseUrl}/v1/files`, {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${apiKey}`
      },
      body: formData as unknown as BodyInit
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to upload dataset: ${response.status} ${error}`);
    }

    const data = await response.json() as { id: string };
    return data.id; // Dataset ID
  }

  /**
   * Create fine-tuning job via API
   * @private
   */
  private async createFineTuningJob(
    request: LoRATrainingRequest,
    datasetId: string,
    apiKey: string
  ): Promise<string> {
    // Create fine-tuning job
    const response = await globalThis.fetch(`${this.config.baseUrl}/v1/fine_tuning/jobs`, {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${apiKey}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        training_file: datasetId,
        model: request.baseModel ?? 'deepseek-chat',
        hyperparameters: {
          n_epochs: request.epochs ?? 3,
          learning_rate_multiplier: (request.learningRate ?? 0.0001) * 10000, // API uses multiplier
          batch_size: request.batchSize ?? 4
        }
      })
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to create fine-tuning job: ${response.status} ${error}`);
    }

    const data = await response.json() as { id: string };
    return data.id; // Job ID
  }

  /**
   * Monitor training job until complete
   * @private
   */
  private async monitorTrainingJob(jobId: string, apiKey: string): Promise<{ finalLoss: number }> {
    // Poll job status every 10 seconds
    let attempts = 0;
    const maxAttempts = 360; // 1 hour max (10s * 360 = 3600s)

    while (attempts < maxAttempts) {
      const response = await globalThis.fetch(`${this.config.baseUrl}/v1/fine_tuning/jobs/${jobId}`, {
        headers: { 'Authorization': `Bearer ${apiKey}` }
      });

      if (!response.ok) {
        throw new Error(`Failed to check job status: ${response.status}`);
      }

      const job = await response.json() as {
        status: string;
        result_files?: Array<{ metrics?: { final_loss?: number } }>;
        error?: { message?: string };
      };

      if (job.status === 'succeeded') {
        return {
          finalLoss: job.result_files?.[0]?.metrics?.final_loss ?? 0.5
        };
      } else if (job.status === 'failed' || job.status === 'cancelled') {
        throw new Error(`Training ${job.status}: ${job.error?.message ?? 'Unknown error'}`);
      }

      // Log progress
      this.log('debug', `   Status: ${job.status} (attempt ${attempts + 1}/${maxAttempts})`);

      // Wait 10 seconds before next poll
      await new Promise(resolve => setTimeout(resolve, 10000));
      attempts++;
    }

    throw new Error('Training timeout: exceeded 1 hour');
  }

  /**
   * Get trained model ID
   * @private
   */
  private async getTrainedModelId(jobId: string, apiKey: string): Promise<string> {
    const response = await globalThis.fetch(`${this.config.baseUrl}/v1/fine_tuning/jobs/${jobId}`, {
      headers: { 'Authorization': `Bearer ${apiKey}` }
    });

    if (!response.ok) {
      throw new Error(`Failed to get model ID: ${response.status}`);
    }

    const job = await response.json() as { fine_tuned_model: string };
    return job.fine_tuned_model; // Model ID for inference
  }

  /**
   * Save adapter metadata
   * @private
   */
  private async saveAdapterMetadata(
    request: LoRATrainingRequest,
    modelId: string,
    metrics: { finalLoss: number }
  ): Promise<string> {
    const metadataPath = path.join(
      '.continuum/genome/adapters',
      `${request.baseModel ?? 'deepseek-chat'}-${request.traitType}-${Date.now()}.json`
    );

    await fs.promises.mkdir(path.dirname(metadataPath), { recursive: true });

    await fs.promises.writeFile(
      metadataPath,
      JSON.stringify({
        provider: 'deepseek',
        modelId,
        baseModel: request.baseModel,
        traitType: request.traitType,
        personaId: request.personaId,
        personaName: request.personaName,
        metrics,
        createdAt: Date.now()
      }, null, 2)
    );

    return metadataPath;
  }

  /**
   * Clean up temporary files
   * @private
   */
  private async cleanupTempFiles(...paths: string[]): Promise<void> {
    for (const filePath of paths) {
      try {
        await fs.promises.unlink(filePath);
      } catch (error) {
        this.log('warn', `Failed to clean up temp file: ${filePath} - ${error instanceof Error ? error.message : String(error)}`);
      }
    }
  }
}
