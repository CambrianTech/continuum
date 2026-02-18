/**
 * PEFTLoRAAdapter - Local PyTorch/PEFT fine-tuning adapter
 *
 * Philosophy: "Start simple, expand systematically"
 * - Phase 7.1: Basic PEFT integration with standard PyTorch
 * - Phase 7.2+: Multi-model support, optimization, quality improvements
 *
 * LOCAL TRAINING STRATEGY:
 * - Uses standard PyTorch + PEFT via Python subprocess
 * - Universal compatibility (MPS, CUDA, CPU)
 * - No API costs, fully local and private
 * - Supports latest models: SmolLM2, Llama 4, DeepSeek-R1, Qwen3, Gemma 3, Phi-4
 * - Adapter files exported to safetensors format
 *
 * SERVER-ONLY: Uses Node.js for file system and process spawning
 */

import { BaseServerLoRATrainer } from '../BaseServerLoRATrainer';
import type {
  LoRATrainingRequest,
  LoRATrainingResult,
  FineTuningCapabilities,
  FineTuningStrategy,
  TrainingStatus
} from '../../shared/FineTuningTypes';
import type { UUID } from '../../../../../system/core/types/CrossPlatformUUID';
import { LOCAL_MODELS } from '@system/shared/Constants';
import { RustCoreIPCClient } from '../../../../../workers/continuum-core/bindings/RustCoreIPC';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

/**
 * PEFT LoRA Adapter - Local Python-based training with PyTorch + PEFT
 *
 * Current Status: ✅ WORKING (Phase 7.1 complete)
 * - Python 3.11.14 + PyTorch 2.9.0 + PEFT 0.17.1
 * - End-to-end tested with real training
 * - Universal compatibility (MPS, CUDA, CPU)
 */
export class PEFTLoRAAdapter extends BaseServerLoRATrainer {
  readonly providerId = 'peft';

  /**
   * Map short model name to HuggingFace model name.
   * Delegates to LOCAL_MODELS.mapToHuggingFace() — SINGLE SOURCE OF TRUTH.
   */
  private mapModelName(shortName: string): string {
    return LOCAL_MODELS.mapToHuggingFace(shortName);
  }

  // ── Public accessors for async mode (GenomeTrainServerCommand) ────────────

  /** Path to the Python environment wrapper script. */
  get wrapperPath(): string {
    return this.getPythonWrapperPath();
  }

  /** Path to the peft-train.py training script. */
  get scriptPath(): string {
    return this.getTrainingScriptPath('peft-train.py');
  }

  /** Export dataset to temp JSONL for async training. */
  async exportDatasetForAsync(dataset: import('../../shared/FineTuningTypes').TrainingDataset): Promise<string> {
    return this.exportDatasetToJSONL(dataset);
  }

  /** Create config JSON for async training. */
  async createConfigForAsync(request: import('../../shared/FineTuningTypes').LoRATrainingRequest, datasetPath: string): Promise<string> {
    const capabilities = this.getFineTuningCapabilities();
    return this.createConfigFile(request, capabilities, datasetPath);
  }

  /**
   * Check if PEFT adapter supports fine-tuning
   *
   * Verifies Python environment is bootstrapped and training script exists
   */
  supportsFineTuning(): boolean {
    // Check if training script exists
    if (!this.trainingScriptExists('peft-train.py')) {
      return false;
    }

    // Check if Python environment is bootstrapped
    return this.isPythonEnvironmentBootstrapped();
  }

  /**
   * Get fine-tuning capabilities
   *
   * PEFT capabilities (local Python training with standard PyTorch):
   * - LoRA rank: 8-256 (default: 32)
   * - Epochs: 1-100 (default: 3)
   * - No API costs (local training, electricity only)
   * - GPU recommended (NVIDIA or Apple Silicon)
   * - Universal compatibility (MPS, CUDA, CPU)
   * - Supports latest models (Llama 4, DeepSeek-R1, Qwen3, Gemma 3, Phi-4)
   */
  getFineTuningCapabilities(): FineTuningCapabilities {
    return {
      supportsFineTuning: this.supportsFineTuning(),
      strategy: this.getFineTuningStrategy(),

      // LoRA parameters
      minRank: 8,
      maxRank: 256,
      defaultRank: 32,
      minAlpha: 8,
      maxAlpha: 256,
      defaultAlpha: 32,

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

      // Cost (free for local training)
      costPerExample: 0,

      // Performance (standard PyTorch/PEFT)
      estimatedTrainingTime: 25, // 25ms per example per epoch (GPU estimate)

      // Model support (PEFT supports any HuggingFace transformers model)
      // Includes both legacy short names and their HuggingFace equivalents
      // Validation is disabled - any transformers model works
      supportedBaseModels: undefined, // Accept any model - PEFT supports all transformers models

      // Requirements
      requiresGPU: true,
      requiresInternet: false // After model downloaded
    };
  }

  /**
   * Train LoRA adapter
   *
   * Phase 7.1: Call PEFT training via Python subprocess for local training
   *
   * @param request Training configuration
   * @returns Training result with adapter location
   */
  async trainLoRA(request: LoRATrainingRequest): Promise<LoRATrainingResult> {
    // Validate request first
    this.validateRequest(request);

    const startTime = Date.now();

    // Map short model name to HuggingFace (PEFT requires HF model names)
    const hfModelName = this.mapModelName(request.baseModel);
    const wasRemapped = hfModelName !== request.baseModel;

    const useQLoRA = request.quantize ?? true;
    const qloraBits = request.quantizeBits ?? 4;

    this.log('info', `Starting PEFT LoRA training: model=${request.baseModel}${wasRemapped ? ` → ${hfModelName}` : ''}, QLoRA=${useQLoRA ? `${qloraBits}-bit` : 'off'}, examples=${request.dataset.examples.length}, epochs=${request.epochs}`);

    // Update request with HuggingFace model name
    const mappedRequest = { ...request, baseModel: hfModelName };

    // 1. Export dataset to JSONL first (need path for config)
    const datasetPath = await this.exportDatasetToJSONL(request.dataset);

    // 2. Create config JSON with real dataset path
    const capabilities = this.getFineTuningCapabilities();
    const configPath = await this.createConfigFile(mappedRequest, capabilities, datasetPath);

    // 3. Create output directory
    const outputDir = path.join(os.tmpdir(), `jtag-training-${Date.now()}`);
    await fs.promises.mkdir(outputDir, { recursive: true });

    try {
      // 4. Execute Python training script via Rust sentinel (process isolation + management)
      const metrics = await this.executePythonScript('peft-train.py', configPath, outputDir);

      const trainingTime = Date.now() - startTime;
      const epochs = request.epochs ?? 3;

      // 5. Build training metadata for manifest
      const trainingMetadata = {
        epochs,
        loss: metrics.finalLoss,
        performance: 0,
        trainingDuration: trainingTime,
        datasetHash: `examples:${request.dataset.examples.length}`,
      };

      // 6. Copy adapter to genome storage with manifest (using base class helper)
      const { adapterPath, manifest } = await this.saveAdapter(request, outputDir, trainingMetadata);

      this.log('info', `Training complete in ${(trainingTime / 1000).toFixed(2)}s, adapter=${adapterPath}, sentinel=${metrics.handle}`);

      return {
        success: true,
        modelPath: adapterPath,
        manifest,
        sentinelHandle: metrics.handle,
        metrics: {
          trainingTime,
          finalLoss: metrics.finalLoss,
          examplesProcessed: request.dataset.examples.length,
          epochs,
        }
      };
    } finally {
      // Cleanup temp files
      await this.cleanupTempFiles(configPath, datasetPath);
    }
  }

  /**
   * Check training status via Rust sentinel handle.
   *
   * For PEFT local training, the sessionId IS the sentinel handle.
   * In async mode, GenomeTrainServerCommand stores the handle and callers
   * pass it here to query progress. In sync mode, this is never called
   * (trainLoRA blocks until completion).
   */
  async checkStatus(sessionId: UUID): Promise<TrainingStatus> {
    const rustClient = RustCoreIPCClient.getInstance();

    try {
      const result = await rustClient.sentinelStatus(sessionId);
      const sentinelStatus = result.handle.status;

      // Map sentinel status → TrainingStatus
      const statusMap: Record<string, TrainingStatus['status']> = {
        'running': 'running',
        'completed': 'completed',
        'failed': 'failed',
        'cancelled': 'cancelled',
      };

      return {
        status: statusMap[sentinelStatus] ?? 'failed',
        progress: result.handle.progress != null ? result.handle.progress / 100 : undefined,
        modelId: sentinelStatus === 'completed' ? sessionId : undefined,
        error: result.handle.error,
        metadata: {
          sentinelHandle: sessionId,
          exitCode: result.handle.exitCode,
          logsDir: result.handle.logsDir,
        },
      };
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);

      // Handle not found = training never started or already cleaned up
      return {
        status: 'failed',
        error: `Sentinel handle not found: ${message}`,
        metadata: { sentinelHandle: sessionId },
      };
    }
  }

  /**
   * Get training strategy (local Python with PEFT/PyTorch)
   */
  getFineTuningStrategy(): FineTuningStrategy {
    return 'local-pytorch'; // PEFT uses PyTorch under the hood
  }

  /**
   * Estimate training cost (free for local training)
   */
  estimateTrainingCost(_exampleCount: number): number {
    // Local training is free (ignoring electricity costs)
    return 0;
  }

  /**
   * Estimate training time
   *
   * Assumptions:
   * - PEFT GPU training: ~25ms per example per epoch (MPS/CUDA)
   * - PEFT CPU training: ~250ms per example per epoch (10x slower, not recommended)
   */
  estimateTrainingTime(exampleCount: number, epochs: number): number {
    const capabilities = this.getFineTuningCapabilities();

    // Use GPU estimate from capabilities
    if (capabilities.estimatedTrainingTime) {
      return exampleCount * epochs * capabilities.estimatedTrainingTime;
    }

    // Fallback: Conservative estimate
    return exampleCount * epochs * 25; // 25ms per example per epoch (GPU)
  }

}
