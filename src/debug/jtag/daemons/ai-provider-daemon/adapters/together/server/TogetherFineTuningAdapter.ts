/**
 * TogetherLoRAAdapter - Remote API fine-tuning adapter
 *
 * REMOTE API STRATEGY:
 * - Uses Together AI API for cloud-based LoRA training
 * - No local GPU required
 * - Supports Llama 3.1, Qwen, and other open-source models
 *
 * Based on TogetherLoRAAdapter with Together-specific changes:
 * - API base: https://api.together.xyz/v1
 * - Requires lora: true parameter
 * - Returns output_name instead of fine_tuned_model
 *
 * SERVER-ONLY: Uses Node.js for HTTP requests and file system
 */

import { BaseLoRATrainerServer } from '../../../../../system/genome/fine-tuning/server/BaseLoRATrainerServer';
import type {
  LoRATrainingRequest,
  FineTuningCapabilities,
  FineTuningStrategy,
  TrainingDataset,
  TrainingHandle,
  TrainingStatus
} from '../../../../../system/genome/fine-tuning/shared/FineTuningTypes';
import type { UUID } from '../../../../../system/core/types/CrossPlatformUUID';
import { TogetherBaseConfig } from '../shared/TogetherBaseConfig';
import { PATHS } from '../../../../../system/shared/Constants';
import * as fs from 'fs';
import * as path from 'path';

// Declare globals (Node.js 18+ built-ins)
declare const fetch: typeof globalThis.fetch;
/* eslint-disable @typescript-eslint/naming-convention */
declare const FormData: typeof globalThis.FormData;
declare const Blob: typeof globalThis.Blob;
/* eslint-enable @typescript-eslint/naming-convention */

/**
 * Together LoRA Adapter - Remote API training with Together
 *
 * Status: ✅ REFACTORED with async handle pattern
 * Architecture: Implements _startTraining() and _queryStatus() primitives
 * Architecture: Uses TogetherBaseConfig for shared state with inference adapter
 */
export class TogetherLoRAAdapter extends BaseLoRATrainerServer {
  readonly providerId = 'together';
  private readonly config: TogetherBaseConfig;

  constructor(apiKey?: string) {
    super();
    this.config = new TogetherBaseConfig(apiKey);
  }

  /**
   * Check if Together supports fine-tuning
   * Requires TOGETHER_API_KEY in SecretManager
   */
  supportsFineTuning(): boolean {
    return this.config.hasApiKey();
  }

  /**
   * Get fine-tuning capabilities
   *
   * Together capabilities (remote API training):
   * - LoRA rank: Fixed by provider (not directly configurable)
   * - Epochs: 1-50 (default: 3)
   * - No GPU required (cloud-based)
   * - Requires internet connection
   * - Minimum 10 training examples required
   * - Supports GPT-3.5-turbo, GPT-4, GPT-4o, GPT-4o-mini models
   */
  getFineTuningCapabilities(): FineTuningCapabilities {
    return {
      supportsFineTuning: this.supportsFineTuning(),
      strategy: this.getFineTuningStrategy(),

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

      // Cost (per example)
      // Average example = ~100 tokens input + ~50 tokens output = ~$0.00405 per example
      costPerExample: 0.00405,

      // Performance (API latency + training time)
      estimatedTrainingTime: 800, // 800ms per example per epoch (API overhead)

      // Model support (Together models)
      supportedBaseModels: [
        'meta-llama/Meta-Llama-3.1-8B-Instruct-Reference',
        'meta-llama/Meta-Llama-3.1-70B-Instruct-Reference',
        'mistralai/Mixtral-8x7B-Instruct-v0.1',
        'Qwen/Qwen2.5-7B-Instruct'
      ],

      // Requirements
      requiresGPU: false, // Cloud-based training
      requiresInternet: true // API calls
    };
  }

  // ==================== ASYNC PRIMITIVES ====================

  /**
   * Start training - Returns handle immediately (FAST!)
   *
   * Steps:
   * 1. Export dataset to JSONL
   * 2. Upload to Together
   * 3. Create fine-tuning job
   * 4. Return handle with jobId and fileId
   *
   * NO BLOCKING - Returns in seconds, not minutes!
   */
  /* eslint-disable @typescript-eslint/naming-convention */
  protected async _startTraining(request: LoRATrainingRequest): Promise<TrainingHandle> {
  /* eslint-enable @typescript-eslint/naming-convention */
    console.log('🚀 Together: Starting training job (async pattern)...');

    // 1. Export dataset to JSONL
    console.log('   Exporting dataset...');
    const datasetPath = await this.exportDatasetToJSONL(request.dataset);
    console.log(`   Dataset exported: ${datasetPath}`);

    // 2. Upload dataset to Together
    console.log('   Uploading to Together...');
    const fileId = await this.uploadDataset(datasetPath);
    console.log(`   File ID: ${fileId}`);

    // 3. Create fine-tuning job
    console.log('   Creating training job...');
    const jobId = await this.createFineTuningJob(request, fileId);
    console.log(`   Job ID: ${jobId}`);

    // 4. Clean up temp file immediately
    await this.cleanupTempFiles(datasetPath);

    // 5. Return handle (training continues on Together servers!)
    return {
      jobId,
      fileId,
      metadata: {
        baseModel: request.baseModel,
        epochs: request.epochs ?? 3
      }
    };
  }

  /**
   * Query training status - Returns current status (FAST!)
   *
   * Steps:
   * 1. Query Together API for job status
   * 2. Map Together status to our status enum
   * 3. Return current status
   *
   * NO BLOCKING - Returns in < 5 seconds!
   */
  /* eslint-disable @typescript-eslint/naming-convention */
  protected async _queryStatus(
    _sessionId: UUID,
    providerJobId: string,
    _metadata: Record<string, unknown>
  ): Promise<TrainingStatus> {
  /* eslint-enable @typescript-eslint/naming-convention */
    console.log(`🔍 Together: Querying job status: ${providerJobId}`);

    if (!this.config.hasApiKey()) {
      return {
        status: 'failed',
        error: 'TOGETHER_API_KEY not configured'
      };
    }

    try {
      const response = await fetch(
        `${this.config.baseUrl}/v1/fine-tunes/${providerJobId}`,
        {
          method: 'GET',
          headers: {
            'Authorization': `Bearer ${this.config.apiKey}`,
            'Content-Type': 'application/json'
          }
        }
      );

      if (!response.ok) {
        const errorText = await response.text();
        return {
          status: 'failed',
          error: `Together API error: ${response.status} - ${errorText}`
        };
      }

      const job = await response.json();

      // Map Together status to our status
      const status = this.mapTogetherStatus(job.status);

      return {
        status,
        modelId: job.output_name ?? undefined,
        error: job.error?.message,
        metadata: {
          togetherStatus: job.status,
          createdAt: job.created_at,
          finishedAt: job.finished_at,
          trainedTokens: job.trained_tokens
        }
      };
    } catch (error) {
      console.error(`❌ Together: Failed to query status:`, error);
      return {
        status: 'failed',
        error: error instanceof Error ? error.message : 'Unknown error'
      };
    }
  }

  /**
   * Map Together status to our status enum
   */
  private mapTogetherStatus(openaiStatus: string): 'pending' | 'running' | 'completed' | 'failed' | 'cancelled' {
    switch (openaiStatus) {
      case 'validating_files':
      case 'queued':
        return 'pending';
      case 'running':
        return 'running';
      case 'succeeded':
        return 'completed';
      case 'failed':
        return 'failed';
      case 'cancelled':
        return 'cancelled';
      default:
        console.warn(`Unknown Together status: ${openaiStatus}, treating as running`);
        return 'running';
    }
  }

  /**
   * Get training strategy (remote API)
   */
  getFineTuningStrategy(): FineTuningStrategy {
    return 'remote-api';
  }

  /**
   * Estimate training cost (Together pricing)
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

  // ==================== IMPLEMENTATION ====================

  /**
   * Export dataset to JSONL file
   * @private
   */
  private async exportDatasetToJSONL(dataset: TrainingDataset): Promise<string> {
    // Use .continuum/media/temp to avoid filling up primary drive
    const tempDir = PATHS.MEDIA_TEMP;
    await fs.promises.mkdir(tempDir, { recursive: true });
    const tempPath = path.join(tempDir, `together-training-${Date.now()}.jsonl`);

    // Convert dataset to JSONL format
    const lines = dataset.examples.map(example => {
      if ('messages' in example) {
        return JSON.stringify({ messages: example.messages });
      } else {
        return JSON.stringify({ messages: example });
      }
    });

    const jsonl = lines.join('\n');
    await fs.promises.writeFile(tempPath, jsonl, 'utf-8');

    return tempPath;
  }

  /**
   * Upload dataset to Together API
   * @private
   */
  private async uploadDataset(datasetPath: string): Promise<string> {
    if (!this.config.hasApiKey()) {
      throw new Error(
        'TOGETHER_API_KEY not configured. ' +
        'Please add it to ~/.continuum/config.env:\n' +
        'TOGETHER_API_KEY=your-key-here'
      );
    }

    // Upload dataset via Together API
    // POST https://api.together.xyz/v1/files/upload
    // Together requires THREE fields: file, file_name, and purpose
    const fileContent = await fs.promises.readFile(datasetPath, 'utf-8');
    const blob = new Blob([fileContent], { type: 'application/jsonl' });
    const filename = path.basename(datasetPath);

    const formData = new FormData();
    formData.append('file', blob, filename);
    formData.append('file_name', filename);  // REQUIRED - Together needs this separately!
    formData.append('purpose', 'fine-tune');

    const response = await fetch(`${this.config.baseUrl}/v1/files/upload`, {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${this.config.apiKey}`
        // Don't set Content-Type - let FormData set it with boundary
      },
      body: formData
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`File upload failed: ${response.status} ${error}`);
    }

    const data: { id: string } = await response.json() as { id: string };
    return data.id; // File ID
  }

  /**
   * Create fine-tuning job via API
   * @private
   */
  private async createFineTuningJob(
    request: LoRATrainingRequest,
    fileId: string
  ): Promise<string> {
    // Create fine-tuning job
    // POST https://api.together.xyz/v1/fine-tunes (NOT fine_tuning/jobs!)
    // Together API uses simpler format than OpenAI
    const response = await fetch(`${this.config.baseUrl}/v1/fine-tunes`, {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${this.config.apiKey}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        training_file: fileId,
        model: request.baseModel ?? 'meta-llama/Meta-Llama-3.1-8B-Instruct-Reference'
      })
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Job creation failed: ${response.status} ${error}`);
    }

    const data: { id: string } = await response.json() as { id: string };
    return data.id; // Job ID
  }

  /**
   * Monitor training job until complete
   * Returns the fine-tuned model ID
   * @private
   * @deprecated This method is not used with async handle pattern - use _queryStatus() instead
   */
  private async monitorTrainingJob(jobId: string): Promise<string> {
    const maxAttempts = 120; // 10 minutes max (5s * 120 = 600s)
    let attempts = 0;

    // Poll job status every 5 seconds
    while (attempts < maxAttempts) {
      attempts++;

      try {
        const response = await fetch(`${this.config.baseUrl}/v1/fine_tuning/jobs/${jobId}`, {
          headers: { 'Authorization': `Bearer ${this.config.apiKey}` }
        });

        if (!response.ok) {
          console.warn(`   Poll attempt ${attempts}: HTTP ${response.status}`);
          await new Promise(resolve => setTimeout(resolve, 5000));
          continue;
        }

        const job: {
          status: string;
          fine_tuned_model?: string;
          error?: { message: string };
        } = await response.json() as {
          status: string;
          fine_tuned_model?: string;
          error?: { message: string };
        };

        console.log(`   Status: ${job.status} (attempt ${attempts}/${maxAttempts})`);

        if (job.status === 'succeeded') {
          if (!job.fine_tuned_model) {
            throw new Error('Training succeeded but no model ID returned');
          }
          return job.fine_tuned_model;
        } else if (job.status === 'failed') {
          const errorMsg = job.error?.message ?? 'unknown error';
          throw new Error(`Training failed: ${errorMsg}`);
        }

        // Wait 5 seconds before next poll
        await new Promise(resolve => setTimeout(resolve, 5000));

      } catch (error) {
        // Socket timeout or network error - job continues on server
        if (error instanceof Error && error.message.includes('fetch failed')) {
          console.warn(`   Socket error (attempt ${attempts}), job continues on server...`);
          await new Promise(resolve => setTimeout(resolve, 5000));
          continue;
        }
        throw error;
      }
    }

    throw new Error(`Training timeout after ${maxAttempts} attempts (10 minutes)`);
  }

  /**
   * Save adapter metadata to permanent storage
   * @protected
   */
  protected async saveAdapterMetadata(
    request: LoRATrainingRequest,
    modelId: string,
    fileId: string,
    jobId: string,
    outputPath?: string
  ): Promise<string> {
    const timestamp = Date.now();
    const baseModelSafe = (request.baseModel ?? 'gpt-4o-mini').replace(/[:/]/g, '-');
    const adapterFilename = `${baseModelSafe}-${request.traitType}-${timestamp}.json`;

    // Use provided output path or default
    const permanentDir = outputPath ?? path.join('.continuum', 'genome', 'adapters');

    // Ensure permanent directory exists
    await fs.promises.mkdir(permanentDir, { recursive: true });

    const destPath = path.join(permanentDir, adapterFilename);

    // Save adapter metadata (not the model itself - that's on Together)
    const metadata = {
      providerId: this.providerId,
      modelId,
      baseModel: request.baseModel ?? 'meta-llama/Meta-Llama-3.1-8B-Instruct-Reference',
      traitType: request.traitType,
      personaId: request.personaId,
      personaName: request.personaName,
      fileId,
      jobId,
      examplesCount: request.dataset.examples.length,
      epochs: request.epochs ?? 3,
      createdAt: timestamp
    };

    await fs.promises.writeFile(destPath, JSON.stringify(metadata, null, 2), 'utf-8');

    return destPath;
  }

  /**
   * Clean up temporary training files
   * @protected
   */
  protected async cleanupTempFiles(datasetPath: string): Promise<void> {
    try {
      await fs.promises.unlink(datasetPath);
    } catch (error) {
      console.warn(`   Failed to clean up temp file: ${datasetPath}`, error);
    }
  }
}
