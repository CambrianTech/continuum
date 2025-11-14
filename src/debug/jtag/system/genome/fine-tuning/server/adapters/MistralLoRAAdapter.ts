/**
 * MistralLoRAAdapter - Remote API fine-tuning adapter
 *
 * REMOTE API STRATEGY:
 * - Uses Mistral AI API for cloud-based LoRA training
 * - No local GPU required
 * - Supports open-mistral-7b, mistral-small-latest, codestral-latest, pixtral-12b-latest
 *
 * API Documentation: https://docs.mistral.ai/capabilities/finetuning/text_vision_finetuning
 *
 * Status Flow: QUEUED → VALIDATED → RUNNING → SUCCESS | FAILED
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
 * Mistral LoRA Adapter - Remote API training with Mistral AI
 *
 * Status: ✅ NEW IMPLEMENTATION with async handle pattern
 * Architecture: Implements _startTraining() and _queryStatus() primitives
 */
export class MistralLoRAAdapter extends BaseLoRATrainerServer {
  readonly providerId = 'mistral';

  /**
   * Check if Mistral supports fine-tuning
   * Requires MISTRAL_API_KEY in SecretManager
   */
  supportsFineTuning(): boolean {
    const apiKey = getSecret('MISTRAL_API_KEY', 'MistralLoRAAdapter');
    return !!apiKey;
  }

  /**
   * Get fine-tuning capabilities
   *
   * Mistral capabilities (remote API training):
   * - LoRA rank: Configurable (default: 16)
   * - Training steps: Configurable (default: 1000)
   * - Learning rate: Configurable (default: 0.0001)
   * - No GPU required (cloud-based)
   * - Requires internet connection
   * - Minimum fee: $4 per job + $2/month storage per model
   * - Supports open-mistral-7b, mistral-small-latest, codestral-latest, pixtral-12b-latest
   */
  getFineTuningCapabilities(): FineTuningCapabilities {
    return {
      supportsFineTuning: this.supportsFineTuning(),
      strategy: this.getFineTuningStrategy(),

      // LoRA parameters (configurable)
      minRank: 4,
      maxRank: 64,
      defaultRank: 16,
      minAlpha: 4,
      maxAlpha: 64,
      defaultAlpha: 16,

      // Training parameters
      minEpochs: 1,
      maxEpochs: 100,
      defaultEpochs: 3,
      minLearningRate: 0.00001,
      maxLearningRate: 0.001,
      defaultLearningRate: 0.0001,
      minBatchSize: 1,
      maxBatchSize: 32,
      defaultBatchSize: 4,

      // Cost ($4 minimum fee + $2/month storage)
      costPerExample: 0.004, // Estimate based on minimum fee

      // Performance (API latency + training time)
      estimatedTrainingTime: 1000, // 1000ms per example per epoch (estimate)

      // Model support (Mistral models)
      supportedBaseModels: [
        'open-mistral-7b',
        'mistral-small-latest',
        'codestral-latest',
        'pixtral-12b-latest'
      ],

      // Removed capabilities field - not part of FineTuningCapabilities interface
    };
  }

  /**
   * Start training - Returns handle immediately (FAST!)
   *
   * Steps:
   * 1. Export dataset to JSONL format
   * 2. Upload training file to Mistral API
   * 3. Create fine-tuning job (auto_start: false)
   * 4. Start job
   * 5. Clean up temp files
   * 6. Return handle (training continues on Mistral servers!)
   *
   * NO BLOCKING - Returns in < 60 seconds!
   */
  protected async _startTraining(
    request: LoRATrainingRequest
  ): Promise<TrainingHandle> {
    console.log('🚀 Mistral: Starting fine-tuning job...');

    // 1. Export dataset to JSONL
    console.log('   Exporting dataset...');
    const datasetPath = await this.exportDatasetToJSONL(request.dataset);
    console.log(`   Dataset exported: ${datasetPath}`);

    // 2. Upload dataset to Mistral
    console.log('   Uploading to Mistral...');
    const fileId = await this.uploadDataset(datasetPath);
    console.log(`   File ID: ${fileId}`);

    // 3. Create fine-tuning job (auto_start: false)
    console.log('   Creating training job...');
    const jobId = await this.createFineTuningJob(request, fileId);
    console.log(`   Job ID: ${jobId}`);

    // 4. Start job
    console.log('   Starting job...');
    await this.startJob(jobId);
    console.log('   Job started!');

    // 5. Clean up temp file immediately
    await this.cleanupTempFiles(datasetPath);

    // 6. Return handle (training continues on Mistral servers!)
    const exampleCount = request.dataset.examples?.length ?? 100;
    const epochs = request.epochs ?? 3;

    return {
      jobId,
      fileId,
      metadata: {
        baseModel: request.baseModel,
        trainingSteps: exampleCount * epochs,
        learningRate: request.learningRate ?? 0.0001
      }
    };
  }

  /**
   * Query training status - Returns current status (FAST!)
   *
   * Steps:
   * 1. Query Mistral API for job status
   * 2. Map Mistral status to our status enum
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
    console.log(`🔍 Mistral: Querying job status: ${providerJobId}`);

    const apiKey = getSecret('MISTRAL_API_KEY', 'MistralLoRAAdapter');
    if (!apiKey) {
      return {
        status: 'failed',
        error: 'MISTRAL_API_KEY not configured'
      };
    }

    try {
      const response = await fetch(
        `https://api.mistral.ai/v1/fine_tuning/jobs/${providerJobId}`,
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
          error: `Mistral API error: ${response.status} - ${errorText}`
        };
      }

      const job = await response.json();

      // Map Mistral status to our status
      const status = this.mapMistralStatus(job.status);

      return {
        status,
        modelId: job.fine_tuned_model ?? undefined,
        error: job.error?.message,
        metadata: {
          mistralStatus: job.status,
          createdAt: job.created_at,
          modifiedAt: job.modified_at,
          trainedTokens: job.trained_tokens
        }
      };
    } catch (error) {
      console.error(`❌ Mistral: Failed to query status:`, error);
      return {
        status: 'failed',
        error: error instanceof Error ? error.message : 'Unknown error'
      };
    }
  }

  /**
   * Map Mistral status to our status enum
   *
   * Mistral statuses:
   * - QUEUED: Initial state after creation
   * - VALIDATED: Ready to start training
   * - RUNNING: Currently training
   * - SUCCESS: Completed successfully
   * - FAILED: Training failed
   * - CANCELLED: User cancelled
   */
  private mapMistralStatus(mistralStatus: string): 'pending' | 'running' | 'completed' | 'failed' | 'cancelled' {
    switch (mistralStatus) {
      case 'QUEUED':
      case 'VALIDATED':
        return 'pending';
      case 'RUNNING':
        return 'running';
      case 'SUCCESS':
        return 'completed';
      case 'FAILED':
        return 'failed';
      case 'CANCELLED':
        return 'cancelled';
      default:
        console.warn(`Unknown Mistral status: ${mistralStatus}, treating as running`);
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
   * Estimate training cost (Mistral pricing)
   *
   * Pricing: $4 minimum fee + $2/month storage per model
   * Conservative estimate: ~$0.004 per example
   */
  estimateTrainingCost(exampleCount: number): number {
    const capabilities = this.getFineTuningCapabilities();

    // Use capability metadata for cost estimation
    if (capabilities.costPerExample) {
      return exampleCount * capabilities.costPerExample;
    }

    // Fallback: Conservative estimate
    return exampleCount * 0.004;
  }

  /**
   * Estimate training time (Mistral cloud training)
   *
   * Conservative estimate: ~1000ms per example per epoch
   */
  estimateTrainingTime(exampleCount: number, epochs: number): number {
    const capabilities = this.getFineTuningCapabilities();

    if (capabilities.estimatedTrainingTime) {
      return exampleCount * epochs * capabilities.estimatedTrainingTime;
    }

    // Fallback: Conservative estimate (1000ms per example per epoch)
    return exampleCount * epochs * 1000;
  }

  // =============================================================================
  // PRIVATE HELPERS
  // =============================================================================

  /**
   * Upload dataset to Mistral API
   *
   * Endpoint: POST https://api.mistral.ai/v1/files
   * Returns: file_id
   */
  private async uploadDataset(datasetPath: string): Promise<string> {
    const apiKey = getSecret('MISTRAL_API_KEY', 'MistralLoRAAdapter');
    if (!apiKey) {
      throw new Error('MISTRAL_API_KEY not configured');
    }

    const formData = new FormData();
    const fileBlob = new Blob([fs.readFileSync(datasetPath)], { type: 'application/jsonl' });
    formData.append('file', fileBlob, 'training.jsonl');
    formData.append('purpose', 'fine-tune');

    const response = await fetch('https://api.mistral.ai/v1/files', {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${apiKey}`
      },
      body: formData
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Mistral file upload failed: ${response.status} - ${error}`);
    }

    const data = await response.json();
    return data.id;
  }

  /**
   * Create fine-tuning job
   *
   * Endpoint: POST https://api.mistral.ai/v1/fine_tuning/jobs
   * Returns: job_id
   */
  private async createFineTuningJob(
    request: LoRATrainingRequest,
    fileId: string
  ): Promise<string> {
    const apiKey = getSecret('MISTRAL_API_KEY', 'MistralLoRAAdapter');
    if (!apiKey) {
      throw new Error('MISTRAL_API_KEY not configured');
    }

    // Convert epochs to training_steps
    // Mistral uses training_steps instead of epochs
    // Estimate: training_steps = examples * epochs
    const exampleCount = request.dataset.examples?.length ?? 100;
    const epochs = request.epochs ?? 3;
    const trainingSteps = exampleCount * epochs;

    const response = await fetch('https://api.mistral.ai/v1/fine_tuning/jobs', {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${apiKey}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        model: request.baseModel,
        training_files: [
          {
            file_id: fileId,
            weight: 1
          }
        ],
        hyperparameters: {
          training_steps: trainingSteps,
          learning_rate: request.learningRate ?? 0.0001
        },
        auto_start: false // We'll start it manually
      })
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Mistral job creation failed: ${response.status} - ${error}`);
    }

    const data = await response.json();
    return data.id;
  }

  /**
   * Start fine-tuning job
   *
   * Endpoint: POST https://api.mistral.ai/v1/fine_tuning/jobs/{job_id}/start
   */
  private async startJob(jobId: string): Promise<void> {
    const apiKey = getSecret('MISTRAL_API_KEY', 'MistralLoRAAdapter');
    if (!apiKey) {
      throw new Error('MISTRAL_API_KEY not configured');
    }

    const response = await fetch(
      `https://api.mistral.ai/v1/fine_tuning/jobs/${jobId}/start`,
      {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${apiKey}`,
          'Content-Type': 'application/json'
        }
      }
    );

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Mistral job start failed: ${response.status} - ${error}`);
    }
  }

  /**
   * Export dataset to JSONL format
   * Mistral expects conversational format with messages array
   */
  private async exportDatasetToJSONL(dataset: TrainingDataset): Promise<string> {
    const tempDir = PATHS.MEDIA_TEMP;
    const tempFile = path.join(tempDir, `mistral-training-${Date.now()}.jsonl`);

    // Ensure temp directory exists
    if (!fs.existsSync(tempDir)) {
      fs.mkdirSync(tempDir, { recursive: true });
    }

    // Convert dataset to JSONL (each line is a JSON object with messages array)
    const lines = (dataset.examples ?? []).map(example => {
      return JSON.stringify(example); // Use example directly - it already has messages array
    });

    fs.writeFileSync(tempFile, lines.join('\n'));
    return tempFile;
  }

  /**
   * Clean up temporary files
   */
  private async cleanupTempFiles(datasetPath: string): Promise<void> {
    try {
      if (fs.existsSync(datasetPath)) {
        fs.unlinkSync(datasetPath);
        console.log(`   Cleaned up temp file: ${datasetPath}`);
      }
    } catch (error) {
      console.warn(`   Failed to clean up temp file: ${error}`);
    }
  }
}
