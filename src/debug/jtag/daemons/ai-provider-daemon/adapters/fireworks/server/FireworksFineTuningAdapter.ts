/**
 * FireworksLoRAAdapter - Remote API fine-tuning adapter
 *
 * REMOTE API STRATEGY:
 * - Uses Fireworks AI API for cloud-based LoRA training
 * - No local GPU required
 * - Supports Llama, Mixtral, Qwen, DeepSeek, and other open-source models
 *
 * KEY DIFFERENCE: Two-step dataset upload:
 * 1. Create dataset record (POST /datasets)
 * 2. Upload file to that dataset (POST /datasets/{id}:upload)
 *
 * UNIQUE FEATURE: Can download trained model weights (.safetensors)!
 * - Unlike OpenAI/Together which lock you into their API
 * - Can version control and share fine-tuned adapters
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
import { getSecret } from '../../../../../system/secrets/SecretManager';
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
 * Fireworks LoRA Adapter - Remote API training with Fireworks
 *
 * Status: ✅ IMPLEMENTED with async handle pattern
 * Architecture: Implements _startTraining() and _queryStatus() primitives
 */
export class FireworksLoRAAdapter extends BaseLoRATrainerServer {
  readonly providerId = 'fireworks';

  /**
   * Check if Fireworks supports fine-tuning
   * Requires FIREWORKS_API_KEY and FIREWORKS_ACCOUNT_ID in SecretManager
   */
  supportsFineTuning(): boolean {
    const apiKey = getSecret('FIREWORKS_API_KEY', 'FireworksLoRAAdapter');
    const accountId = getSecret('FIREWORKS_ACCOUNT_ID', 'FireworksLoRAAdapter');
    return !!apiKey && !!accountId;
  }

  /**
   * Get fine-tuning capabilities
   *
   * Fireworks capabilities (remote API training):
   * - LoRA rank: Configurable 1-256 (default: 16)
   * - Epochs: 1-50 (default: 3)
   * - No GPU required (cloud-based)
   * - Requires internet connection
   * - Minimum 10 training examples required
   * - Supports Llama, Mixtral, Qwen, DeepSeek, and more
   * - UNIQUE: Can download trained model weights!
   */
  getFineTuningCapabilities(): FineTuningCapabilities {
    return {
      supportsFineTuning: this.supportsFineTuning(),
      strategy: this.getFineTuningStrategy(),

      // LoRA parameters (configurable by Fireworks)
      minRank: 1,
      maxRank: 256,
      defaultRank: 16,
      minAlpha: 1,
      maxAlpha: 256,
      defaultAlpha: 16,

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

      // Cost (per 1M training tokens)
      // Mixtral: $2/1M tokens, minimum $3
      // Llama: Similar pricing
      costPerExample: 0.000002, // ~$0.002 per 1k token example

      // Performance (API latency + training time)
      estimatedTrainingTime: 600, // 600ms per example per epoch

      // Model support (Fireworks models)
      supportedBaseModels: [
        'accounts/fireworks/models/llama-v3-8b-instruct',
        'accounts/fireworks/models/llama-v3-70b-instruct',
        'accounts/fireworks/models/llama-v3p1-8b-instruct',
        'accounts/fireworks/models/llama-v3p1-70b-instruct',
        'accounts/fireworks/models/mixtral-8x7b-instruct',
        'accounts/fireworks/models/qwen2-72b-instruct'
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
   * 2. Create dataset record on Fireworks
   * 3. Upload file to dataset
   * 4. Create fine-tuning job
   * 5. Return handle with jobId and datasetId
   *
   * NO BLOCKING - Returns in seconds, not minutes!
   */
  /* eslint-disable @typescript-eslint/naming-convention */
  protected async _startTraining(request: LoRATrainingRequest): Promise<TrainingHandle> {
  /* eslint-enable @typescript-eslint/naming-convention */
    console.log('🚀 Fireworks: Starting training job (async pattern)...');

    // 1. Export dataset to JSONL
    console.log('   Exporting dataset...');
    const datasetPath = await this.exportDatasetToJSONL(request.dataset);
    console.log(`   Dataset exported: ${datasetPath}`);

    // 2. Create dataset record
    console.log('   Creating dataset record...');
    const datasetId = await this.createDatasetRecord(request);
    console.log(`   Dataset ID: ${datasetId}`);

    // 3. Upload dataset file
    console.log('   Uploading dataset file...');
    await this.uploadDatasetFile(datasetId, datasetPath);
    console.log(`   Upload complete`);

    // 4. Wait for dataset to be READY
    console.log('   Waiting for dataset validation...');
    await this.waitForDatasetReady(datasetId);
    console.log(`   Dataset ready`);

    // 5. Create fine-tuning job
    console.log('   Creating training job...');
    const jobId = await this.createFineTuningJob(request, datasetId);
    console.log(`   Job ID: ${jobId}`);

    // 6. Clean up temp file immediately
    await this.cleanupTempFiles(datasetPath);

    // 7. Return handle (training continues on Fireworks servers!)
    return {
      jobId,
      fileId: datasetId, // Use datasetId as fileId for consistency
      metadata: {
        baseModel: request.baseModel,
        epochs: request.epochs ?? 3,
        datasetId
      }
    };
  }

  /**
   * Query training status - Returns current status (FAST!)
   *
   * Steps:
   * 1. Query Fireworks API for job status
   * 2. Map Fireworks status to our status enum
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
    console.log(`🔍 Fireworks: Querying job status: ${providerJobId}`);

    const apiKey = getSecret('FIREWORKS_API_KEY', 'FireworksLoRAAdapter');
    const accountId = getSecret('FIREWORKS_ACCOUNT_ID', 'FireworksLoRAAdapter');

    if (!apiKey || !accountId) {
      return {
        status: 'failed',
        error: 'FIREWORKS_API_KEY or FIREWORKS_ACCOUNT_ID not configured'
      };
    }

    try {
      const response = await fetch(
        `https://api.fireworks.ai/v1/accounts/${accountId}/supervisedFineTuningJobs/${providerJobId}`,
        {
          method: 'GET',
          headers: {
            'Authorization': `Bearer ${apiKey}`,
            'Content-Type': 'application/json'
          }
        }
      );

      if (!response.ok) {
        const errorText = await response.text();
        return {
          status: 'failed',
          error: `Fireworks API error: ${response.status} - ${errorText}`
        };
      }

      const job = await response.json();

      // Map Fireworks status to our status
      const status = this.mapFireworksStatus(job.status?.state ?? job.state);

      return {
        status,
        modelId: job.outputModel ?? undefined,
        error: job.status?.message,
        metadata: {
          fireworksStatus: job.status?.state ?? job.state,
          createdAt: job.createTime,
          completedAt: job.completedTime,
          outputModel: job.outputModel
        }
      };
    } catch (error) {
      console.error(`❌ Fireworks: Failed to query status:`, error);
      return {
        status: 'failed',
        error: error instanceof Error ? error.message : 'Unknown error'
      };
    }
  }

  /**
   * Map Fireworks status to our status enum
   */
  private mapFireworksStatus(fireworksStatus: string): 'pending' | 'running' | 'completed' | 'failed' | 'cancelled' {
    switch (fireworksStatus) {
      case 'CREATING':
      case 'PENDING':
      case 'VALIDATING':
        return 'pending';
      case 'RUNNING':
      case 'WRITING_RESULTS':
        return 'running';
      case 'COMPLETED':
        return 'completed';
      case 'FAILED':
        return 'failed';
      case 'CANCELLED':
        return 'cancelled';
      default:
        console.warn(`Unknown Fireworks status: ${fireworksStatus}, treating as running`);
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
   * Estimate training cost (Fireworks pricing)
   *
   * Pricing: $2/1M training tokens (minimum $3)
   * Assumption: Average example = 100 tokens input + 50 tokens output = 150 tokens
   */
  estimateTrainingCost(exampleCount: number): number {
    const capabilities = this.getFineTuningCapabilities();

    // Use capability metadata for cost estimation
    if (capabilities.costPerExample) {
      return Math.max(3.0, exampleCount * capabilities.costPerExample * 150); // 150 tokens per example
    }

    // Fallback: Conservative estimate
    // $2/1M tokens, 150 tokens per example
    return Math.max(3.0, exampleCount * 150 * 0.000002);
  }

  /**
   * Estimate training time
   *
   * Assumptions:
   * - API latency + training time: ~600ms per example per epoch
   * - Includes upload, processing, validation
   */
  estimateTrainingTime(exampleCount: number, epochs: number): number {
    const capabilities = this.getFineTuningCapabilities();

    // Use API estimate from capabilities
    if (capabilities.estimatedTrainingTime) {
      return exampleCount * epochs * capabilities.estimatedTrainingTime;
    }

    // Fallback: Conservative estimate
    return exampleCount * epochs * 600; // 600ms per example per epoch
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
    const tempPath = path.join(tempDir, `fireworks-training-${Date.now()}.jsonl`);

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
   * Create dataset record on Fireworks
   * Returns dataset ID
   * @private
   */
  private async createDatasetRecord(request: LoRATrainingRequest): Promise<string> {
    const apiKey = getSecret('FIREWORKS_API_KEY', 'FireworksLoRAAdapter');
    const accountId = getSecret('FIREWORKS_ACCOUNT_ID', 'FireworksLoRAAdapter');

    if (!apiKey || !accountId) {
      throw new Error(
        'FIREWORKS_API_KEY or FIREWORKS_ACCOUNT_ID not configured. ' +
        'Please add them to ~/.continuum/config.env:\n' +
        'FIREWORKS_API_KEY=your-key-here\n' +
        'FIREWORKS_ACCOUNT_ID=your-account-id'
      );
    }

    // Create dataset record
    // POST https://api.fireworks.ai/v1/accounts/{account_id}/datasets
    const datasetId = `training-${request.personaId}-${Date.now()}`;

    const response = await fetch(
      `https://api.fireworks.ai/v1/accounts/${accountId}/datasets`,
      {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${apiKey}`,
          'Content-Type': 'application/json'
        },
        body: JSON.stringify({
          datasetId: datasetId,
          dataset: {
            userUploaded: {}
          }
        })
      }
    );

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Dataset creation failed: ${response.status} ${error}`);
    }

    const data = await response.json() as any;
    // API returns the datasetId we sent in
    return datasetId;
  }

  /**
   * Upload dataset file to Fireworks
   * @private
   */
  private async uploadDatasetFile(datasetId: string, datasetPath: string): Promise<void> {
    const apiKey = getSecret('FIREWORKS_API_KEY', 'FireworksLoRAAdapter');
    const accountId = getSecret('FIREWORKS_ACCOUNT_ID', 'FireworksLoRAAdapter');

    // Upload dataset file
    // POST https://api.fireworks.ai/v1/accounts/{account_id}/datasets/{dataset_id}:upload
    const fileContent = await fs.promises.readFile(datasetPath, 'utf-8');
    const blob = new Blob([fileContent], { type: 'application/jsonl' });

    const formData = new FormData();
    formData.append('file', blob, path.basename(datasetPath));

    const response = await fetch(
      `https://api.fireworks.ai/v1/accounts/${accountId}/datasets/${datasetId}:upload`,
      {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${apiKey}`
          // Don't set Content-Type - let FormData set it with boundary
        },
        body: formData
      }
    );

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Dataset upload failed: ${response.status} ${error}`);
    }
  }

  /**
   * Wait for dataset to be READY
   * @private
   */
  private async waitForDatasetReady(datasetId: string): Promise<void> {
    const apiKey = getSecret('FIREWORKS_API_KEY', 'FireworksLoRAAdapter');
    const accountId = getSecret('FIREWORKS_ACCOUNT_ID', 'FireworksLoRAAdapter');

    const maxAttempts = 60; // 5 minutes (5s * 60)
    let attempts = 0;

    while (attempts < maxAttempts) {
      attempts++;

      const response = await fetch(
        `https://api.fireworks.ai/v1/accounts/${accountId}/datasets/${datasetId}`,
        {
          method: 'GET',
          headers: {
            'Authorization': `Bearer ${apiKey}`,
            'Content-Type': 'application/json'
          }
        }
      );

      if (!response.ok) {
        console.warn(`   Dataset check attempt ${attempts}: HTTP ${response.status}`);
        await new Promise(resolve => setTimeout(resolve, 5000));
        continue;
      }

      const dataset: { state: string } = await response.json() as { state: string };

      if (dataset.state === 'READY') {
        return;
      } else if (dataset.state === 'FAILED') {
        throw new Error('Dataset validation failed');
      }

      // Wait 5 seconds before next check
      await new Promise(resolve => setTimeout(resolve, 5000));
    }

    throw new Error('Dataset validation timeout after 5 minutes');
  }

  /**
   * Create fine-tuning job via API
   * @private
   */
  private async createFineTuningJob(
    request: LoRATrainingRequest,
    datasetId: string
  ): Promise<string> {
    const apiKey = getSecret('FIREWORKS_API_KEY', 'FireworksLoRAAdapter');
    const accountId = getSecret('FIREWORKS_ACCOUNT_ID', 'FireworksLoRAAdapter');

    const capabilities = this.getFineTuningCapabilities();
    const epochs = request.epochs ?? capabilities.defaultEpochs ?? 3;

    // Create fine-tuning job
    // POST https://api.fireworks.ai/v1/accounts/{account_id}/supervisedFineTuningJobs
    const response = await fetch(
      `https://api.fireworks.ai/v1/accounts/${accountId}/supervisedFineTuningJobs`,
      {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${apiKey}`,
          'Content-Type': 'application/json'
        },
        body: JSON.stringify({
          baseModel: request.baseModel ?? 'accounts/fireworks/models/llama-v3p1-8b-instruct',
          dataset: `accounts/${accountId}/datasets/${datasetId}`,
          displayName: `${request.personaName} - ${request.traitType}`,
          epochs,
          loraRank: request.rank ?? capabilities.defaultRank ?? 16
        })
      }
    );

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Job creation failed: ${response.status} ${error}`);
    }

    const data: { name: string } = await response.json() as { name: string };
    // Extract just the job ID from the full path (e.g., "accounts/continuum/supervisedFineTuningJobs/zupjmr19" -> "zupjmr19")
    return data.name.split('/').pop() ?? data.name;
  }

  /**
   * Save adapter metadata to permanent storage
   * @protected
   */
  protected async saveAdapterMetadata(
    request: LoRATrainingRequest,
    modelId: string,
    datasetId: string,
    jobId: string,
    outputPath?: string
  ): Promise<string> {
    const timestamp = Date.now();
    const baseModelSafe = (request.baseModel ?? 'llama-v3p1-8b').replace(/[:/]/g, '-');
    const adapterFilename = `${baseModelSafe}-${request.traitType}-${timestamp}.json`;

    // Use provided output path or default
    const permanentDir = outputPath ?? path.join('.continuum', 'genome', 'adapters');

    // Ensure permanent directory exists
    await fs.promises.mkdir(permanentDir, { recursive: true });

    const destPath = path.join(permanentDir, adapterFilename);

    // Save adapter metadata
    const metadata = {
      providerId: this.providerId,
      modelId,
      baseModel: request.baseModel ?? 'accounts/fireworks/models/llama-v3p1-8b-instruct',
      traitType: request.traitType,
      personaId: request.personaId,
      personaName: request.personaName,
      datasetId,
      jobId,
      examplesCount: request.dataset.examples.length,
      epochs: request.epochs ?? 3,
      loraRank: request.rank ?? 16,
      createdAt: timestamp,
      downloadable: true // UNIQUE: Fireworks allows downloading model weights!
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
