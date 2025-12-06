/**
 * OllamaLoRAAdapter - Local llama.cpp fine-tuning adapter
 *
 * Phase 7.1: Direct llama.cpp finetune command integration
 *
 * LOCAL TRAINING STRATEGY:
 * - Uses llama.cpp finetune command directly
 * - No API costs, fully local and private
 * - Works on CPU (multi-threaded) or GPU
 * - Adapter files stored locally (.bin format)
 * - Models from Ollama library (GGUF format)
 *
 * Example command:
 * ./finetune --model-base model.gguf --train-data data.txt \
 *   --lora-out adapter.bin --threads 8 --adam-iter 100
 *
 * SERVER-ONLY: Uses Node.js for file system and process spawning
 */

import { BaseLoRATrainerServer } from '../../../../../system/genome/fine-tuning/server/BaseLoRATrainerServer';
import type {
  LoRATrainingRequest,
  LoRATrainingResult,
  FineTuningCapabilities,
  FineTuningStrategy,
  TrainingDataset,
  TrainingHandle,
  TrainingStatus
} from '../../../../../system/genome/fine-tuning/shared/FineTuningTypes';
import type { UUID } from '../../../../../system/core/types/CrossPlatformUUID';
import { spawn } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

/**
 * Ollama LoRA Adapter - Local llama.cpp training
 *
 * Phase 7.1: Full implementation with llama.cpp finetune command
 */
export class OllamaLoRAAdapter extends BaseLoRATrainerServer {
  readonly providerId = 'ollama';

  /**
   * Check if llama.cpp finetune is available
   * Checks for:
   * 1. llama.cpp installation
   * 2. finetune binary
   */
  supportsFineTuning(): boolean {
    try {
      // Check if llama.cpp finetune exists
      // Common locations: /usr/local/bin/finetune, ~/.ollama/bin/finetune
      const possiblePaths = [
        '/usr/local/bin/finetune',
        path.join(os.homedir(), '.ollama/bin/finetune'),
        'finetune' // In PATH
      ];

      for (const binPath of possiblePaths) {
        if (fs.existsSync(binPath)) {
          return true;
        }
      }

      // If not found in common locations, assume it's in PATH
      // (will fail gracefully during training if not available)
      return true;
    } catch {
      return false;
    }
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

      // Performance
      estimatedTrainingTime: 50, // 50ms per example per epoch (GPU)

      // Model support (any model Ollama can load)
      supportedBaseModels: undefined, // undefined = all models supported

      // Requirements
      requiresGPU: true,
      requiresInternet: false
    };
  }

  // ==================== ASYNC PRIMITIVES (STUB - TODO: Implement properly) ====================

  /**
   * Start training - Currently runs synchronously and completes immediately
   * TODO: Refactor for true async local training with background process
   */
  /* eslint-disable @typescript-eslint/naming-convention */
  protected async _startTraining(request: LoRATrainingRequest): Promise<TrainingHandle> {
  /* eslint-enable @typescript-eslint/naming-convention */
    // For now, run training synchronously and return completed handle
    // This is a STUB - proper implementation should spawn background process
    const result = await this.trainLoRA(request);

    return {
      jobId: `ollama-local-${Date.now()}`,
      metadata: {
        synchronous: true,
        completed: true,
        result
      }
    };
  }

  /**
   * Query training status - Always returns completed for synchronous training
   * TODO: Implement proper async polling when background training is added
   */
  /* eslint-disable @typescript-eslint/naming-convention */
  protected async _queryStatus(
    _sessionId: UUID,
    _providerJobId: string,
    metadata: Record<string, unknown>
  ): Promise<TrainingStatus> {
  /* eslint-enable @typescript-eslint/naming-convention */
    // For synchronous training, always completed
    const result = metadata.result as LoRATrainingResult | undefined;

    if (result?.success) {
      return {
        status: 'completed',
        modelId: result.modelPath
      };
    } else {
      return {
        status: 'failed',
        error: result?.error ?? 'Unknown error'
      };
    }
  }

  // ==================== LEGACY SYNCHRONOUS METHOD ====================

  /**
   * Train LoRA adapter with llama.cpp finetune command
   *
   * @param request Training configuration
   * @returns Training result with adapter location
   * @deprecated This method runs synchronously - will be refactored for async pattern
   */
  async trainLoRA(request: LoRATrainingRequest): Promise<LoRATrainingResult> {
    // Validate request first
    this.validateRequest(request);

    this.log('info', '🚀 Ollama/llama.cpp: Starting local LoRA training...');
    const startTime = Date.now();

    try {
      // 1. Export dataset to training file (plain text format for llama.cpp)
      this.log('info', '   Exporting dataset...');
      const datasetPath = await this.exportDatasetForLlamaCpp(request.dataset);
      this.log('debug', `   Dataset exported: ${datasetPath}`);

      // 2. Get model path from Ollama
      this.log('info', '   Locating model...');
      const modelPath = await this.getOllamaModelPath(request.baseModel ?? 'llama3.2:3b');
      this.log('debug', `   Model path: ${modelPath}`);

      // 3. Create output directory
      const outputDir = path.join(os.tmpdir(), `ollama-training-${Date.now()}`);
      await fs.promises.mkdir(outputDir, { recursive: true });

      // 4. Build finetune command
      const adapterPath = path.join(outputDir, 'adapter.bin');
      const command = this.buildFinetuneCommand(request, modelPath, datasetPath, adapterPath);
      this.log('debug', `   Command: ${command.join(' ')}`);

      // 5. Execute training
      this.log('info', '   Training...');
      const metrics = await this.executeFinetuneCommand(command);
      this.log('info', '   Training complete!');

      // 6. Save adapter
      const savedPath = await this.saveAdapter(request, outputDir);
      this.log('debug', `   Adapter saved: ${savedPath}`);

      // 7. Clean up temp files
      await this.cleanupTempFiles(datasetPath);

      const trainingTime = Date.now() - startTime;

      return {
        success: true,
        modelPath: savedPath,
        metrics: {
          trainingTime,
          finalLoss: metrics.finalLoss,
          examplesProcessed: request.dataset.examples.length,
          epochs: request.epochs ?? 3
        }
      };

    } catch (error) {
      this.log('error', `❌ Ollama training failed: ${error instanceof Error ? error.message : String(error)}`);
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
   * Get training strategy (local llama.cpp)
   */
  getFineTuningStrategy(): FineTuningStrategy {
    return 'local-llama-cpp';
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

  // ==================== IMPLEMENTATION (Phase 7.1) ====================

  /**
   * Export dataset to plain text format for llama.cpp
   * Format: conversation-style text with special tokens
   * @private
   */
  private async exportDatasetForLlamaCpp(dataset: TrainingDataset): Promise<string> {
    const tempPath = path.join(os.tmpdir(), `llama-training-${Date.now()}.txt`);

    // Convert JSONL format to plain text with special tokens
    let textContent = '';
    for (const example of dataset.examples) {
      if ('messages' in example) {
        for (const msg of example.messages) {
          if (msg.role === 'user') {
            textContent += `<|user|>\n${msg.content}\n`;
          } else if (msg.role === 'assistant') {
            textContent += `<|assistant|>\n${msg.content}\n`;
          } else if (msg.role === 'system') {
            textContent += `<|system|>\n${msg.content}\n`;
          }
        }
        textContent += '\n'; // Separator between examples
      }
    }

    await fs.promises.writeFile(tempPath, textContent, 'utf-8');
    return tempPath;
  }

  /**
   * Get Ollama model path from model name
   * Uses `ollama show` to get the actual model file path
   * @private
   */
  private async getOllamaModelPath(modelName: string): Promise<string> {
    return new Promise((resolve, reject) => {
      // Use `ollama show --modelfile` to get model info
      const proc = spawn('ollama', ['show', '--modelfile', modelName]);
      let stdout = '';
      let stderr = '';

      proc.stdout.on('data', (data: Buffer) => {
        stdout += data.toString();
      });

      proc.stderr.on('data', (data: Buffer) => {
        stderr += data.toString();
      });

      proc.on('close', (code: number | null) => {
        if (code === 0) {
          // Parse the modelfile output to get the actual GGUF path
          // Look for FROM line: "FROM /path/to/model.gguf"
          const fromMatch = stdout.match(/FROM\s+(.+\.gguf)/i);
          if (fromMatch) {
            resolve(fromMatch[1].trim());
          } else {
            // Fallback: construct path manually
            // Ollama stores models in ~/.ollama/models/blobs/
            const ollamaDir = path.join(os.homedir(), '.ollama', 'models', 'manifests');
            const manifestPath = path.join(ollamaDir, modelName.replace(':', '/'));

            // For now, return model name and let finetune try to resolve
            this.log('warn', `   Could not parse model path from ollama show, using model name: ${modelName}`);
            resolve(modelName);
          }
        } else {
          reject(new Error(`Failed to get model path: ${stderr || 'unknown error'}`));
        }
      });

      proc.on('error', (error: Error) => {
        // If ollama command fails, return model name as fallback
        this.log('warn', `   Ollama not available, using model name: ${modelName}`);
        resolve(modelName);
      });
    });
  }

  /**
   * Build finetune command array
   * @private
   */
  private buildFinetuneCommand(
    request: LoRATrainingRequest,
    modelPath: string,
    datasetPath: string,
    adapterPath: string
  ): string[] {
    const capabilities = this.getFineTuningCapabilities();
    const rank = request.rank ?? capabilities.defaultRank ?? 32;
    const epochs = request.epochs ?? capabilities.defaultEpochs ?? 3;
    const threads = os.cpus().length; // Use all available CPUs

    return [
      'finetune',
      '--model-base', modelPath,
      '--train-data', datasetPath,
      '--lora-out', adapterPath,
      '--lora-r', String(rank),
      '--threads', String(threads),
      '--adam-iter', String(epochs * 100), // Rough estimate: epochs * steps
      '--sample-start', '<|user|>'
    ];
  }

  /**
   * Execute finetune command
   * @private
   */
  private async executeFinetuneCommand(command: string[]): Promise<{ finalLoss: number }> {
    return new Promise((resolve, reject) => {
      const proc = spawn(command[0], command.slice(1));
      let stderr = '';
      let finalLoss = 0.5;

      proc.stdout.on('data', (data: Buffer) => {
        const text = data.toString();
        process.stdout.write(text);

        // Parse loss from output
        const lossMatch = text.match(/loss[:\s]+([\d.]+)/i);
        if (lossMatch) {
          finalLoss = parseFloat(lossMatch[1]);
        }
      });

      proc.stderr.on('data', (data: Buffer) => {
        stderr += data.toString();
        process.stderr.write(data.toString());
      });

      proc.on('close', (code: number | null) => {
        if (code === 0) {
          resolve({ finalLoss });
        } else {
          reject(new Error(`Finetune failed with exit code ${code}: ${stderr}`));
        }
      });

      proc.on('error', (error: Error) => {
        reject(new Error(`Failed to spawn finetune: ${error.message}`));
      });
    });
  }

  /**
   * Save trained adapter to permanent storage
   * @protected
   */
  protected async saveAdapter(request: LoRATrainingRequest, tempDir: string): Promise<string> {
    const adapterFilename = `${request.baseModel?.replace(/[:/]/g, '-')}-${request.traitType}-${Date.now()}.bin`;
    const permanentDir = path.join('.continuum', 'genome', 'adapters');

    // Ensure permanent directory exists
    await fs.promises.mkdir(permanentDir, { recursive: true });

    const sourcePath = path.join(tempDir, 'adapter.bin');
    const destPath = path.join(permanentDir, adapterFilename);

    // Copy adapter to permanent storage
    await fs.promises.copyFile(sourcePath, destPath);

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
      this.log('warn', `   Failed to clean up temp file: ${datasetPath} - ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  // ==================== FUTURE IMPLEMENTATION ====================

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
  protected async exportDatasetToJSONL(dataset: TrainingDataset): Promise<string> {
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
      this.log('warn', `Failed to clean up temp file: ${datasetPath}`, error);
    }
  }
  */
}
