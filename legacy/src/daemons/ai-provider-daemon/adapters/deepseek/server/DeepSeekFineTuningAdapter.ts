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

import { BaseLoRATrainerServer } from '../../../../../system/genome/fine-tuning/server/BaseLoRATrainerServer';
import type {
  LoRATrainingRequest,
  FineTuningCapabilities,
  FineTuningStrategy,
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
export class DeepSeekLoRAAdapter extends BaseLoRATrainerServer {
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
   * Start training (async pattern primitive)
   *
   * Uploads dataset, creates job, returns handle immediately.
   * NO BLOCKING - returns in seconds!
   */
  protected async _startTraining(
    request: LoRATrainingRequest
  ): Promise<import('../../../../../system/genome/fine-tuning/shared/FineTuningTypes').TrainingHandle> {
    // Check API key
    const apiKey = this.config.apiKey;
    if (!apiKey) {
      throw new Error('DEEPSEEK_API_KEY not configured. Please add it to ~/.continuum/config.env');
    }

    this.log('info', '🚀 DeepSeek: Starting async training job...');

    // 1. Export dataset to JSONL
    this.log('info', '   Exporting dataset to JSONL...');
    const datasetPath = path.join(os.tmpdir(), `deepseek-training-${Date.now()}.jsonl`);
    await this.exportDatasetToJSONL(request.dataset, datasetPath);

    // 2. Upload dataset to DeepSeek
    this.log('info', '   Uploading dataset to DeepSeek API...');
    const fileId = await this.uploadDataset(datasetPath, apiKey);

    // 3. Create fine-tuning job
    this.log('info', '   Creating fine-tuning job...');
    const jobId = await this.createFineTuningJob(request, fileId, apiKey);

    // 4. Clean up temp file
    await this.cleanupTempFiles(datasetPath);

    // Return handle for async monitoring
    return {
      jobId,
      fileId,
      metadata: {
        baseModel: request.baseModel,
        personaId: request.personaId,
        personaName: request.personaName,
        traitType: request.traitType
      }
    };
  }

  /**
   * Query training status (async pattern primitive)
   *
   * Polls DeepSeek API for job status.
   * FAST - returns immediately!
   */
  protected async _queryStatus(
    _sessionId: UUID,
    providerJobId: string,
    _metadata: Record<string, unknown>
  ): Promise<TrainingStatus> {
    const apiKey = this.config.apiKey;
    if (!apiKey) {
      return { status: 'failed', error: 'DEEPSEEK_API_KEY not configured' };
    }

    try {
      const response = await globalThis.fetch(
        `${this.config.baseUrl}/v1/fine_tuning/jobs/${providerJobId}`,
        { headers: { 'Authorization': `Bearer ${apiKey}` } }
      );

      if (!response.ok) {
        return { status: 'failed', error: `API error: ${response.status}` };
      }

      const job = await response.json() as {
        status: string;
        fine_tuned_model?: string;
        result_files?: Array<{ metrics?: { final_loss?: number } }>;
        error?: { message?: string };
      };

      // Map DeepSeek status to our status
      switch (job.status) {
        case 'succeeded':
          return {
            status: 'completed',
            modelId: job.fine_tuned_model,
            metadata: {
              finalLoss: job.result_files?.[0]?.metrics?.final_loss
            }
          };

        case 'failed':
        case 'cancelled':
          return {
            status: 'failed',
            error: job.error?.message ?? `Training ${job.status}`
          };

        case 'running':
        case 'validating_files':
        case 'queued':
        default:
          return {
            status: 'running',
            progress: job.status === 'running' ? 50 : 10  // Rough progress estimate
          };
      }
    } catch (error) {
      return {
        status: 'failed',
        error: error instanceof Error ? error.message : String(error)
      };
    }
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
