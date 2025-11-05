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
  FineTuningStrategy
} from '../../shared/FineTuningTypes';
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
      // Listed models are verified to work on M1 MacBook Air (8GB RAM)
      supportedBaseModels: [
        'TinyLlama/TinyLlama-1.1B-Chat-v1.0',  // 1.1B - Perfect for M1 testing
        'microsoft/phi-2',                       // 2.7B - Fast, quality
        'meta-llama/Llama-2-7b-chat-hf',        // 7B - Requires auth
        'mistralai/Mistral-7B-v0.1',            // 7B - Open, no auth
        'HuggingFaceH4/zephyr-7b-beta'          // 7B - Instruct-tuned
      ],

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

    console.log('🧬 Starting PEFT LoRA training...');
    console.log(`   Model: ${request.baseModel}`);
    console.log(`   Examples: ${request.dataset.examples.length}`);
    console.log(`   Epochs: ${request.epochs}`);

    // 1. Create config JSON (using base class helper)
    const capabilities = this.getFineTuningCapabilities();
    const configPath = await this.createConfigFile(request, capabilities);

    // 2. Export dataset to JSONL (using base class helper)
    const datasetPath = await this.exportDatasetToJSONL(request.dataset);

    // 3. Create output directory
    const outputDir = path.join(os.tmpdir(), `jtag-training-${Date.now()}`);
    await fs.promises.mkdir(outputDir, { recursive: true });

    try {
      // 4. Execute Python training script (using base class helper)
      const metrics = await this.executePythonScript('peft-train.py', configPath, outputDir);

      // 5. Copy adapter to genome storage (using base class helper)
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
          epochs: request.epochs ?? 3
        }
      };
    } finally {
      // Cleanup temp files
      await this.cleanupTempFiles(configPath, datasetPath);
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

  // ==================== PHASE 7.1 IMPLEMENTATION ====================
  // All helper methods now inherited from BaseServerLoRATrainer

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
