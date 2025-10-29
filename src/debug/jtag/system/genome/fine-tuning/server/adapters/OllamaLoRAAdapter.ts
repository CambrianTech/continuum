/**
 * OllamaLoRAAdapter - Local llama.cpp fine-tuning adapter (MVP stub)
 *
 * Phase 7.0 MVP: Stub implementation with interface structure
 * Phase 7.1+: Full llama.cpp integration for local LoRA training
 *
 * Philosophy: "Start simple, expand systematically"
 * - MVP: Interface structure, capabilities reporting
 * - Later: Actual llama.cpp training via Ollama API
 *
 * LOCAL TRAINING STRATEGY:
 * - Uses llama.cpp via Ollama for local LoRA training
 * - No API costs, fully local and private
 * - Requires GPU for reasonable performance
 * - Adapter files stored locally (.gguf format)
 *
 * SERVER-ONLY: Uses Node.js for file system and process spawning
 */

import { BaseLoRATrainer } from '../../shared/BaseLoRATrainer';
import type {
  LoRATrainingRequest,
  LoRATrainingResult,
  FineTuningCapabilities,
  FineTuningStrategy
} from '../../shared/FineTuningTypes';

/**
 * Ollama LoRA Adapter - Local llama.cpp training
 *
 * MVP Status: Stub implementation (interface only)
 * Full Implementation: Phase 7.1+
 */
export class OllamaLoRAAdapter extends BaseLoRATrainer {
  readonly providerId = 'ollama';

  /**
   * Check if Ollama supports fine-tuning
   *
   * MVP: Returns false (not implemented yet)
   * Phase 7.1+: Check if llama.cpp is available and has GPU
   */
  supportsFineTuning(): boolean {
    // MVP: Not yet implemented
    return false;

    // TODO Phase 7.1: Check for llama.cpp and GPU availability
    // return this.checkLlamaCppAvailable() && this.checkGPUAvailable();
  }

  /**
   * Get fine-tuning capabilities
   *
   * Ollama capabilities (local llama.cpp):
   * - LoRA rank: 8-256 (default: 32)
   * - Epochs: 1-100 (default: 3)
   * - No API costs (local training)
   * - GPU recommended for performance
   * - Supports any model that Ollama can load
   */
  getFineTuningCapabilities(): FineTuningCapabilities {
    return {
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

      // Performance
      estimatedTrainingTime: 50, // 50ms per example per epoch (GPU)

      // Model support (any model Ollama can load)
      supportedBaseModels: undefined, // undefined = all models supported

      // Requirements
      requiresGPU: true,
      requiresInternet: false
    };
  }

  /**
   * Train LoRA adapter
   *
   * MVP: Throws "not implemented" error
   * Phase 7.1+: Call llama.cpp via Ollama API for local training
   *
   * @param request Training configuration
   * @returns Training result with adapter location
   */
  async trainLoRA(request: LoRATrainingRequest): Promise<LoRATrainingResult> {
    // Validate request first
    this.validateRequest(request);

    // MVP: Not implemented yet
    throw new Error(
      'Ollama LoRA training not implemented yet (Phase 7.0 MVP). ' +
      'Full implementation in Phase 7.1+ will use llama.cpp for local training.'
    );

    // TODO Phase 7.1: Implement local llama.cpp training
    // const result = await this.trainWithLlamaCpp(request);
    // return result;
  }

  /**
   * Get training strategy (local llama.cpp)
   */
  getFineTuningStrategy(): FineTuningStrategy {
    return 'local-llama-cpp';
  }

  /**
   * Estimate training cost (free for local training)
   */
  estimateTrainingCost(exampleCount: number): number {
    // Local training is free (ignoring electricity costs)
    return 0;
  }

  /**
   * Estimate training time
   *
   * Assumptions:
   * - GPU training: ~50ms per example per epoch
   * - CPU training: ~500ms per example per epoch (10x slower)
   */
  estimateTrainingTime(exampleCount: number, epochs: number): number {
    const capabilities = this.getFineTuningCapabilities();

    // Use GPU estimate from capabilities
    if (capabilities.estimatedTrainingTime) {
      return exampleCount * epochs * capabilities.estimatedTrainingTime;
    }

    // Fallback: Conservative estimate
    return exampleCount * epochs * 50; // 50ms per example per epoch (GPU)
  }

  // ==================== FUTURE IMPLEMENTATION (Phase 7.1+) ====================

  /**
   * TODO Phase 7.1: Train LoRA adapter with llama.cpp
   *
   * Implementation steps:
   * 1. Export dataset to JSONL
   * 2. Call ollama create with fine-tuning parameters
   * 3. Monitor training progress
   * 4. Save adapter to local path
   * 5. Return result with metrics
   *
   * @private
   */
  /*
  private async trainWithLlamaCpp(request: LoRATrainingRequest): Promise<LoRATrainingResult> {
    const startTime = Date.now();

    // 1. Export dataset to temp file
    const datasetPath = await this.exportDatasetToJSONL(request.dataset);

    // 2. Prepare llama.cpp command
    const command = this.buildLlamaCppCommand(request, datasetPath);

    // 3. Execute training
    const metrics = await this.executeLlamaCppTraining(command);

    // 4. Save adapter
    const adapterPath = await this.saveAdapter(request, metrics);

    // 5. Clean up temp files
    await this.cleanupTempFiles(datasetPath);

    const trainingTime = Date.now() - startTime;

    return {
      success: true,
      adapterPath,
      baseModel: request.baseModel,
      traitType: request.traitType,
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
   * TODO Phase 7.1: Check if llama.cpp is available
   *
   * @private
   */
  /*
  private checkLlamaCppAvailable(): boolean {
    // Check if ollama is installed and llama.cpp is available
    // exec('ollama --version') or similar
    return false; // Stub
  }
  */

  /**
   * TODO Phase 7.1: Check if GPU is available
   *
   * @private
   */
  /*
  private checkGPUAvailable(): boolean {
    // Check for CUDA/Metal/ROCm availability
    // Platform-specific detection
    return false; // Stub
  }
  */

  /**
   * TODO Phase 7.1: Export dataset to JSONL file
   *
   * @private
   */
  /*
  private async exportDatasetToJSONL(dataset: TrainingDataset): Promise<string> {
    const tempPath = path.join(os.tmpdir(), `jtag-training-${Date.now()}.jsonl`);
    const jsonl = TrainingDatasetBuilder.exportToJSONL(dataset);
    await fs.promises.writeFile(tempPath, jsonl, 'utf-8');
    return tempPath;
  }
  */

  /**
   * TODO Phase 7.1: Build llama.cpp training command
   *
   * @private
   */
  /*
  private buildLlamaCppCommand(request: LoRATrainingRequest, datasetPath: string): string {
    const rank = request.rank || this.getFineTuningCapabilities().defaultRank;
    const alpha = request.alpha || this.getFineTuningCapabilities().defaultAlpha;
    const epochs = request.epochs || this.getFineTuningCapabilities().defaultEpochs;
    const learningRate = request.learningRate || this.getFineTuningCapabilities().defaultLearningRate;

    return `ollama create ${request.baseModel}-lora ` +
           `--from ${request.baseModel} ` +
           `--adapter lora ` +
           `--rank ${rank} ` +
           `--alpha ${alpha} ` +
           `--epochs ${epochs} ` +
           `--learning-rate ${learningRate} ` +
           `--data ${datasetPath}`;
  }
  */

  /**
   * TODO Phase 7.1: Execute llama.cpp training
   *
   * @private
   */
  /*
  private async executeLlamaCppTraining(command: string): Promise<TrainingMetrics> {
    // Execute command, monitor output, extract metrics
    // Use child_process.spawn() for real-time progress
    return {
      finalLoss: 0.5,
      trainingSteps: 100,
      examplesProcessed: 50
    };
  }
  */

  /**
   * TODO Phase 7.1: Save trained adapter
   *
   * @private
   */
  /*
  private async saveAdapter(request: LoRATrainingRequest, metrics: TrainingMetrics): Promise<string> {
    // Copy adapter from ollama models dir to genome storage
    const adapterPath = path.join(
      '.continuum/genome/adapters',
      `${request.baseModel}-${request.traitType}-${Date.now()}.gguf`
    );

    // Ensure directory exists
    await fs.promises.mkdir(path.dirname(adapterPath), { recursive: true });

    // Copy adapter file
    // await fs.promises.copyFile(sourceAdapterPath, adapterPath);

    return adapterPath;
  }
  */

  /**
   * TODO Phase 7.1: Clean up temporary files
   *
   * @private
   */
  /*
  private async cleanupTempFiles(datasetPath: string): Promise<void> {
    try {
      await fs.promises.unlink(datasetPath);
    } catch (error) {
      console.warn(`Failed to clean up temp file: ${datasetPath}`, error);
    }
  }
  */
}
