/**
 * UnslothLoRAAdapter - Local Unsloth fine-tuning adapter (Phase 7.1)
 *
 * Philosophy: "Start simple, expand systematically"
 * - Phase 7.1: Basic Unsloth integration with simplest models
 * - Phase 7.2+: Multi-model support, optimization, quality improvements
 *
 * LOCAL TRAINING STRATEGY:
 * - Uses Unsloth via Python subprocess for local LoRA training
 * - 2x faster training, 70% less VRAM than traditional methods
 * - No API costs, fully local and private
 * - Supports latest models: Llama 4, DeepSeek-R1, Qwen3, Gemma 3, Phi-4
 * - Adapter files exported to GGUF format for Ollama serving
 *
 * SERVER-ONLY: Uses Node.js for file system and process spawning
 */

import { BaseLoRATrainer } from '../../shared/BaseLoRATrainer';
import { TrainingDatasetBuilder } from '../TrainingDatasetBuilder';
import type {
  LoRATrainingRequest,
  LoRATrainingResult,
  FineTuningCapabilities,
  FineTuningStrategy,
  TrainingDataset
} from '../../shared/FineTuningTypes';
import { spawn } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

/**
 * Unsloth LoRA Adapter - Local Python-based training with Unsloth
 *
 * Current Status: MVP stub (interface only)
 * Full Implementation: Phase 7.1+
 */
export class UnslothLoRAAdapter extends BaseLoRATrainer {
  readonly providerId = 'unsloth';

  /**
   * Check if Unsloth supports fine-tuning
   *
   * Phase 7.1: Check if Python + Unsloth training script available
   */
  supportsFineTuning(): boolean {
    // Check if Python training script exists
    const scriptPath = path.join(__dirname, 'scripts', 'unsloth-train.py');
    return fs.existsSync(scriptPath);
  }

  /**
   * Get fine-tuning capabilities
   *
   * Unsloth capabilities (local Python training):
   * - LoRA rank: 8-256 (default: 32)
   * - Epochs: 1-100 (default: 3)
   * - No API costs (local training, electricity only)
   * - GPU recommended (NVIDIA or Apple Silicon)
   * - 2x faster than traditional PyTorch
   * - 70% less VRAM usage
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

      // Performance (Unsloth is 2x faster than traditional PyTorch)
      estimatedTrainingTime: 25, // 25ms per example per epoch (GPU, Unsloth optimized)

      // Model support (Unsloth supports latest models)
      supportedBaseModels: [
        'unsloth/Llama-4-8b',
        'unsloth/DeepSeek-R1-7b',
        'unsloth/Qwen3-7b',
        'unsloth/Gemma-3-8b',
        'unsloth/Phi-4'
      ],

      // Requirements
      requiresGPU: true,
      requiresInternet: false // After model downloaded
    };
  }

  /**
   * Train LoRA adapter
   *
   * Phase 7.1: Call Unsloth via Python subprocess for local training
   *
   * @param request Training configuration
   * @returns Training result with adapter location
   */
  async trainLoRA(request: LoRATrainingRequest): Promise<LoRATrainingResult> {
    // Validate request first
    this.validateRequest(request);

    const startTime = Date.now();

    console.log('🧬 Starting Unsloth LoRA training...');
    console.log(`   Model: ${request.baseModel}`);
    console.log(`   Examples: ${request.dataset.examples.length}`);
    console.log(`   Epochs: ${request.epochs}`);

    // 1. Create config JSON
    const configPath = await this.createConfigFile(request);

    // 2. Export dataset to JSONL
    const datasetPath = await this.exportDatasetToJSONL(request.dataset);

    // 3. Create output directory
    const outputDir = path.join(os.tmpdir(), `jtag-training-${Date.now()}`);
    await fs.promises.mkdir(outputDir, { recursive: true });

    try {
      // 4. Execute Python training script
      const metrics = await this.executeUnslothTraining(configPath, outputDir);

      // 5. Copy adapter to genome storage
      const adapterPath = await this.saveAdapter(request, outputDir);

      const trainingTime = Date.now() - startTime;

      console.log(`✅ Training complete in ${(trainingTime / 1000).toFixed(2)}s`);
      console.log(`   Adapter saved to: ${adapterPath}`);

      return {
        success: true,
        modelPath: adapterPath,
        metrics: {
          trainingTime,
          finalLoss: metrics.finalLoss,
          examplesProcessed: request.dataset.examples.length,
          epochs: request.epochs || 3
        }
      };
    } finally {
      // Cleanup temp files
      await this.cleanupTempFiles(configPath, datasetPath);
    }
  }

  /**
   * Get training strategy (local Python with Unsloth)
   */
  getFineTuningStrategy(): FineTuningStrategy {
    return 'local-pytorch'; // Unsloth uses PyTorch under the hood
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
   * - Unsloth GPU training: ~25ms per example per epoch (2x faster than traditional)
   * - Unsloth CPU training: ~250ms per example per epoch (10x slower, not recommended)
   */
  estimateTrainingTime(exampleCount: number, epochs: number): number {
    const capabilities = this.getFineTuningCapabilities();

    // Use GPU estimate from capabilities (Unsloth optimized)
    if (capabilities.estimatedTrainingTime) {
      return exampleCount * epochs * capabilities.estimatedTrainingTime;
    }

    // Fallback: Conservative estimate
    return exampleCount * epochs * 25; // 25ms per example per epoch (GPU, Unsloth)
  }

  // ==================== PHASE 7.1 IMPLEMENTATION ====================

  /**
   * Create config JSON file for Python script
   *
   * @private
   */
  private async createConfigFile(request: LoRATrainingRequest): Promise<string> {
    const capabilities = this.getFineTuningCapabilities();

    const config = {
      baseModel: request.baseModel,
      datasetPath: '', // Will be set by Python script
      rank: request.rank || capabilities.defaultRank,
      alpha: request.alpha || capabilities.defaultAlpha,
      epochs: request.epochs || capabilities.defaultEpochs,
      learningRate: request.learningRate || capabilities.defaultLearningRate,
      batchSize: request.batchSize || capabilities.defaultBatchSize,
      outputDir: '' // Will be set by Python script
    };

    const configPath = path.join(os.tmpdir(), `jtag-config-${Date.now()}.json`);
    await fs.promises.writeFile(configPath, JSON.stringify(config, null, 2), 'utf-8');

    console.log(`   Config written to: ${configPath}`);
    return configPath;
  }

  /**
   * Export dataset to JSONL file
   *
   * @private
   */
  private async exportDatasetToJSONL(dataset: TrainingDataset): Promise<string> {
    const tempPath = path.join(os.tmpdir(), `jtag-training-${Date.now()}.jsonl`);
    const jsonl = TrainingDatasetBuilder.exportToJSONL(dataset);
    await fs.promises.writeFile(tempPath, jsonl, 'utf-8');

    console.log(`   Dataset exported to: ${tempPath}`);
    return tempPath;
  }

  /**
   * Execute Python training script
   *
   * @private
   */
  private async executeUnslothTraining(configPath: string, outputDir: string): Promise<{ finalLoss: number }> {
    const scriptPath = path.join(__dirname, 'scripts', 'unsloth-train.py');

    console.log(`   Executing: python3 ${scriptPath}`);
    console.log(`   Config: ${configPath}`);
    console.log(`   Output: ${outputDir}`);

    return new Promise((resolve, reject) => {
      const python = spawn('python3', [scriptPath, '--config', configPath, '--output', outputDir]);

      let stdout = '';
      let stderr = '';
      let finalLoss = 0.5; // Default

      python.stdout.on('data', (data: Buffer) => {
        const text = data.toString();
        stdout += text;
        process.stdout.write(text); // Stream to console

        // Parse final loss from output
        const lossMatch = text.match(/Final loss: ([\d.]+)/);
        if (lossMatch) {
          finalLoss = parseFloat(lossMatch[1]);
        }
      });

      python.stderr.on('data', (data: Buffer) => {
        const text = data.toString();
        stderr += text;
        process.stderr.write(text); // Stream to console
      });

      python.on('close', (code: number | null) => {
        if (code === 0) {
          console.log(`   Training script completed successfully`);
          resolve({ finalLoss });
        } else {
          reject(new Error(`Training script failed with exit code ${code}\nStderr: ${stderr}`));
        }
      });

      python.on('error', (error: Error) => {
        reject(new Error(`Failed to spawn Python process: ${error.message}`));
      });
    });
  }

  /**
   * Save trained adapter to genome storage
   *
   * @private
   */
  private async saveAdapter(request: LoRATrainingRequest, outputDir: string): Promise<string> {
    // Create genome adapters directory
    const adaptersDir = path.join('.continuum', 'genome', 'adapters');
    await fs.promises.mkdir(adaptersDir, { recursive: true });

    // Create adapter subdirectory
    const adapterName = `${request.personaName.replace(/\s+/g, '-')}-${request.traitType}-${Date.now()}`;
    const adapterPath = path.join(adaptersDir, adapterName);
    await fs.promises.mkdir(adapterPath, { recursive: true });

    // Copy all adapter files from output directory
    const files = await fs.promises.readdir(outputDir);
    for (const file of files) {
      const srcPath = path.join(outputDir, file);
      const destPath = path.join(adapterPath, file);
      await fs.promises.copyFile(srcPath, destPath);
    }

    console.log(`   Adapter files copied to: ${adapterPath}`);
    return adapterPath;
  }

  /**
   * Clean up temporary files
   *
   * @private
   */
  private async cleanupTempFiles(...paths: string[]): Promise<void> {
    for (const filePath of paths) {
      try {
        const stats = await fs.promises.stat(filePath);
        if (stats.isDirectory()) {
          await fs.promises.rm(filePath, { recursive: true, force: true });
        } else {
          await fs.promises.unlink(filePath);
        }
        console.log(`   Cleaned up: ${filePath}`);
      } catch (error) {
        console.warn(`   Failed to clean up ${filePath}:`, error);
      }
    }
  }

  /**
   * TODO Phase 7.1: Create Python training script with Unsloth
   *
   * @private
   */
  /*
  private async createTrainingScript(
    request: LoRATrainingRequest,
    datasetPath: string
  ): Promise<string> {
    const rank = request.rank || this.getFineTuningCapabilities().defaultRank;
    const alpha = request.alpha || this.getFineTuningCapabilities().defaultAlpha;
    const epochs = request.epochs || this.getFineTuningCapabilities().defaultEpochs;
    const learningRate = request.learningRate || this.getFineTuningCapabilities().defaultLearningRate;

    const script = `
import os
from unsloth import FastLanguageModel
import torch

# Load model
model, tokenizer = FastLanguageModel.from_pretrained(
    model_name = "${request.baseModel}",
    max_seq_length = 2048,
    dtype = None,
    load_in_4bit = True,
)

# Add LoRA adapters
model = FastLanguageModel.get_peft_model(
    model,
    r = ${rank},
    lora_alpha = ${alpha},
    target_modules = ["q_proj", "k_proj", "v_proj", "o_proj"],
    lora_dropout = 0,
    bias = "none",
    use_gradient_checkpointing = True,
)

# Load dataset
from datasets import load_dataset
dataset = load_dataset("json", data_files="${datasetPath}")

# Training
from trl import SFTTrainer
from transformers import TrainingArguments

trainer = SFTTrainer(
    model = model,
    tokenizer = tokenizer,
    train_dataset = dataset["train"],
    dataset_text_field = "text",
    max_seq_length = 2048,
    args = TrainingArguments(
        per_device_train_batch_size = ${request.batchSize || 4},
        gradient_accumulation_steps = 4,
        warmup_steps = 5,
        num_train_epochs = ${epochs},
        learning_rate = ${learningRate},
        fp16 = not torch.cuda.is_bf16_supported(),
        bf16 = torch.cuda.is_bf16_supported(),
        logging_steps = 1,
        output_dir = "outputs",
    ),
)

# Train
trainer.train()

# Save adapter
model.save_pretrained("lora_model")
tokenizer.save_pretrained("lora_model")

print("Training complete!")
`;

    const scriptPath = path.join(os.tmpdir(), `jtag-train-${Date.now()}.py`);
    await fs.promises.writeFile(scriptPath, script, 'utf-8');
    return scriptPath;
  }
  */

  /**
   * TODO Phase 7.1: Execute Unsloth training via subprocess
   *
   * @private
   */
  /*
  private async executeUnslothTraining(scriptPath: string): Promise<TrainingMetrics> {
    // Execute Python script, monitor output, extract metrics
    // Use child_process.spawn() for real-time progress
    return {
      finalLoss: 0.5,
      trainingSteps: 100,
      examplesProcessed: 50
    };
  }
  */

  /**
   * TODO Phase 7.1: Export trained model to GGUF format
   *
   * @private
   */
  /*
  private async exportToGGUF(request: LoRATrainingRequest): Promise<string> {
    // Use llama.cpp convert script to create GGUF
    // model.save_pretrained_gguf() or llama.cpp/convert.py
    const ggufPath = path.join(
      os.tmpdir(),
      `${request.baseModel}-${request.traitType}-${Date.now()}.gguf`
    );
    return ggufPath;
  }
  */

  /**
   * TODO Phase 7.1: Save trained adapter to genome storage
   *
   * @private
   */
  /*
  private async saveAdapter(request: LoRATrainingRequest, ggufPath: string): Promise<string> {
    // Copy adapter from temp to genome storage
    const adapterPath = path.join(
      '.continuum/genome/adapters',
      `${request.baseModel}-${request.traitType}-${Date.now()}.gguf`
    );

    // Ensure directory exists
    await fs.promises.mkdir(path.dirname(adapterPath), { recursive: true });

    // Copy adapter file
    await fs.promises.copyFile(ggufPath, adapterPath);

    return adapterPath;
  }
  */

  /**
   * TODO Phase 7.1: Get Ollama model name for loading adapter
   *
   * @private
   */
  /*
  private getOllamaModelName(request: LoRATrainingRequest): string {
    return `${request.personaName}-${request.traitType}`;
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
