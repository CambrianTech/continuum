/**
 * OpenAILoRAAdapter - Remote API fine-tuning adapter
 *
 * REMOTE API STRATEGY:
 * - Uses OpenAI API for cloud-based LoRA training
 * - No local GPU required
 * - Supports GPT-3.5-turbo, GPT-4, GPT-4o, GPT-4o-mini models
 *
 * Implementation proven in /tmp/prototype-finetune-all.ts
 * Successfully created model: ft:gpt-4o-mini-2024-07-18:personal::CbScXmaV
 *
 * SERVER-ONLY: Uses Node.js for HTTP requests and file system
 */

import { BaseLoRATrainerServer } from '../BaseLoRATrainerServer';
import type {
  LoRATrainingRequest,
  FineTuningCapabilities,
  FineTuningStrategy,
  TrainingDataset,
  TrainingHandle,
  TrainingStatus
} from '../../shared/FineTuningTypes';
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
 * OpenAI LoRA Adapter - Remote API training with OpenAI
 *
 * Status: ✅ REFACTORED with async handle pattern
 * Architecture: Implements _startTraining() and _queryStatus() primitives
 */
export class OpenAILoRAAdapter extends BaseLoRATrainerServer {
  readonly providerId = 'openai';

  /**
   * Check if OpenAI supports fine-tuning
   * Requires OPENAI_API_KEY in SecretManager
   */
  supportsFineTuning(): boolean {
    const apiKey = getSecret('OPENAI_API_KEY', 'OpenAILoRAAdapter');
    return !!apiKey;
  }

  /**
   * Get fine-tuning capabilities
   *
   * OpenAI capabilities (remote API training):
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

      // Model support (OpenAI models)
      supportedBaseModels: [
        'gpt-3.5-turbo',
        'gpt-4',
        'gpt-4-turbo',
        'gpt-4o',
        'gpt-4o-mini-2024-07-18'
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
   * 2. Upload to OpenAI
   * 3. Create fine-tuning job
   * 4. Return handle with jobId and fileId
   *
   * NO BLOCKING - Returns in seconds, not minutes!
   */
  /* eslint-disable @typescript-eslint/naming-convention */
  protected async _startTraining(request: LoRATrainingRequest): Promise<TrainingHandle> {
  /* eslint-enable @typescript-eslint/naming-convention */
    console.log('🚀 OpenAI: Starting training job (async pattern)...');

    // 1. Export dataset to JSONL
    console.log('   Exporting dataset...');
    const datasetPath = await this.exportDatasetToJSONL(request.dataset);
    console.log(`   Dataset exported: ${datasetPath}`);

    // 2. Upload dataset to OpenAI
    console.log('   Uploading to OpenAI...');
    const fileId = await this.uploadDataset(datasetPath);
    console.log(`   File ID: ${fileId}`);

    // 3. Create fine-tuning job
    console.log('   Creating training job...');
    const jobId = await this.createFineTuningJob(request, fileId);
    console.log(`   Job ID: ${jobId}`);

    // 4. Clean up temp file immediately
    await this.cleanupTempFiles(datasetPath);

    // 5. Return handle (training continues on OpenAI servers!)
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
   * 1. Query OpenAI API for job status
   * 2. Map OpenAI status to our status enum
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
    console.log(`🔍 OpenAI: Querying job status: ${providerJobId}`);

    const apiKey = getSecret('OPENAI_API_KEY', 'OpenAILoRAAdapter');
    if (!apiKey) {
      return {
        status: 'failed',
        error: 'OPENAI_API_KEY not configured'
      };
    }

    try {
      const response = await fetch(
        `https://api.openai.com/v1/fine_tuning/jobs/${providerJobId}`,
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
          error: `OpenAI API error: ${response.status} - ${errorText}`
        };
      }

      const job = await response.json();

      // Map OpenAI status to our status
      const status = this.mapOpenAIStatus(job.status);

      return {
        status,
        modelId: job.fine_tuned_model ?? undefined,
        error: job.error?.message,
        metadata: {
          openaiStatus: job.status,
          createdAt: job.created_at,
          finishedAt: job.finished_at,
          trainedTokens: job.trained_tokens
        }
      };
    } catch (error) {
      console.error(`❌ OpenAI: Failed to query status:`, error);
      return {
        status: 'failed',
        error: error instanceof Error ? error.message : 'Unknown error'
      };
    }
  }

  /**
   * Map OpenAI status to our status enum
   */
  private mapOpenAIStatus(openaiStatus: string): 'pending' | 'running' | 'completed' | 'failed' | 'cancelled' {
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
        console.warn(`Unknown OpenAI status: ${openaiStatus}, treating as running`);
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

  // ==================== IMPLEMENTATION ====================

  /**
   * Export dataset to JSONL file
   * @private
   */
  private async exportDatasetToJSONL(dataset: TrainingDataset): Promise<string> {
    // Use .continuum/media/temp to avoid filling up primary drive
    const tempDir = PATHS.MEDIA_TEMP;
    await fs.promises.mkdir(tempDir, { recursive: true });
    const tempPath = path.join(tempDir, `openai-training-${Date.now()}.jsonl`);

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
   * Upload dataset to OpenAI API
   * @private
   */
  private async uploadDataset(datasetPath: string): Promise<string> {
    const apiKey = getSecret('OPENAI_API_KEY', 'OpenAILoRAAdapter');
    if (!apiKey) {
      throw new Error(
        'OPENAI_API_KEY not configured. ' +
        'Please add it to ~/.continuum/config.env:\n' +
        'OPENAI_API_KEY=your-key-here'
      );
    }

    // Upload dataset via OpenAI API
    // POST https://api.openai.com/v1/files
    const fileContent = await fs.promises.readFile(datasetPath, 'utf-8');
    const blob = new Blob([fileContent], { type: 'application/json' });

    const formData = new FormData();
    formData.append('file', blob, 'training.jsonl');
    formData.append('purpose', 'fine-tune');

    const response = await fetch('https://api.openai.com/v1/files', {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${apiKey}`
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
    const apiKey = getSecret('OPENAI_API_KEY', 'OpenAILoRAAdapter');

    const capabilities = this.getFineTuningCapabilities();
    const epochs = request.epochs ?? capabilities.defaultEpochs ?? 3;

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
        model: request.baseModel ?? 'gpt-4o-mini-2024-07-18',
        hyperparameters: {
          n_epochs: epochs
        }
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
   */
  private async monitorTrainingJob(jobId: string): Promise<string> {
    const apiKey = getSecret('OPENAI_API_KEY', 'OpenAILoRAAdapter');

    const maxAttempts = 120; // 10 minutes max (5s * 120 = 600s)
    let attempts = 0;

    // Poll job status every 5 seconds
    while (attempts < maxAttempts) {
      attempts++;

      try {
        const response = await fetch(`https://api.openai.com/v1/fine_tuning/jobs/${jobId}`, {
          headers: { 'Authorization': `Bearer ${apiKey}` }
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

    // Save adapter metadata (not the model itself - that's on OpenAI)
    const metadata = {
      providerId: this.providerId,
      modelId,
      baseModel: request.baseModel ?? 'gpt-4o-mini-2024-07-18',
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
